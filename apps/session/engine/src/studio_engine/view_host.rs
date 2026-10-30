//! The engine's side of [`crate::view::EngineView`]: what a window reads
//! and asks of the engine, served from the engine's own state.
//!
//! One host per engine ([`host`]), its hubs fed by the engine as things
//! change — the set as it is installed, picked and arrives ([`start`]
//! follows `super::set`), the hit overlay as it is read again
//! ([`overlay_read`]), the tracks' marks ([`marks_changed`]), the
//! streaming state ([`streams_read`]), what changed in a song
//! ([`changed`]), what is asked of a window (`crate::window_queue::ask`) —
//! and its calls answered from the engine in this process.
//!
//! Served beside the daw facade on one router ([`router`]): over the
//! engine's socket by `studio engine`, in memory by an engine that runs in
//! its window's process (iOS, a test).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use architect::PubSub;
use daw::service::ProjectContext;

use crate::view::{
    ActionCall, Applied, AutomationRead, Change, Changed, ChangesAsk, DetailAsk, EditAsked,
    EditBatch, EditShown, EngineView, EngineViewStreamSource, ItemChanges, ItemNow, JobRow,
    JobsFrame, MarksFrame, OverlayFrame, Playhead, Queued, SetFrame, SetOpening, SongFacts,
    StreamSong, StreamsFrame, ViewError, WaveRead, WavesAsk, WindowAction, WindowAnswer,
};

/// The engine's view host: its hubs and what each last said.
#[derive(Clone)]
pub struct ViewHost {
    inner: Arc<Inner>,
}

struct Inner {
    set: PubSub<SetFrame>,
    overlays: PubSub<OverlayFrame>,
    marks: PubSub<MarksFrame>,
    streams: PubSub<StreamsFrame>,
    jobs: PubSub<JobsFrame>,
    edits: PubSub<EditAsked>,
    changes: PubSub<Changed>,
    queued: PubSub<Queued>,
    last_set: Mutex<SetFrame>,
    last_overlay: Mutex<OverlayFrame>,
    last_marks: Mutex<MarksFrame>,
    last_streams: Mutex<StreamsFrame>,
    last_jobs: Mutex<JobsFrame>,
    revision: AtomicU64,
    asked: AtomicU64,
    /// The engine's own waveform reads: what it cuts slices from.
    previews: crate::midi::Previews,
}

/// The engine's host, made on first use.
#[must_use]
pub fn host() -> &'static ViewHost {
    static HOST: OnceLock<ViewHost> = OnceLock::new();
    HOST.get_or_init(|| ViewHost {
        inner: Arc::new(Inner {
            // A level each (the latest is all that matters) but for what
            // changed, which a window must hear every one of.
            set: PubSub::sliding(16),
            overlays: PubSub::sliding(4),
            marks: PubSub::sliding(4),
            streams: PubSub::sliding(4),
            jobs: PubSub::sliding(4),
            edits: PubSub::sliding(16),
            changes: PubSub::sliding(1024),
            queued: PubSub::sliding(64),
            last_set: Mutex::new(SetFrame::default()),
            last_overlay: Mutex::new(OverlayFrame::default()),
            last_marks: Mutex::new(MarksFrame::default()),
            last_streams: Mutex::new(StreamsFrame::default()),
            last_jobs: Mutex::new(JobsFrame::default()),
            revision: AtomicU64::new(0),
            asked: AtomicU64::new(0),
            previews: crate::midi::Previews::default(),
        }),
    })
}

/// The engine's whole surface for a window: the daw facade over `daw` and
/// the view host, on one router.
#[must_use]
pub fn router(daw: &daw::standalone::Standalone) -> daw::LayerRouter {
    start();
    let host = host().clone();
    daw::Services::into_router(daw.clone())
        .merge(crate::view::layer(host.clone()))
        .merge(crate::view::stream_layer(host))
}

/// Start feeding the hubs (once per process): the set followed, the
/// windows counted, what is asked told.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        crate::window_queue::on_ask(|pending| {
            let host = host();
            let revision = host.inner.asked.fetch_add(1, Ordering::AcqRel) + 1;
            host.inner.queued.publish(Queued {
                pending: u32::try_from(pending).unwrap_or(u32::MAX),
                revision,
            });
        });
        crate::edit_state::on_ask(|song, edit, revision| {
            host().inner.edits.publish(EditAsked {
                song: song.to_owned(),
                revision,
                at: edit.at,
                selection: edit
                    .selection
                    .map(|s| editing::scope::Span::between(s.start, s.end)),
            });
        });
        crate::changes::listen(|change| match change {
            crate::changes::Change::Marks => marks_changed(),
        });
        let Some(runtime) = crate::open::runtime() else {
            tracing::warn!("view host: no runtime to follow the set on");
            return;
        };
        runtime.spawn(follow_set());
        runtime.spawn(count_windows());
        let _ = std::thread::Builder::new()
            .name("view-jobs".into())
            .spawn(follow_jobs);
        // A recording pass that ends has its takes selected and the ranks
        // pressed while it ran written on them, window or none.
        let _ = std::thread::Builder::new()
            .name("take-settle".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    crate::take_rank::settle_open();
                }
            });
    });
}

