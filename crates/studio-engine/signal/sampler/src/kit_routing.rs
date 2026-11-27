//! A kit preset's note map: which pieces a MIDI note plays.
//!
//! Pure data, so it builds everywhere: the desktop's per-track kit
//! ([`crate::kit_tracks`]) and a page's kit voice both route by it.

use std::collections::HashMap;

/// One routed target of a MIDI note.
#[derive(Clone, Debug)]
pub struct RouteTarget {
    /// Engine-ref id within the preset (`"kick"`, `"hats-closed"`).
    pub piece: String,
    /// Per-route articulation override (fired ignoring key). Empty = keyed.
    pub articulation: String,
}

/// The preset's note → pieces dispatch table.
#[derive(Clone, Debug, Default)]
pub struct KitRouting {
    routes: HashMap<u8, Vec<RouteTarget>>,
}

impl KitRouting {
    #[must_use]
    pub fn from_preset(preset: &crate::preset_spec::PresetSpec) -> Self {
        let mut routes: HashMap<u8, Vec<RouteTarget>> = HashMap::new();
        for r in &preset.note_routing {
            let entry = routes.entry(r.note).or_default();
            for target in &r.targets {
                entry.push(RouteTarget {
                    piece: target.clone(),
                    articulation: r.articulation.clone(),
                });
            }
        }
        Self { routes }
    }

    /// The targets for `note`. An empty routing table routes every note to
    /// every piece (each engine's own zones filter), mirroring the bank's
    /// default-instrument behavior.
    #[must_use]
    pub fn targets(&self, note: u8) -> Option<&[RouteTarget]> {
        self.routes.get(&note).map(Vec::as_slice)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}
