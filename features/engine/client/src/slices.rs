//! Long work on a page's one thread, cut into slices: [`Slices`].
//!
//! A page has one thread. Work that awaits nothing but other in-process
//! work (a song built through the in-process facade, each call answered
//! as a microtask) never gives the page its turn — one song was one
//! 800 ms task, the page frozen for it. [`Slices`] says when the work
//! has had its slice and should give way: the caller then waits on a
//! timer, which ends the task, and carries on in the next.
//!
//! The work gets as long a slice as the page's last turn took, within
//! bounds: where a frame is cheap the slices stay short, and where one is
//! dear (a canvas drawn without a GPU) the work still gets half the
//! thread rather than a sliver between frames.

use std::time::Duration;

/// When work that has run its slice without a break gives way. Times are
/// any one clock's, as offsets from a fixed origin.
#[derive(Debug)]
pub struct Slices {
    /// The shortest slice, and the longest.
    least: Duration,
    most: Duration,
    /// The slice now: as long as the page's last turn, within bounds.
    slice: Duration,
    /// A pause between two steps at least this long was a wait (on the
    /// network, a timer): the page had its turn then. Generous: the time
    /// between steps also holds whatever else ran in the same task (the
    /// facade answering), and taking that for a wait would miss a turn,
    /// where taking a short wait for work only gives way once too often.
    gap: Duration,
    /// When the current slice began, if one has.
    began: Option<Duration>,
    /// When the last step ended.
    last: Option<Duration>,
    /// When the work last gave way, until it is back.
    gave_way: Option<Duration>,
}

impl Slices {
    /// Slices of `least` to `most`; a break of `gap` was a wait.
    #[must_use]
    pub const fn new(least: Duration, most: Duration, gap: Duration) -> Self {
        Self {
            least,
            most,
            slice: least,
            gap,
            began: None,
            last: None,
            gave_way: None,
        }
    }

    /// About to take a step of the work at `now`: whether to give the page
    /// its turn first. Said yes, the slice is over — the next step begins
    /// the next one.
    pub fn give_way(&mut self, now: Duration) -> bool {
        // Back from the page's turn: the next slice as long as it was.
        if let Some(gave_way) = self.gave_way.take() {
            self.slice = now.saturating_sub(gave_way).clamp(self.least, self.most);
        }
        let waited = self
            .last
            .is_none_or(|last| now.saturating_sub(last) >= self.gap);
        match self.began {
            Some(began) if !waited => {
                if now.saturating_sub(began) >= self.slice {
                    self.began = None;
                    self.last = None;
                    self.gave_way = Some(now);
                    return true;
                }
            }
            _ => self.began = Some(now),
        }
        false
    }

    /// A step of the work ended at `now`.
    pub fn stepped(&mut self, now: Duration) {
        self.last = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn fresh() -> Slices {
        Slices::new(ms(25), ms(250), ms(50))
    }

    /// Steps of `each` ms back to back from `from`, the page's turns
    /// `turn` ms long: the times it gave way.
    fn run_with(slices: &mut Slices, from: u64, steps: u64, each: u64, turn: u64) -> Vec<u64> {
        let mut gave_way = Vec::new();
        let mut now = from;
        for _ in 0..steps {
            if slices.give_way(ms(now)) {
                gave_way.push(now);
                now += turn;
                continue;
            }
            now += each;
            slices.stepped(ms(now));
        }
        gave_way
    }

    /// [`run_with`], the page's turns a few ms on the timer.
    fn run(slices: &mut Slices, from: u64, steps: u64, each: u64) -> Vec<u64> {
        run_with(slices, from, steps, each, 4)
    }

    #[test]
    fn work_without_a_break_gives_way_once_a_slice() {
        let mut slices = fresh();
        assert_eq!(run(&mut slices, 0, 19, 10), [30, 64, 98, 132]);
    }

    #[test]
    fn work_that_waited_starts_a_new_slice() {
        let mut slices = fresh();
        // 20 ms of work, a 100 ms wait on the network, 20 ms more: never
        // 25 ms without the page having had a turn.
        assert_eq!(run(&mut slices, 0, 2, 10), Vec::<u64>::new());
        assert_eq!(run(&mut slices, 120, 2, 10), Vec::<u64>::new());
    }

    #[test]
    fn a_dear_page_turn_earns_the_work_as_long_a_slice() {
        let mut slices = fresh();
        // Turns of 100 ms (frames drawn without a GPU): after the first
        // 25 ms slice, the work gets 100 ms at a time.
        assert_eq!(run_with(&mut slices, 0, 26, 10, 100), [30, 230, 430]);
        // Never more than the longest slice, however dear the turn.
        let mut slices = fresh();
        assert_eq!(run_with(&mut slices, 0, 30, 10, 1_000), [30, 1_280]);
    }

    #[test]
    fn short_steps_add_up_to_a_slice() {
        let mut slices = fresh();
        assert_eq!(run(&mut slices, 0, 30, 1), [25]);
    }
}