/// Publish the set every time it changes; a new current song's marks and
/// overlay read with it.
async fn follow_set() {
    let mut set = super::set::subscribe();
    let mut current = None;
    loop {
        let frame = frame_of(&set.borrow_and_update());
        let now = frame
            .current
            .and_then(|at| frame.songs.get(at as usize))
            .map(|(_, project)| project.clone());
        if let Ok(mut last) = host().inner.last_set.lock() {
            *last = frame.clone();
        }
        host().inner.set.publish(frame);
        if now != current {
            current = now;
            marks_changed();
            crate::hit_overlay::refresh();
        }
        if set.changed().await.is_err() {
            return;
        }
    }
}

/// Keep `crate::window_queue` told how many windows follow (a hub's
/// subscribers are counted as of its last publish, so it is swept).
async fn count_windows() {
    loop {
        let host = host();
        let pending = crate::window_queue::asked_pending();
        let revision = host.inner.asked.load(Ordering::Acquire);
        host.inner.queued.publish(Queued {
            pending: u32::try_from(pending).unwrap_or(u32::MAX),
            revision,
        });
        crate::window_queue::set_linked_windows(host.inner.queued.subscriber_count());
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// The set as a window reads it.
#[must_use]
pub fn frame_of(view: &super::set::SetView) -> SetFrame {
    let opening = match &view.opening {
        super::set::Opening::Idle => SetOpening::Idle,
        super::set::Opening::Busy(progress) => SetOpening::Busy(progress.clone()),
        super::set::Opening::Failed(why) => SetOpening::Failed(why.clone()),
    };
    let Some(setlist) = &view.setlist else {
        return SetFrame {
            revision: view.revision,
            epoch: view.epoch,
            opening,
            ..SetFrame::default()
        };
    };
    SetFrame {
        revision: view.revision,
        epoch: view.epoch,
        songs: setlist
            .songs
            .iter()
            .map(|song| (song.title(), song.project.clone()))
            .collect(),
        colors: setlist
            .songs
            .iter()
            .map(|song| song.color.clone())
            .collect(),
        current: u32::try_from(setlist.at).ok(),
        arriving: setlist.pending.clone(),
        opening,
    }
}

/// The hit overlay of `song` was read: shown to every window.
pub fn overlay_read(song: String, overlay: crate::hit_overlay::Overlay) {
    let host = host();
    let version = host.inner.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let frame = OverlayFrame {
        song,
        version,
        overlay,
    };
    if let Ok(mut last) = host.inner.last_overlay.lock() {
        *last = frame.clone();
    }
    host.inner.overlays.publish(frame);
}

/// The tracks' own marks changed (or another song is current): read them
/// again, off the caller's thread, and show them.
pub fn marks_changed() {
    let Some(runtime) = crate::open::runtime() else {
        return;
    };
    runtime.spawn_blocking(|| {
        let Ok((daw, project)) = crate::actions::song() else {
            return;
        };
        let ProjectContext::Project(song) = &project else {
            return;
        };
        let host = host();
        let version = host.inner.revision.fetch_add(1, Ordering::AcqRel) + 1;
        let frame = MarksFrame {
            song: song.clone(),
            version,
            marks: crate::track_regions::all(daw, &project),
            pois: crate::poi::read(daw, &project),
            poi_current: crate::poi::current(),
        };
        if let Ok(mut last) = host.inner.last_marks.lock() {
            *last = frame.clone();
        }
        host.inner.marks.publish(frame);
    });
}

/// The streaming state, as the engine reads it every half second.
pub fn streams_read(
    shown: &crate::stream_set::Shown,
    waiting: bool,
    listening: crate::view::Listening,
) {
    let host = host();
    let mut items: Vec<(String, f32)> = shown.items.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    let mut songs: Vec<(String, StreamSong)> = shown
        .songs
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                StreamSong {
                    arrived: v.arrived,
                    ready: v.ready,
                    waiting_on: v.waiting_on,
                },
            )
        })
        .collect();
    songs.sort_by(|a, b| a.0.cmp(&b.0));
    let frame = StreamsFrame {
        version: host.inner.revision.fetch_add(1, Ordering::AcqRel) + 1,
        items,
        songs,
        waiting,
        listening,
    };
    if let Ok(mut last) = host.inner.last_streams.lock() {
        *last = frame.clone();
    }
    host.inner.streams.publish(frame);
}

