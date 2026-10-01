//! What a host can bind.

/// The fields a host offers a theme, in slot order.
#[derive(Debug, Clone, Default)]
pub struct Schema {
    fields: Vec<(String, bool)>,
}

impl Schema {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A field whose value changes only when the instance is re-recorded
    /// (a track's name, colour, mute).
    #[must_use]
    pub fn field(mut self, name: &str) -> Self {
        self.fields.push((name.to_owned(), false));
        self
    }

    /// A field that changes every frame (a meter, a fader in a drag):
    /// whatever reads it is painted in the live pass.
    #[must_use]
    pub fn live(mut self, name: &str) -> Self {
        self.fields.push((name.to_owned(), true));
        self
    }

    /// The slot `name` is read from.
    #[must_use]
    pub fn slot(&self, name: &str) -> Option<u32> {
        self.fields.iter().position(|(n, _)| n == name).and_then(|i| u32::try_from(i).ok())
    }

    #[must_use]
    pub fn is_live(&self, slot: u32) -> bool {
        usize::try_from(slot).ok().and_then(|i| self.fields.get(i)).is_some_and(|(_, live)| *live)
    }

    /// Field names, in slot order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|(n, _)| n.as_str())
    }
}
