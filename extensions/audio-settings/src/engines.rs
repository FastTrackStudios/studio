//! Both engines' audio, read and set.
//!
//! The Studio engine (the `daw` extension) keeps its output device, rate
//! and buffer itself and is asked through its `DAW_AUDIO` action. The rig
//! engine (`studio.rig`) keeps its prefs in the rig's own store and is
//! asked through its `AudioSettings` service; a change reopens its device
//! when it is open.

use guitar::proto::AudioPrefs;
use serde_json::{Value as Json, json};

use crate::{ID, LINK_KEY};

/// Which engine a change is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Studio,
    Rig,
    Both,
}

impl Target {
    /// `studio`, `rig` or `both`; nothing given is both.
    ///
    /// # Errors
    ///
    /// Another word.
    pub fn parse(name: Option<&str>) -> Result<Self, String> {
        match name.map(str::to_lowercase).as_deref() {
            None | Some("both") => Ok(Self::Both),
            Some("studio" | "daw") => Ok(Self::Studio),
            Some("rig") => Ok(Self::Rig),
            Some(other) => Err(format!("no engine {other}: studio, rig or both")),
        }
    }

    const fn studio(self) -> bool {
        matches!(self, Self::Studio | Self::Both)
    }

    const fn rig(self) -> bool {
        matches!(self, Self::Rig | Self::Both)
    }
}

/// What to change; what is `None` stays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub output_device: Option<String>,
    pub input_device: Option<String>,
    /// 0-based.
    pub input_channel: Option<u32>,
    pub sample_rate: Option<u32>,
    pub buffer_size: Option<u32>,
}

impl Change {
    /// A change from an action's arguments (`input_channel` from 1).
    ///
    /// # Errors
    ///
    /// A number that is not one a device could take.
    pub fn from_args(args: &Json) -> Result<Self, String> {
        let text = |key: &str| args.get(key).and_then(Json::as_str).map(str::to_owned);
        let number = |key: &str| -> Result<Option<u32>, String> {
            match args.get(key) {
                None | Some(Json::Null) => Ok(None),
                Some(value) => value
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .map(Some)
                    .ok_or_else(|| format!("`{key}` is a whole number, got {value}")),
            }
        };
        let input_channel = match number("input_channel")? {
            Some(0) => return Err("`input_channel` counts from 1".to_owned()),
            Some(n) => Some(n.saturating_sub(1)),
            None => None,
        };
        Ok(Self {
            output_device: text("output_device"),
            input_device: text("input_device"),
            input_channel,
            sample_rate: number("sample_rate")?,
            buffer_size: number("buffer_size")?,
        })
    }

    /// The part the Studio engine takes (it has no input), as
    /// `DAW_AUDIO`'s arguments; `None` when there is none.
    #[must_use]
    pub fn for_studio(&self) -> Option<Json> {
        let mut args = serde_json::Map::new();
        if let Some(device) = &self.output_device {
            args.insert("output_device".to_owned(), json!(device));
        }
        if let Some(rate) = self.sample_rate {
            args.insert("sample_rate".to_owned(), json!(rate));
        }
        if let Some(buffer) = self.buffer_size {
            args.insert("buffer_size".to_owned(), json!(buffer));
        }
        (!args.is_empty()).then_some(Json::Object(args))
    }

    /// The part a linked Studio engine follows the rig in: its output
    /// device and rate. The buffer stays each engine's own (the rig plays
    /// live at a small one; the Studio engine renders ahead).
    #[must_use]
    pub fn linked(&self) -> Self {
        Self {
            output_device: self.output_device.clone(),
            sample_rate: self.sample_rate,
            ..Self::default()
        }
    }

    /// The rig's prefs with this change made.
    #[must_use]
    pub fn applied(&self, mut prefs: AudioPrefs) -> AudioPrefs {
        if let Some(device) = &self.output_device {
            prefs.output_device.clone_from(device);
        }
        if let Some(device) = &self.input_device {
            prefs.input_device.clone_from(device);
        }
        if let Some(channel) = self.input_channel {
            prefs.input_channel = channel;
        }
        if let Some(rate) = self.sample_rate {
            prefs.sample_rate = rate;
        }
        if let Some(buffer) = self.buffer_size {
            prefs.buffer_size = buffer;
        }
        prefs
    }
}