/// What the engine is doing in the background, published when it changes:
/// read four times a second, as the bottom bar draws it.
fn follow_jobs() {
    let host = host();
    let mut told: Vec<JobRow> = Vec::new();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let now: Vec<JobRow> = crate::jobs::running()
            .into_iter()
            .map(
                |crate::jobs::Running {
                     id,
                     label,
                     done,
                     total,
                 }| JobRow {
                    id,
                    label,
                    done: u64::try_from(done).unwrap_or(u64::MAX),
                    total: u64::try_from(total).unwrap_or(u64::MAX),
                },
            )
            .collect();
        if now == told {
            continue;
        }
        told.clone_from(&now);
        let frame = JobsFrame {
            version: host.inner.revision.fetch_add(1, Ordering::AcqRel) + 1,
            jobs: now,
        };
        if let Ok(mut last) = host.inner.last_jobs.lock() {
            *last = frame.clone();
        }
        host.inner.jobs.publish(frame);
    }
}

/// Something about `song` changed in the engine: every window reads it
/// back (`items`: which items' waveforms, for [`Change::Waves`]).
pub fn changed(song: Option<String>, what: Change) {
    changed_items(song, what, Vec::new());
}

/// [`changed`], naming the items it is about.
pub fn changed_items(song: Option<String>, what: Change, items: Vec<String>) {
    let host = host();
    let song = song.or_else(crate::open::current_song).unwrap_or_default();
    let revision = host.inner.revision.fetch_add(1, Ordering::AcqRel) + 1;
    host.inner.changes.publish(Changed {
        song,
        revision,
        what,
        items,
    });
}

/// A window action's description, as the engine's list keeps it.
fn forwarded_info(action: WindowAction) -> architect::action::ActionInfo {
    use architect::action::Effect;
    architect::action::ActionInfo {
        effect: match action.effect.as_str() {
            "read-only" => Effect::ReadOnly,
            "destructive" => Effect::Destructive,
            _ => Effect::Mutating,
        },
        id: action.id,
        display_name: action.name,
        description: action.description,
        category: action.category,
        group: action.group,
        shortcut: action.shortcut,
        params_schema: action.params_schema,
        result_schema: action.result_schema,
        enabled: true,
        disabled_reason: None,
    }
}

/// What the engine's list runs for the window action `id`: it asked of
/// the window, and what the window answered.
fn forward_to_window(id: String) -> architect::action::AsyncHandler {
    Arc::new(move |args: &[u8]| {
        let id = id.clone();
        let args = String::from_utf8_lossy(args).into_owned();
        Ok(Box::pin(async move {
            crate::window_queue::ask_window_action(id.clone(), args)
                .await
                .map(String::into_bytes)
                .map_err(|message| architect::action::ActionError::Failed { id, message })
        }) as architect::action::ActionFuture)
    })
}

/// Run blocking engine work off the runtime's workers, inside the call's
/// span: what it records (`waves.fill`, its logs) stays the call's.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let span = tracing::Span::current();
    tokio::task::spawn_blocking(move || span.in_scope(work))
        .await
        .ok()
}

/// The engine, and `song` (the current one when empty).
fn engine_and(song: &str) -> Option<(&'static daw::standalone::Standalone, String)> {
    let daw = crate::open::local_engine()?;
    let song = if song.is_empty() {
        crate::open::current_song()?
    } else {
        song.to_owned()
    };
    Some((daw, song))
}

/// Whether a batch the window made for `song` (empty: it did not say)
/// may be applied while `current` is the engine's current song.
fn targets_current(song: &str, current: Option<&str>) -> bool {
    song.is_empty() || current == Some(song)
}

impl EngineView for ViewHost {
    async fn run(&self, call: ActionCall) -> Result<String, ViewError> {
        let span = tracing::info_span!("view.run", action.id = %call.id, action.outcome = tracing::field::Empty);
        let ran = crate::actions::list()
            .run_json_async(&call.id, call.args.as_bytes())
            .await;
        match ran {
            Ok(out) => {
                span.record("action.outcome", "ok");
                Ok(String::from_utf8_lossy(&out).into_owned())
            }
            Err(e) => {
                span.record("action.outcome", "refused");
                Err(ViewError::Refused(e.to_string()))
            }
        }
    }

