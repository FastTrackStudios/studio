//! Screensets: named, recallable dockspaces, one per form class.
//!
//! REAPER's screensets, made for a window that is sometimes a phone held
//! upright and sometimes a desk monitor. A [`Screenset`] keeps a
//! [`Dockspace`] per [`FormClass`]; recalling it on an iPad held sideways
//! shows the sideways-iPad variant, or the nearest one it has
//! ([`FormClass::fallbacks`]).
//!
//! [`Screensets`] holds the saved sets and the *working* copy: what is on
//! screen now, edited live. Saving writes the working copy for the current
//! class back into the active set; recalling throws the working copies
//! away. Turning the device round switches to the working copy for the new
//! class without losing the edits made in the old one.
//!
//! Signal and Session are screensets of the same panels, not two apps.

use facet::Facet;
use uuid::Uuid;

use crate::dockspace::Dockspace;

/// A screenset's id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
pub struct ScreensetId(Uuid);

impl ScreensetId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// A built-in set's fixed id, so a saved active set survives restarts.
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for ScreensetId {
    fn default() -> Self {
        Self::new()
    }
}

/// The shape of the window a layout is made for.
///
/// Phone, tablet and desk as `app_shell::ui::form::FormFactor` has them
/// (sizes, not devices), with the phone and tablet split by orientation:
/// an upright phone and a sideways one want different layouts, a desk
/// window does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Facet)]
#[repr(u8)]
pub enum FormClass {
    PhonePortrait,
    PhoneLandscape,
    TabletPortrait,
    TabletLandscape,
    Desk,
}

impl FormClass {
    pub const ALL: [Self; 5] = [
        Self::PhonePortrait,
        Self::PhoneLandscape,
        Self::TabletPortrait,
        Self::TabletLandscape,
        Self::Desk,
    ];

    /// The class of a window the form classifier calls a phone (or not),
    /// touched (or not), upright (or not).
    #[must_use]
    pub const fn new(phone: bool, touch: bool, portrait: bool) -> Self {
        match (phone, touch, portrait) {
            (true, _, true) => Self::PhonePortrait,
            (true, _, false) => Self::PhoneLandscape,
            (false, true, true) => Self::TabletPortrait,
            (false, true, false) => Self::TabletLandscape,
            (false, false, _) => Self::Desk,
        }
    }

    /// True for the phone classes: edge docks become sheets.
    #[must_use]
    pub const fn is_phone(self) -> bool {
        matches!(self, Self::PhonePortrait | Self::PhoneLandscape)
    }

    /// The classes whose layout stands in, nearest first, when a set has
    /// none for this one. A phone borrows the other phone orientation
    /// before a tablet's; a tablet borrows the desk's before a phone's.
    #[must_use]
    pub const fn fallbacks(self) -> [Self; 4] {
        match self {
            Self::PhonePortrait => [
                Self::PhoneLandscape,
                Self::TabletPortrait,
                Self::TabletLandscape,
                Self::Desk,
            ],
            Self::PhoneLandscape => [
                Self::PhonePortrait,
                Self::TabletLandscape,
                Self::Desk,
                Self::TabletPortrait,
            ],
            Self::TabletPortrait => [
                Self::TabletLandscape,
                Self::Desk,
                Self::PhonePortrait,
                Self::PhoneLandscape,
            ],
            Self::TabletLandscape => [
                Self::Desk,
                Self::TabletPortrait,
                Self::PhoneLandscape,
                Self::PhonePortrait,
            ],
            Self::Desk => [
                Self::TabletLandscape,
                Self::TabletPortrait,
                Self::PhoneLandscape,
                Self::PhonePortrait,
            ],
        }
    }
}

/// One class's layout in a set.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Variant {
    pub class: FormClass,
    pub space: Dockspace,
}

/// A named, recallable layout.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct Screenset {
    pub id: ScreensetId,
    pub name: String,
    /// The number key it is recalled by (REAPER's screenset slots), if any.
    pub slot: Option<u8>,
    /// Shipped with the app: "Reset" puts it back as shipped.
    pub builtin: bool,
    pub variants: Vec<Variant>,
}

impl Screenset {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: ScreensetId::new(),
            name: name.into(),
            slot: None,
            builtin: false,
            variants: Vec::new(),
        }
    }

    /// This set with `space` as its layout for `class`.
    #[must_use]
    pub fn with(mut self, class: FormClass, space: Dockspace) -> Self {
        self.set_variant(class, space);
        self
    }

    #[must_use]
    pub const fn in_slot(mut self, slot: u8) -> Self {
        self.slot = Some(slot);
        self
    }

    /// The layout made for `class` exactly.
    #[must_use]
    pub fn variant(&self, class: FormClass) -> Option<&Dockspace> {
        self.variants
            .iter()
            .find(|v| v.class == class)
            .map(|v| &v.space)
    }

    /// The layout to show on `class`: its own, else the nearest stand-in.
    #[must_use]
    pub fn for_class(&self, class: FormClass) -> Option<&Dockspace> {
        std::iter::once(class)
            .chain(class.fallbacks())
            .find_map(|c| self.variant(c))
    }

    pub fn set_variant(&mut self, class: FormClass, space: Dockspace) {
        match self.variants.iter_mut().find(|v| v.class == class) {
            Some(v) => v.space = space,
            None => self.variants.push(Variant { class, space }),
        }
    }
}

