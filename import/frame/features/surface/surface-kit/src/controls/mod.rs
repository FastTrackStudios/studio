//! Kit families: buttons, switches and levers — the panel's switching.
//! Ported from `fts-audio-ui::hardware` (`button`, `button_kit`,
//! `button_parts`, `switches`, `lever`) — see `KIT.md`.

mod buttons;
pub(crate) mod parts;
mod levers;
mod switches;

use crate::registry::ComponentDef;

/// This family's components.
pub(crate) fn defs() -> Vec<ComponentDef> {
    let mut all = buttons::defs();
    all.extend(switches::defs());
    all.extend(levers::defs());
    all
}