    async fn apply(&self, batch: EditBatch) -> Applied {
        let mut applied = Applied::default();
        // The edits name no song of their own: they go to the song current
        // here, so a batch made against another (the window moved on
        // before the engine did) is refused whole — but its barriers
        // still release whoever waits on them.
        let current = crate::open::current_song();
        if !targets_current(&batch.song, current.as_deref()) {
            tracing::warn!(
                edit.song = %batch.song,
                edit.current = current.as_deref().unwrap_or(""),
                edit.count = batch.edits.len(),
                "view link: edits for a song that is not current refused"
            );
            for done in batch
                .edits
                .iter()
                .filter(|e| matches!(e, crate::engine::Edit::Done(_)))
            {
                crate::engine::apply_reported(done).await;
            }
            return applied;
        }
        for edit in &batch.edits {
            applied.items |= edit.is_item();
            crate::engine::apply_reported(edit).await;
            if let Some(predicted) = edit.predicted() {
                let real = crate::engine::made(predicted);
                if real != predicted {
                    applied.made.push((predicted.to_owned(), real));
                }
            }
        }
        if applied.items {
            crate::hit_overlay::refresh();
            changed(Some(batch.song).filter(|s| !s.is_empty()), Change::Items);
        }
        applied
    }

    async fn take_window(&self) -> Vec<crate::window_queue::Asked> {
        crate::window_queue::take_asked()
    }

    async fn window_actions(&self, actions: Vec<WindowAction>) -> u32 {
        let list = crate::actions::list();
        let mut added = 0;
        for action in actions {
            if list.info(&action.id).is_some() {
                continue;
            }
            let id = action.id.clone();
            match list.register_forwarded(&forwarded_info(action), forward_to_window(id.clone())) {
                Ok(()) => {
                    let _ = list.set_enabled(&id, crate::window_queue::a_window_follows);
                    added += 1;
                }
                Err(e) => {
                    tracing::warn!(action.id = %id, error = %e, "view: a window action could not join the list")
                }
            }
        }
        if added > 0 {
            tracing::info!(
                window.actions = added,
                "view: the window's actions joined the engine's list"
            );
        }
        added
    }

    async fn answer(&self, answer: WindowAnswer) {
        let result = if answer.ok {
            Ok(answer.json)
        } else {
            Err(answer.json)
        };
        crate::window_queue::answered(answer.ticket, result);
    }