/// A wire value as JSON.
fn wire<'a, T: facet::Facet<'a>>(value: &T) -> Result<Json, String> {
    let text = facet_json::to_string(value).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// The Studio engine's device and the outputs it could open.
async fn studio() -> Result<Json, String> {
    studio_rig_link::run(ID, "DAW_AUDIO", json!({})).await
}

/// The rig engine's prefs, devices and whether its audio runs.
async fn rig() -> Result<Json, String> {
    let clients = studio_rig_link::clients()
        .await
        .ok_or("the rig engine (studio.rig) does not answer")?;
    let prefs = clients
        .settings
        .prefs()
        .await
        .map_err(|e| format!("{e:?}"))?;
    let devices = clients
        .settings
        .devices()
        .await
        .map_err(|e| format!("{e:?}"))?;
    let status = clients.rig.status().await.map_err(|e| format!("{e:?}"))?;
    Ok(json!({
        "prefs": wire(&prefs)?,
        "devices": wire(&devices)?,
        "running": status.running,
        "audio_error": status.audio_error,
    }))
}

/// An engine's answer, or why it gave none.
fn answered(answer: Result<Json, String>) -> Json {
    match answer {
        Ok(state) => state,
        Err(why) => json!({ "error": why }),
    }
}

/// Whether one output is kept for both engines (the registry's setting).
pub async fn linked() -> bool {
    studio_rig_link::setting(ID, LINK_KEY)
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Both engines' audio as it is set.
pub async fn state() -> Json {
    json!({
        "studio": answered(studio().await),
        "rig": answered(rig().await),
        "link": linked().await,
    })
}

/// Makes `change` to the rig: kept, and its device reopened when open.
async fn set_rig(change: &Change) -> Result<(), String> {
    let clients = studio_rig_link::clients()
        .await
        .ok_or("the rig engine (studio.rig) does not answer")?;
    let was = clients
        .settings
        .prefs()
        .await
        .map_err(|e| format!("{e:?}"))?;
    let now = change.applied(was.clone());
    if now == was {
        return Ok(());
    }
    clients
        .settings
        .save_prefs(now)
        .await
        .map_err(|e| format!("{e:?}"))?;
    // Saved prefs are read when the device opens.
    let running = clients
        .rig
        .status()
        .await
        .map(|s| s.running)
        .unwrap_or(false);
    if running {
        clients.rig.restart().await.map_err(|e| format!("{e:?}"))?;
    }
    Ok(())
}

/// Makes `change` to the Studio engine.
async fn set_studio(change: &Change) -> Result<(), String> {
    match change.for_studio() {
        Some(args) => studio_rig_link::run(ID, "DAW_AUDIO", args)
            .await
            .map(|_| ()),
        None => Ok(()),
    }
}

/// Makes `change` to `target`. A change to the rig alone is followed by
/// the Studio engine in its output and rate while the two are linked.
///
/// # Errors
///
/// What each engine that refused said.
pub async fn set(target: Target, change: &Change) -> Result<(), String> {
    let mut problems = Vec::new();
    if target.rig()
        && let Err(why) = set_rig(change).await
    {
        problems.push(format!("rig: {why}"));
    }
    let for_studio = if target.studio() {
        Some(change.clone())
    } else if linked().await {
        Some(change.linked())
    } else {
        None
    };
    if let Some(change) = for_studio
        && let Err(why) = set_studio(&change).await
    {
        problems.push(format!("studio: {why}"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::{Change, Target};
    use guitar::proto::AudioPrefs;
    use serde_json::json;

    #[test]
    fn an_actions_arguments_are_a_change() {
        let change = Change::from_args(&json!({
            "output_device": "TF", "input_channel": 4, "sample_rate": 48000
        }));
        assert_eq!(
            change,
            Ok(Change {
                output_device: Some("TF".to_owned()),
                input_channel: Some(3),
                sample_rate: Some(48_000),
                ..Change::default()
            })
        );
        assert!(Change::from_args(&json!({ "input_channel": 0 })).is_err());
        assert!(Change::from_args(&json!({ "buffer_size": "big" })).is_err());
    }

    #[test]
    fn the_studio_engine_takes_no_input() {
        let change = Change {
            output_device: Some("TF".to_owned()),
            input_device: Some("TF".to_owned()),
            input_channel: Some(3),
            buffer_size: Some(64),
            ..Change::default()
        };
        assert_eq!(
            change.for_studio(),
            Some(json!({ "output_device": "TF", "buffer_size": 64 }))
        );
        let input_only = Change {
            input_channel: Some(1),
            ..Change::default()
        };
        assert_eq!(input_only.for_studio(), None);
    }

    #[test]
    fn a_linked_studio_engine_follows_the_output_and_rate_not_the_buffer() {
        let change = Change {
            output_device: Some("TF".to_owned()),
            sample_rate: Some(48_000),
            buffer_size: Some(64),
            input_channel: Some(3),
            ..Change::default()
        };
        assert_eq!(
            change.linked().for_studio(),
            Some(json!({ "output_device": "TF", "sample_rate": 48000 }))
        );
    }

    #[test]
    fn a_change_leaves_the_rest_of_the_rigs_prefs() {
        let was = AudioPrefs {
            input_device: "MiniFuse".to_owned(),
            output_device: "MiniFuse".to_owned(),
            sample_rate: 48_000,
            buffer_size: 64,
            phones_routing: true,
            main_out_l: 2,
            main_out_r: 3,
            ..AudioPrefs::default()
        };
        let now = Change {
            buffer_size: Some(128),
            ..Change::default()
        }
        .applied(was.clone());
        assert_eq!(now.buffer_size, 128);
        assert_eq!(
            AudioPrefs {
                buffer_size: 64,
                ..now
            },
            was
        );
    }

    #[test]
    fn an_engine_is_named_or_both() {
        assert_eq!(Target::parse(None), Ok(Target::Both));
        assert_eq!(Target::parse(Some("Rig")), Ok(Target::Rig));
        assert_eq!(Target::parse(Some("daw")), Ok(Target::Studio));
        assert!(Target::parse(Some("keys")).is_err());
    }
}
