//! A song's one doc, and a copy of it elsewhere kept in step with it — as
//! a CRDT, never through the engine.
//!
//! A song open in an engine has one doc and one bridge to it
//! ([`crate::sync::Bridge`], one per project: [`crate::sync::bridge::claim`]).
//! A live set keeps its own doc of the song (Task names it), and a browser
//! in the set reads and edits only that. Bridging that doc to the engine
//! too made two bridges on one project, each with its own map of which
//! doc FX is which engine FX, and they added each other's FX chains again
//! every round (#316). So the live doc is made a copy of the song's doc —
//! the same history from the moment it was seeded ([`seed`]), so the two
//! share every operation — and whatever either has that the other lacks
//! is passed across as Loro updates ([`exchange`]). An update merges as
//! the same operations in both, however often it is passed: nothing is
//! made twice. The song's bridge hears what came in as it hears any other
//! device's edit.
//!
//! Two docs that were not seeded one from the other ([`same_lineage`] says
//! no) cannot be kept in step this way: their lists would merge side by
//! side. A song meeting a live doc of another lineage is moved onto it
//! instead (its bridge switched to a copy of that doc), or left out.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use loro::{ExportMode, LoroDoc};

/// How one side takes in the bytes the other had and it lacked: an import
/// into its doc, and whatever else it must do with them (send them up its
/// connection, write them through its file).
pub type Take =
    Arc<dyn Fn(Vec<u8>) -> architect::platform::BoxFuture<'static, eyre::Result<()>> + Send + Sync>;

/// One of the two docs a relay keeps in step.
#[derive(Clone)]
pub struct Side {
    pub doc: LoroDoc,
    pub take: Take,
}

impl Side {
    /// `doc`, taking what it lacks by importing it, and nothing more.
    #[must_use]
    pub fn imported(doc: LoroDoc) -> Self {
        let into = doc.clone();
        Self {
            doc,
            take: Arc::new(move |bytes| {
                let into = into.clone();
                Box::pin(async move {
                    into.import(&bytes)?;
                    Ok(())
                })
            }),
        }
    }
}

/// Whether `a` and `b` share a history — one seeded from the other, or
/// both from a third: some peer has written into both. Two docs made
/// apart never share a peer (each writer's id is random).
#[must_use]
pub fn same_lineage(a: &LoroDoc, b: &LoroDoc) -> bool {
    let theirs = b.oplog_vv();
    a.oplog_vv()
        .iter()
        .any(|(peer, counter)| *counter > 0 && theirs.get(peer).is_some_and(|c| *c > 0))
}

/// What seeds an empty doc as a copy of `from`.
///
/// It is `from`'s state now, with none of the history before it (a
/// long-edited song's history weighs far more than the song, and a first
/// sync is one message), on the same operations — so what either side
/// writes after merges into the other.
///
/// # Errors
/// When `from` cannot be exported.
pub fn seed(from: &LoroDoc) -> eyre::Result<Vec<u8>> {
    Ok(from.export(ExportMode::shallow_snapshot(&from.oplog_frontiers()))?)
}

/// What `from` holds that `to` lacks, as one update; `None` when `to` has
/// it all.
///
/// # Errors
/// When `from` cannot be exported.
pub fn missing(from: &LoroDoc, to: &LoroDoc) -> eyre::Result<Option<Vec<u8>>> {
    let theirs = to.oplog_vv();
    let ours = from.oplog_vv();
    let ahead = ours
        .iter()
        .any(|(peer, counter)| theirs.get(peer).is_none_or(|c| c < counter));
    if !ahead {
        return Ok(None);
    }
    Ok(Some(from.export(ExportMode::updates(&theirs))?))
}

/// Pass each side what the other has and it lacks. Returns how many
/// updates moved (0, 1 or 2).
///
/// # Errors
/// When a side cannot be exported or refuses what it is given.
pub async fn exchange(a: &Side, b: &Side) -> eyre::Result<usize> {
    let to_b = missing(&a.doc, &b.doc)?;
    let moved_to_b = to_b.is_some();
    if let Some(bytes) = to_b {
        (b.take)(bytes).await?;
    }
    let to_a = missing(&b.doc, &a.doc)?;
    let moved_to_a = to_a.is_some();
    if let Some(bytes) = to_a {
        (a.take)(bytes).await?;
    }
    Ok(usize::from(moved_to_b) + usize::from(moved_to_a))
}

/// Two docs of one song kept in step for as long as it lives.
///
/// Whenever either changes, each is passed what the other has. While `locked`
/// (this device's "don't sync"), nothing is passed: each keeps its own
/// edits, and they are passed both ways when it is unlocked.
pub struct Relay {
    stop: tokio::sync::watch::Sender<bool>,
    held: Arc<AtomicBool>,
    _subscriptions: [loro::Subscription; 2],
}

impl Relay {
    /// Keep `song` and `live` in step from now on (passing what each has
    /// at once, unless locked).
    #[must_use]
    pub fn start(song: Side, live: Side, mut locked: tokio::sync::watch::Receiver<bool>) -> Self {
        let (changed_tx, mut changed) = tokio::sync::mpsc::unbounded_channel::<()>();
        let subscribe = |doc: &LoroDoc| {
            let tx = changed_tx.clone();
            doc.subscribe_root(Arc::new(move |_| {
                let _ = tx.send(());
            }))
        };
        let subscriptions = [subscribe(&song.doc), subscribe(&live.doc)];
        let (stop, mut stopped) = tokio::sync::watch::channel(false);
        let held = Arc::new(AtomicBool::new(false));
        let holding = Arc::clone(&held);
        architect::platform::spawn(async move {
            // Whatever either holds now, first. `pending`: a change not
            // passed yet — kept while locked, which is what it holds.
            let mut pending = true;
            loop {
                let is_locked = *locked.borrow_and_update();
                if pending && !is_locked {
                    pending = false;
                    match exchange(&song, &live).await {
                        Ok(moved) => {
                            if moved > 0 {
                                tracing::debug!(
                                    relay.moved = moved,
                                    "relay: the song's docs passed what each lacked"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(relay.error = %e, "relay: the song's docs could not be kept in step");
                        }
                    }
                }
                holding.store(pending, Ordering::Relaxed);
                tokio::select! {
                    _ = stopped.changed() => break,
                    // Locked or unlocked: looked at again at the top.
                    Ok(()) = locked.changed() => {}
                    Some(()) = changed.recv() => {
                        while changed.try_recv().is_ok() {}
                        pending = true;
                    }
                    else => break,
                }
            }
        });
        Self {
            stop,
            held,
            _subscriptions: subscriptions,
        }
    }

    /// Whether this device's lock is holding something back from either
    /// side.
    #[must_use]
    pub fn held(&self) -> bool {
        self.held.load(Ordering::Relaxed)
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

#[cfg(test)]
mod tests;
