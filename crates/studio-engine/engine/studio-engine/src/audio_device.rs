//! The audio device the engine plays through (`SESSION_AUDIO_DEVICE`).
//!
//! Which output device, at what sample rate and buffer: yours, not the
//! song's (`~/.config/fts/audio-device.json`), as the render ahead is.
//! With nothing kept, the system's default output at its own rate and the
//! backend's buffer, as the engine always opened. A change is kept and
//! the device is opened again, the set's songs rejoining it.
//!
//! Studio's audio settings (`studio.audio-settings`) set it beside the
//! rig engine's (`docs/design/signal-extensions.md`).
//!
//! `FTS_AUDIO_DEVICE` names the output for this process only, over the
//! kept setting, and nothing this process changes is kept: a second
//! engine (a test's, an agent's) plays where it is told, `null` for no
//! device at all (`daw_standalone`'s null output, at real time), and
//! never moves the person's default.

use architect::action::{ActionList, ActionMeta, Effect};
use daw::standalone::audio_engine::AudioIoPrefs;
use facet::Facet;

/// The output the engine opens. Empty and `0` mean the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Facet)]
pub struct AudioDevice {
    /// Part of the output device's name. Empty: the system's default.
    #[facet(default)]
    pub output_device: String,
    /// Hz. `0`: the device's own.
    #[facet(default)]
    pub sample_rate: u32,
    /// Frames. `0`: the backend's default.
    #[facet(default)]
    pub buffer_size: u32,
}

impl AudioDevice {
    /// `prefs` opening this device.
    #[must_use]
    pub fn applied(&self, prefs: AudioIoPrefs) -> AudioIoPrefs {
        AudioIoPrefs {
            output_device: self.output_device.clone(),
            sample_rate: self.sample_rate,
            buffer_size: self.buffer_size,
            ..prefs
        }
    }
}

/// The process's own output, over the kept setting.
pub const ENV: &str = "FTS_AUDIO_DEVICE";

fn store() -> &'static std::sync::Mutex<AudioDevice> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<AudioDevice>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| {
        std::sync::Mutex::new(
            from_env(std::env::var(ENV).ok())
                .or_else(load)
                .unwrap_or_default(),
        )
    })
}

/// The device `ENV`'s value names, when it names one.
fn from_env(value: Option<String>) -> Option<AudioDevice> {
    let name = value.filter(|v| !v.trim().is_empty())?;
    Some(AudioDevice {
        output_device: name.trim().to_owned(),
        ..AudioDevice::default()
    })
}

/// The device as it stands.
#[must_use]
pub fn current() -> AudioDevice {
    store()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Change it, keep it, and play on through it: the device is opened
/// again. Says what it is now.
pub(crate) fn set(want: AudioDevice) -> AudioDevice {
    let changed = {
        let mut held = store()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::replace(&mut *held, want.clone()) != want
    };
    if changed {
        save(&want);
        crate::open::restart_audio();
    }
    want
}

fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("audio-device.json"))
}

fn load() -> Option<AudioDevice> {
    let text = std::fs::read(path()?).ok()?;
    facet_json::from_slice(&text).ok()
}

fn save(device: &AudioDevice) {
    // A process told its device keeps nothing: the setting is someone
    // else's.
    if from_env(std::env::var(ENV).ok()).is_some() {
        return;
    }
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    match facet_json::to_vec(device) {
        Ok(bytes) => {
            if std::fs::write(&path, bytes).is_err() {
                tracing::warn!("audio device: the setting could not be saved");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "audio device: the setting would not serialize");
        }
    }
}

static SESSION_AUDIO_DEVICE: ActionMeta = crate::actions::meta(
    "SESSION_AUDIO_DEVICE",
    "session_audio_device",
    "Audio device",
    "The output the engine plays through: `output_device` (part of its name; empty for the system's default), `sample_rate` (Hz; 0 for the device's own) and `buffer_size` (frames; 0 for the backend's default). Any left out stays as it is. Kept for every song; the audio device is opened again to take it up. Says what it is now.",
    "Audio",
    "",
);

/// The device changed: any field left out stays.
#[derive(Facet)]
struct AudioDeviceArg {
    output_device: Option<String>,
    sample_rate: Option<u32>,
    buffer_size: Option<u32>,
}

pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(
        &SESSION_AUDIO_DEVICE,
        Effect::Mutating,
        |a: AudioDeviceArg| {
            let was = current();
            Ok::<_, String>(set(AudioDevice {
                output_device: a.output_device.unwrap_or(was.output_device),
                sample_rate: a.sample_rate.unwrap_or(was.sample_rate),
                buffer_size: a.buffer_size.unwrap_or(was.buffer_size),
            }))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::AudioDevice;
    use daw::standalone::audio_engine::AudioIoPrefs;

    #[test]
    fn the_device_is_laid_over_the_engines_prefs() {
        let ahead = AudioIoPrefs {
            render_ahead_ms: 200,
            ..AudioIoPrefs::default()
        };
        let opened = AudioDevice {
            output_device: "TF".to_owned(),
            sample_rate: 48_000,
            buffer_size: 128,
        }
        .applied(ahead);
        assert_eq!(opened.output_device, "TF");
        assert_eq!(opened.sample_rate, 48_000);
        assert_eq!(opened.buffer_size, 128);
        // What the render ahead asked for stays.
        assert_eq!(opened.render_ahead_ms, 200);
        assert!(!opened.want_input);
    }

    #[test]
    fn the_environment_names_the_output_and_nothing_else() {
        assert_eq!(super::from_env(None), None);
        assert_eq!(super::from_env(Some("  ".to_owned())), None);
        assert_eq!(
            super::from_env(Some("null".to_owned())),
            Some(AudioDevice {
                output_device: "null".to_owned(),
                sample_rate: 0,
                buffer_size: 0,
            })
        );
    }

    #[test]
    fn nothing_kept_opens_the_default_as_before() {
        let opened = AudioDevice::default().applied(AudioIoPrefs::default());
        assert_eq!(opened, AudioIoPrefs::default());
    }
}