    async fn waves(&self, ask: WavesAsk) -> Vec<WaveRead> {
        let previews = self.inner.previews.clone();
        blocking(move || {
            let song = Some(ask.song).filter(|s| !s.is_empty());
            previews.read_here(song, &ask.items);
            let partial = previews.partial();
            ask.items
                .into_iter()
                .map(|item| {
                    if partial.contains(&item) {
                        return WaveRead::Partial { item };
                    }
                    match previews.wave(&item) {
                        Some(wave) => WaveRead::Whole {
                            wave: (*wave).clone(),
                            item,
                        },
                        None => WaveRead::None { item },
                    }
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    async fn detail(&self, ask: DetailAsk) -> Option<crate::wave_detail::Detail> {
        blocking(move || {
            let (daw, song) = engine_and(&ask.song)?;
            crate::wave_detail::read_in(daw, &song, &ask.source, (ask.from, ask.to), ask.step)
        })
        .await
        .flatten()
    }

    async fn automation(&self, song: String) -> AutomationRead {
        blocking(move || {
            let Some((daw, song)) = engine_and(&song) else {
                return AutomationRead::default();
            };
            crate::automation::read_for_window(daw, &song)
        })
        .await
        .unwrap_or_default()
    }

    async fn item_changes(&self, ask: ChangesAsk) -> ItemChanges {
        blocking(move || {
            let Some((daw, song)) = engine_and(&ask.song) else {
                return ItemChanges::default();
            };
            item_changes(daw, &song, ask.since)
        })
        .await
        .unwrap_or_default()
    }

    async fn journal_now(&self) -> u64 {
        daw::standalone::journal::now()
    }

    async fn song_facts(&self, song: String) -> SongFacts {
        blocking(move || {
            let Some((daw, song)) = engine_and(&song) else {
                return SongFacts::default();
            };
            song_facts(daw, &song)
        })
        .await
        .unwrap_or_default()
    }

    async fn show_edit(&self, shown: EditShown) -> bool {
        if let Some(scope) = shown.scope {
            crate::edit_actions::publish(scope);
        }
        let edit = crate::edit_state::Edit {
            at: shown.at,
            selection: shown
                .selection
                .map(|s| crate::edit_state::Span::between(s.from, s.to)),
        };
        let before = crate::edit_state::of(&shown.song);
        crate::edit_state::show_on(&shown.song, edit);
        !before.is_some_and(|b| crate::edit_state::same(&b, &edit))
    }

    async fn playheads(&self) -> Vec<Playhead> {
        let lineup = super::set::lineup();
        lineup
            .map(|lineup| {
                lineup
                    .songs
                    .iter()
                    .filter_map(|(_, song)| {
                        let (at, playing) = crate::engine::playhead_of(song)?;
                        Some(Playhead {
                            song: song.clone(),
                            at,
                            playing,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl EngineViewStreamSource for ViewHost {
    fn set_hub(&self) -> &PubSub<SetFrame> {
        &self.inner.set
    }

    fn set_intro(&self) -> Option<SetFrame> {
        Some(frame_of(&super::set::view()))
    }

    fn overlays_hub(&self) -> &PubSub<OverlayFrame> {
        &self.inner.overlays
    }

    fn overlays_intro(&self) -> Option<OverlayFrame> {
        self.inner
            .last_overlay
            .lock()
            .ok()
            .map(|f| f.clone())
            .filter(|f| f.version > 0)
    }

    fn marks_hub(&self) -> &PubSub<MarksFrame> {
        &self.inner.marks
    }

    fn marks_intro(&self) -> Option<MarksFrame> {
        self.inner
            .last_marks
            .lock()
            .ok()
            .map(|f| f.clone())
            .filter(|f| f.version > 0)
    }

    fn streams_hub(&self) -> &PubSub<StreamsFrame> {
        &self.inner.streams
    }

    fn streams_intro(&self) -> Option<StreamsFrame> {
        self.inner.last_streams.lock().ok().map(|f| f.clone())
    }

    fn jobs_hub(&self) -> &PubSub<JobsFrame> {
        &self.inner.jobs
    }

    fn jobs_intro(&self) -> Option<JobsFrame> {
        self.inner.last_jobs.lock().ok().map(|f| f.clone())
    }

    fn edits_hub(&self) -> &PubSub<EditAsked> {
        &self.inner.edits
    }

    fn changes_hub(&self) -> &PubSub<Changed> {
        &self.inner.changes
    }

    fn queued_hub(&self) -> &PubSub<Queued> {
        &self.inner.queued
    }

    fn queued_intro(&self) -> Option<Queued> {
        Some(Queued {
            pending: u32::try_from(crate::window_queue::asked_pending()).unwrap_or(u32::MAX),
            revision: self.inner.asked.load(Ordering::Acquire),
        })
    }
}

/// The items of `song` the journal says changed since `since`, each as it
/// is now.
fn item_changes(daw: &daw::standalone::Standalone, song: &str, since: u64) -> ItemChanges {
    use daw::service::{ItemRef, Items as _, Takes as _};
    let Some(changes) = daw.item_changes_since(song, since) else {
        return ItemChanges::default();
    };
    let Some(changed) = changes.items else {
        return ItemChanges {
            cursor: changes.cursor,
            changed: None,
            gone: Vec::new(),
        };
    };
    let project = ProjectContext::Project(song.to_owned());
    let mut now = Vec::new();
    let mut gone = Vec::new();
    for guid in changed {
        let Some(item) = daw.get_item(project.clone(), ItemRef::Guid(guid.clone())) else {
            gone.push(guid);
            continue;
        };
        let take = daw.get_active_take(project.clone(), ItemRef::Guid(guid));
        now.push(ItemNow {
            item,
            take_name: take
                .as_ref()
                .map(|t| t.name.clone())
                .filter(|n| !n.is_empty()),
            midi: take.is_some_and(|t| t.is_midi),
        });
    }
    ItemChanges {
        cursor: changes.cursor,
        changed: Some(now),
        gone,
    }
}

/// What a window needs of `song` beside the facade.
fn song_facts(daw: &daw::standalone::Standalone, song: &str) -> SongFacts {
    use daw::service::Projects as _;
    let kinds = crate::plan::Kinds::from_engine(daw, song).entries();
    let chart = crate::song_chart::of_song(daw, &ProjectContext::Project(song.to_owned()))
        .unwrap_or_default();
    let path = daw.project(song).map(|info| info.path).unwrap_or_default();
    SongFacts { kinds, chart, path }
}

#[cfg(test)]
mod tests;
