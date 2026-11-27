//! Many steps undone as one: an agent's script (or batch), named after
//! it ("Agent: tighten snare").
//!
//! A song's undo is its doc's (`Bridge::undo`): one undo item for each
//! step the recorder reads into it — a quiet step (one that changed
//! nothing the doc holds) counts as one too, undone as nothing. The
//! engine's own undo blocks cannot nest (a block begun inside another
//! drops it), so a script cannot be one block there. Instead this book
//! counts the song's undo items as the recorder makes them, and keeps the
//! stretch of them a group made: `SESSION_UNDO` with the group on top
//! takes back every item of it in one call, `SESSION_REDO` brings every
//! one back.
//!
//! The count is this engine's, so it can drift from the doc's (a step
//! written outside the recorder, the doc's own 200-step cap). It is never
//! trusted alone: each item a group undo takes back must be the next the
//! doc would undo by name, or the group undo stops there — a drifted book
//! undoes one step, as an undo without it would, never a step that is not
//! the group's.
//!
//! What the person does while a group is open (a script running) is
//! inside its stretch, and is undone with it.

/// A run of a song's undo items made as one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// The undo step's name.
    pub name: String,
    /// How many undo items were below it when it began.
    pub start: usize,
    /// Each item's name, oldest first: what the doc calls it.
    pub labels: Vec<String>,
}

impl Group {
    /// How many items were below its last.
    #[must_use]
    pub fn end(&self) -> usize {
        self.start.saturating_add(self.labels.len())
    }
}

/// The most finished groups a song keeps.
const KEPT: usize = 32;

/// One song's undo items as this engine counted them, and its groups.
#[derive(Clone, Debug, Default)]
pub struct Book {
    /// How many undo items the doc holds now (its undone ones not).
    depth: usize,
    /// The group being made, if one is.
    open: Option<Group>,
    /// Groups made, oldest first, whose items are all still on the undo
    /// or the redo side.
    closed: Vec<Group>,
}

impl Book {
    /// The recorder read a step named `label` into the doc: one undo
    /// item more, and any redo after it gone (with the groups that were
    /// on it).
    pub fn recorded(&mut self, label: &str) {
        let depth = self.depth;
        self.closed.retain(|g| g.end() <= depth);
        self.depth = depth.saturating_add(1);
        if let Some(open) = self.open.as_mut() {
            open.labels.push(label.to_owned());
        }
    }

    /// From now, every item read is the group `name`'s, until
    /// [`Self::end`]. One already open is dropped, its items steps of their
    /// own: it was never ended (its agent went away), so what was made
    /// since may be the person's.
    pub fn begin(&mut self, name: &str) {
        self.drop_open();
        self.open = Some(Group {
            name: name.to_owned(),
            start: self.depth,
            labels: Vec::new(),
        });
    }

    /// The name of the group being made, if one is: what each step read
    /// meanwhile is called in the song's doc, so every device that has
    /// the song sees it as the agent's (`Agent: tighten snare`).
    #[must_use]
    pub fn open_name(&self) -> Option<&str> {
        self.open.as_ref().map(|g| g.name.as_str())
    }

    /// Whether the group `name` is being made.
    #[must_use]
    pub fn is_open(&self, name: &str) -> bool {
        self.open.as_ref().is_some_and(|g| g.name == name)
    }

    /// End the group `name`, if it is the open one: it, when it made any
    /// item (a group that changed nothing is no step).
    pub fn end(&mut self, name: &str) -> Option<Group> {
        if !self.is_open(name) {
            return None;
        }
        self.close()
    }

    /// Drop the group `name`, if it is the open one: its items stay steps
    /// of their own. Whether it was.
    pub fn abandon(&mut self, name: &str) -> bool {
        if !self.is_open(name) {
            return false;
        }
        self.open = None;
        true
    }

    fn close(&mut self) -> Option<Group> {
        let group = self.open.take().filter(|g| !g.labels.is_empty())?;
        self.closed.push(group.clone());
        if self.closed.len() > KEPT {
            self.closed.remove(0);
        }
        Some(group)
    }

    /// The group an undo would take back whole: the one whose last item
    /// is on top. None while a group is open — a script still running is
    /// undone a step at a time.
    #[must_use]
    pub fn to_undo(&self) -> Option<&Group> {
        if self.open.is_some() {
            return None;
        }
        self.closed
            .iter()
            .rev()
            .find(|g| g.end() == self.depth && !g.labels.is_empty())
    }

