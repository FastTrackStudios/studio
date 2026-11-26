//! Work on a page cut into slices, the page given its turn between them
//! ([`in_slices`]): opening a song (`web_engine`) and joining a live set
//! with it (engine-collab's `collab`) share one budget.

/// How long opening a song runs before the page gets its turn
/// ([`in_slices`]), at least: under a long task (50 ms), and long enough
/// that the frames painted between slices do not double the time a song
/// takes.
const SLICE: std::time::Duration = std::time::Duration::from_millis(40);
/// And at most, however long the page's turns take (a canvas drawn
/// without a GPU took 400 ms a frame, and a song got a sliver between).
const SLICE_MOST: std::time::Duration = std::time::Duration::from_millis(250);
/// A break this long in opening a song was a wait the page had its turn
/// in ([`crate::slices::Slices`]).
const SLICE_GAP: std::time::Duration = std::time::Duration::from_millis(50);

/// The page's turns, shared by everything run [`in_slices`]: one budget,
/// so two songs being worked on at once (one opening, the one before it
/// joining the live set) give way together — and the page's turn is
/// measured with nothing of theirs in it. Each measuring its own, they
/// took each other's slices for the page's and lengthened them in turn.
struct Turns {
    origin: web_time::Instant,
    slices: crate::slices::Slices,
    /// The page's turn while it has it: the timer all sliced work waits on.
    turn: Option<futures_util::future::Shared<gloo_timers::future::TimeoutFuture>>,
}

thread_local! {
    static TURNS: std::cell::RefCell<Turns> = std::cell::RefCell::new(Turns {
        origin: web_time::Instant::now(),
        slices: crate::slices::Slices::new(SLICE, SLICE_MOST, SLICE_GAP),
        turn: None,
    });
}

/// Run `work` in slices ([`SLICE`] to [`SLICE_MOST`]), the page given its
/// turn between them: work that awaits only the in-process facade (a song
/// built through it, each call answered as a microtask) never ends the
/// task it runs in, and one song froze the page for most of a second.
/// Giving way is a zero timer, which ends the task; every sliced work
/// waits on it ([`Turns`]).
pub async fn in_slices<T>(work: impl std::future::Future<Output = T>) -> T {
    use futures_util::FutureExt as _;
    use std::future::Future as _;
    use std::task::Poll;
    let mut work = std::pin::pin!(work);
    let mut waiting: Option<futures_util::future::Shared<gloo_timers::future::TimeoutFuture>> =
        None;
    std::future::poll_fn(move |cx| {
        // The page's turn, if another's slice ended in it: wait it out.
        if waiting.is_none() {
            waiting = TURNS.with(|t| t.borrow().turn.clone());
        }
        if let Some(timer) = waiting.as_mut() {
            if std::pin::Pin::new(timer).poll(cx).is_pending() {
                return Poll::Pending;
            }
            waiting = None;
            TURNS.with(|t| {
                let mut t = t.borrow_mut();
                if t.turn.as_ref().is_some_and(|turn| turn.peek().is_some()) {
                    t.turn = None;
                }
            });
        }
        let turn = TURNS.with(|t| {
            let mut t = t.borrow_mut();
            let now = t.origin.elapsed();
            t.slices.give_way(now).then(|| {
                let timer = gloo_timers::future::TimeoutFuture::new(0).shared();
                t.turn = Some(timer.clone());
                timer
            })
        });
        if let Some(mut timer) = turn {
            // Polled once, so it wakes this task when it fires.
            let _ = std::pin::Pin::new(&mut timer).poll(cx);
            waiting = Some(timer);
            return Poll::Pending;
        }
        let polled = work.as_mut().poll(cx);
        TURNS.with(|t| {
            let mut t = t.borrow_mut();
            let now = t.origin.elapsed();
            t.slices.stepped(now);
        });
        polled
    })
    .await
}