/// The saved sets, which one is active, and the working copies on screen.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
pub struct Screensets {
    pub sets: Vec<Screenset>,
    pub active: Option<ScreensetId>,
    /// What is on screen, per class, edited live since the last recall.
    pub working: Vec<Variant>,
}

impl Screensets {
    #[must_use]
    pub fn new(sets: Vec<Screenset>) -> Self {
        let active = sets.first().map(|s| s.id);
        Self {
            sets,
            active,
            working: Vec::new(),
        }
    }

    #[must_use]
    pub fn get(&self, id: ScreensetId) -> Option<&Screenset> {
        self.sets.iter().find(|s| s.id == id)
    }

    #[must_use]
    pub fn active_set(&self) -> Option<&Screenset> {
        self.active.and_then(|id| self.get(id))
    }

    /// The set in number slot `slot`.
    #[must_use]
    pub fn in_slot(&self, slot: u8) -> Option<&Screenset> {
        self.sets.iter().find(|s| s.slot == Some(slot))
    }

    /// The layout on screen for `class`, made from the active set the first
    /// time it is asked for.
    pub fn current(&mut self, class: FormClass) -> &mut Dockspace {
        let at = match self.working.iter().position(|v| v.class == class) {
            Some(at) => at,
            None => {
                let space = self
                    .active_set()
                    .and_then(|s| s.for_class(class))
                    .cloned()
                    .unwrap_or_default();
                self.working.push(Variant { class, space });
                self.working.len() - 1
            }
        };
        &mut self.working[at].space
    }

    /// The layout on screen for `class`, without making one: what a
    /// renderer reads (asking [`Screensets::current`] writes).
    #[must_use]
    pub fn shown(&self, class: FormClass) -> Option<&Dockspace> {
        self.working
            .iter()
            .find(|v| v.class == class)
            .map(|v| &v.space)
            .or_else(|| self.active_set().and_then(|s| s.for_class(class)))
    }

    /// Makes `id` the active set; the working copies start again from it.
    /// False when there is no such set.
    pub fn recall(&mut self, id: ScreensetId) -> bool {
        if self.get(id).is_none() {
            return false;
        }
        self.active = Some(id);
        self.working.clear();
        true
    }

    /// Writes what is on screen for `class` into the active set. False when
    /// no set is active.
    pub fn save(&mut self, class: FormClass) -> bool {
        let space = self.current(class).clone();
        let Some(id) = self.active else { return false };
        match self.sets.iter_mut().find(|s| s.id == id) {
            Some(set) => {
                set.set_variant(class, space);
                true
            }
            None => false,
        }
    }

    /// Saves what is on screen for `class` as a new set and makes it
    /// active.
    pub fn save_as(&mut self, name: impl Into<String>, class: FormClass) -> ScreensetId {
        let space = self.current(class).clone();
        let set = Screenset::new(name).with(class, space);
        let id = set.id;
        self.sets.push(set);
        self.active = Some(id);
        // The other classes' working copies came from the old set; the new
        // one has only this class, so they would show what it does not hold.
        self.working.retain(|v| v.class == class);
        id
    }

    /// True when what is on screen for `class` differs from what the active
    /// set would show there.
    #[must_use]
    pub fn is_dirty(&self, class: FormClass) -> bool {
        let Some(working) = self.working.iter().find(|v| v.class == class) else {
            return false;
        };
        self.active_set().and_then(|s| s.for_class(class)) != Some(&working.space)
    }

    /// Removes the set. A removed active set leaves the first set active.
    pub fn delete(&mut self, id: ScreensetId) {
        self.sets.retain(|s| s.id != id);
        if self.active == Some(id) {
            self.active = self.sets.first().map(|s| s.id);
            self.working.clear();
        }
    }

    /// Repairs every saved and working layout ([`Dockspace::repair`]):
    /// what a layout read from disk needs before it is shown.
    pub fn repair(&mut self) {
        let sets = self.sets.iter_mut().flat_map(|s| s.variants.iter_mut());
        for v in sets.chain(self.working.iter_mut()) {
            v.space.repair();
        }
        if self.active.is_some_and(|id| self.get(id).is_none()) {
            self.active = self.sets.first().map(|s| s.id);
        }
    }

    /// The sets as JSON, for `utils::prefs` or a file.
    pub fn to_json(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(facet_json::to_string(self)?)
    }

    /// Sets read back from [`Screensets::to_json`], repaired. `None` when
    /// the text is not a set list (a newer format, or damage): the caller
    /// falls back to its built-ins rather than show half a layout.
    #[must_use]
    pub fn from_json(json: &str) -> Option<Self> {
        let mut sets: Self = facet_json::from_str(json).ok()?;
        sets.repair();
        Some(sets)
    }
}
