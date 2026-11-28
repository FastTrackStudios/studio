//! Each project behind its own lock: an edit to one song holds up no
//! reader or editor of another.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use daw_proto::{Daw, ProjectInfo};

use super::Standalone;

fn info(guid: &str) -> ProjectInfo {
    ProjectInfo {
        guid: guid.into(),
        name: guid.into(),
        path: String::new(),
    }
}

/// How long a call that should not wait is given before it counts as
/// stuck. Only a call blocked behind the held lock comes near it: the lock
/// is held until the probe lets it go, so a blocked call never returns in
/// time, however loaded the box (studio#780).
const STUCK: Duration = Duration::from_secs(30);

/// Run `op` on its own thread; `Some(answer)` if it returned within
/// [`STUCK`], `None` if it was still waiting.
fn returns<R: Send + 'static>(op: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    let (done, answer) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(op());
    });
    answer.recv_timeout(STUCK).ok()
}

/// Song A's lock is held by an editor (until it is let go), and song B's
/// readers and editors, the listing and A's revision probe all return while
/// it is; A's own reader waits for it. Ordering, not elapsed time: a load
/// spike can make a call slow, never make a blocked one return.
#[test]
fn an_edit_to_one_song_never_blocks_another() {
    let daw = Standalone::new();
    daw.seed_project(info("a"));
    daw.seed_project(info("b"));
    let (holding, held) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let editor = {
        let daw = daw.clone();
        thread::spawn(move || {
            daw.write_project("a", |p| {
                p.revision = p.revision.wrapping_add(1);
                holding.send(()).expect("the probe waits for the hold");
                // A's lock stays held until the probe lets it go.
                let _ = released.recv();
            })
            .expect("a");
        })
    };
    held.recv().expect("the editor took a");

    let d = daw.clone();
    assert!(
        returns(move || d.read_project("b", |p| p.tracks.len()).expect("b")).is_some(),
        "a read of b waited on the edit of a"
    );
    let d = daw.clone();
    assert!(
        returns(move || d.write_project("b", |p| p.master_volume = 0.5).expect("b")).is_some(),
        "an edit of b waited on the edit of a"
    );
    // The listing reads each project's published info: no wait.
    let d = daw.clone();
    assert_eq!(
        returns(move || d.projects().len()),
        Some(2),
        "listing projects waited on the edit of a"
    );
    // The renderer's revision probe never waits either.
    let d = daw.clone();
    assert!(
        returns(move || d.try_project_revision("a").expect("a's revision")).is_some(),
        "a's revision waited on the edit of a"
    );

    // And A's own reader does wait: it returns only after the editor lets go.
    let (read, a_read) = mpsc::channel();
    let reader = {
        let daw = daw.clone();
        thread::spawn(move || {
            daw.read_project("a", |p| p.revision).expect("a");
            let _ = read.send(());
        })
    };
    // Given a moment to run: a reader that did not wait would be done.
    assert!(
        a_read.recv_timeout(Duration::from_millis(200)).is_err(),
        "a's reader did not wait: the editor did not hold a"
    );
    release.send(()).expect("the editor is holding a");
    a_read
        .recv_timeout(STUCK)
        .expect("a's reader returned once the editor let go");
    editor.join().expect("editor thread");
    reader.join().expect("reader thread");
}

/// A project's revision and info, published at the end of every write,
/// read as the project holds them.
#[test]
fn a_write_publishes_revision_and_info() {
    let daw = Standalone::new();
    daw.seed_project(info("a"));
    let before = daw.try_project_revision("a").expect("a");
    daw.write_project("a", |p| p.info.name = "renamed".into());
    assert_eq!(
        daw.try_project_revision("a"),
        daw.read_project("a", |p| p.revision)
    );
    assert_ne!(daw.try_project_revision("a"), Some(before));
    assert_eq!(daw.projects()[0].name, "renamed");
}