    /// The group a redo would bring back whole: an undone one whose first
    /// item is next.
    #[must_use]
    pub fn to_redo(&self) -> Option<&Group> {
        if self.open.is_some() {
            return None;
        }
        self.closed.iter().rev().find(|g| g.start == self.depth)
    }

    /// `n` items undone.
    pub fn undone(&mut self, n: usize) {
        self.depth = self.depth.saturating_sub(n);
        self.drop_open();
    }

    /// `n` items redone.
    pub fn redone(&mut self, n: usize) {
        self.depth = self.depth.saturating_add(n);
        self.drop_open();
    }

    /// An undo, a redo or a new group while a group is open: what the
    /// group made is steps of their own from here.
    fn drop_open(&mut self) {
        if let Some(g) = self.open.take() {
            tracing::warn!(
                undo.group = %g.name,
                undo.steps = g.labels.len(),
                "an agent's step was left unfinished: its edits stay steps of their own"
            );
        }
    }
}

/// Which way, and the names the doc must give, in order, for a group's
/// items to be taken back (`back`) or brought back.
#[must_use]
pub fn plan(group: &Group, back: bool) -> Vec<String> {
    if back {
        group.labels.iter().rev().cloned().collect()
    } else {
        group.labels.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(book: &mut Book, labels: &[&str]) {
        for l in labels {
            book.recorded(l);
        }
    }

    #[test]
    fn a_group_on_top_is_undone_and_redone_whole() {
        let mut book = Book::default();
        steps(&mut book, &["Person: fader"]);
        book.begin("Agent: tighten snare");
        steps(&mut book, &["Set block", "Set block", "Set track"]);
        let g = book.end("Agent: tighten snare").expect("a group");
        assert_eq!((g.start, g.end()), (1, 4));
        let top = book.to_undo().expect("on top").clone();
        assert_eq!(plan(&top, true), ["Set track", "Set block", "Set block"]);
        book.undone(3);
        assert!(
            book.to_undo().is_none(),
            "the person's fader is a step of its own"
        );
        assert_eq!(
            book.to_redo().map(|g| g.name.as_str()),
            Some("Agent: tighten snare")
        );
        book.redone(3);
        assert_eq!(
            book.to_undo().map(|g| g.name.as_str()),
            Some("Agent: tighten snare")
        );
    }

    #[test]
    fn a_step_after_it_is_undone_first_then_the_group() {
        let mut book = Book::default();
        book.begin("Agent: a");
        steps(&mut book, &["x", "y"]);
        book.end("Agent: a");
        book.recorded("Person: z");
        assert!(book.to_undo().is_none());
        book.undone(1);
        assert_eq!(book.to_undo().map(|g| g.name.as_str()), Some("Agent: a"));
    }

    #[test]
    fn a_new_step_over_an_undone_group_drops_it() {
        let mut book = Book::default();
        book.begin("Agent: a");
        steps(&mut book, &["x", "y"]);
        book.end("Agent: a");
        book.undone(2);
        book.recorded("Person: z");
        assert!(book.to_redo().is_none());
        book.undone(1);
        assert!(book.to_redo().is_none(), "its redo is gone");
    }

    #[test]
    fn a_group_that_changed_nothing_is_no_step_and_an_open_one_is_not_undone_whole() {
        let mut book = Book::default();
        book.begin("Agent: nothing");
        assert!(book.end("Agent: nothing").is_none());
        book.begin("Agent: running");
        book.recorded("x");
        assert!(book.to_undo().is_none());
        assert!(
            book.end("Agent: other").is_none(),
            "only its own name ends it"
        );
        assert!(book.end("Agent: running").is_some());
    }

    #[test]
    fn a_group_never_ended_is_dropped_by_the_next() {
        let mut book = Book::default();
        book.begin("Agent: crashed");
        steps(&mut book, &["x", "Person: y"]);
        book.begin("Agent: next");
        book.recorded("z");
        book.end("Agent: next");
        let top = book.to_undo().expect("the next is on top");
        assert_eq!(top.labels, ["z"], "the crashed one's steps are not in it");
        book.undone(1);
        assert!(book.to_undo().is_none(), "nor a group of their own");
    }
}
