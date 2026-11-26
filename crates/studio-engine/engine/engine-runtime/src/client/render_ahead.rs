//! The render ahead as a window shows it (the Settings page): what it is,
//! the depths offered, and the one kept in its file. Setting it, and the
//! engine taking it up, is the engine's (`studio_engine::render_ahead`).

use facet::Facet;

/// Whether the engine renders ahead, and how far, ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
pub struct RenderAhead {
    pub on: bool,
    pub depth_ms: u32,
}

impl Default for RenderAhead {
    fn default() -> Self {
        Self {
            on: true,
            depth_ms: 200,
        }
    }
}

/// The depths the Settings page offers, ms: REAPER's default among them.
pub const DEPTHS: [u32; 4] = [50, 100, 200, 400];

impl RenderAhead {
    /// The engine's prefs for it: a device opened as Session opens it.
    #[must_use]
    pub fn io_prefs(self) -> daw::standalone::audio_engine::AudioIoPrefs {
        daw::standalone::audio_engine::AudioIoPrefs {
            render_ahead_ms: self.depth_ms,
            no_render_ahead: !self.on,
            ..Default::default()
        }
    }
}

/// The render ahead as it was last kept — what a window shows, read
/// afresh: the engine that keeps it is another process.
#[must_use]
pub fn saved() -> RenderAhead {
    load().unwrap_or_default()
}

pub fn path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("render-ahead.json"))
}

pub fn load() -> Option<RenderAhead> {
    let text = std::fs::read(path()?).ok()?;
    facet_json::from_slice(&text).ok()
}
