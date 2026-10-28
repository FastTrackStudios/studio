//! The song as the window reads it back from the engine: the project, the
//! rows planned from it, the item previews and the chart
//! ([`StudioSession`]), and the read-back the engine asks for after an edit
//! ([`request_resync`]).
//!
//! Engine side: nothing here draws. The arrangement panel that stands on it
//! is `crate::studio`, which re-exports all of this.

use std::sync::Arc;

use daw_ui::studio::{ProjectRef, RowsRef};

/// The opened session, as the panels read it.
///
/// Cheap to clone: everything in it is shared.
#[derive(Clone)]
pub struct StudioSession {
    pub project: ProjectRef,
    pub rows: RowsRef,
    pub previews: crate::midi::Previews,
    /// The song's chart, parsed once at [`StudioSession::open`] — what the
    /// Chart panel paints (`crate::chart_panel`). `None` when the session
    /// was opened with no `--chart`, or the file failed to parse: the DAW
    /// and Performance views still work without it.
    pub chart: Option<Arc<keyflow::Chart>>,
    /// The `.kf` the chart was read from — what Organize mode's editor
    /// opens and saves. `None` with no chart file.
    pub chart_file: Option<std::path::PathBuf>,
    /// How the rows were planned, kept so they can be planned again after
    /// a change the plan depends on (a track shown or hidden) without
    /// reading the whole session back from the engine.
    pub planner: Planner,
}

/// The row plan's inputs: the project as the engine had it (before the
/// plan folds anything), the scene it was laid out by, and the track kinds
/// the template wrote into the file (read once, at open).
#[derive(Clone)]
pub struct Planner {
    pub raw: Arc<daw_ui::studio::project::Project>,
    pub scene: Option<&'static str>,
    pub kinds: Arc<crate::plan::Kinds>,
}

impl Planner {
    /// Plan the rows for `raw`: [`plan_rows`] with this planner's scene
    /// and file.
    #[must_use]
    pub fn plan(&self, raw: &daw_ui::studio::project::Project) -> (ProjectRef, RowsRef) {
        plan_rows_with(raw, self.scene, &self.kinds)
    }

    /// The mixer's strips for `raw` outside Mix mode — docked under the
    /// arrangement or on its own: the scene's MIXER rows (a folder the
    /// scene hides from the mixer is gone, one it folds only in the
    /// arrangement keeps every strip), and the folders the window shut
    /// (`crate::folds`), which both panels share. Each strip is the track
    /// as the engine has it: the scene decides which strips, not how wide
    /// a strip is outside Mix mode.
    ///
    /// Not the arrangement's rows: the two panels answer different
    /// questions, and the scene says so per surface.
    #[must_use]
    pub fn plan_strips(&self, raw: &daw_ui::studio::project::Project) -> RowsRef {
        RowsRef(Arc::new(plan_strips_with(raw, self.scene, &self.kinds)))
    }

    /// The mixer's rows for `raw`: the scene laid out for the mixer — each
    /// track as wide as the scene opens it, the one it focuses selected —
    /// and every folder as the scene leaves it, the way the scene renders
    /// (`tests/golden_scenes.rs`), then as the window's folds leave it.
    /// `extent` is the panel's height.
    #[must_use]
    pub fn plan_mixer(
        &self,
        raw: &daw_ui::studio::project::Project,
        scene: Option<&str>,
        settings: crate::settings::Settings,
        extent: f64,
    ) -> RowsRef {
        let (visible, depths) =
            daw_ui::components::folders::FolderState::default().visible(&raw.tracks);
        let planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
        let Some(scene) = scene
            .or(self.scene)
            .and_then(dynamic_template::scenes::scene)
        else {
            return RowsRef(Arc::new(crate::folds::apply(planned)));
        };
        // And the folders the window shut (`crate::folds`), over the
        // scene's: shut in the arrangement, shut here — and a folder
        // opened or shut on a strip in Mix mode is.
        RowsRef(Arc::new(crate::folds::apply(crate::plan::apply_scene(
            &planned,
            &self.kinds,
            scene,
            crate::plan::Panel {
                surface: crate::plan::Surface::Mixer,
                mode: None,
                settings,
                extent,
                active_language: None,
            },
        ))))
    }
}

impl PartialEq for StudioSession {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.project.0, &other.project.0) && Arc::ptr_eq(&self.rows.0, &other.rows.0)
    }
}

impl StudioSession {
    /// The song that opened as `project` in the engine — current or not —
    /// read back as the studio lays it out, by the project rather than
    /// whichever is current, so a song opening behind the one on screen
    /// does not move it: how a page reads every song it opens, and a
    /// window the songs of a streamed set that arrive after the first.
    /// `rpp_text` is the project's text (for its track kinds), `chart_text`
    /// its chart's; `waves` also reads its audio items' peaks from the
    /// engine (a page is handed them instead).
    ///
    /// # Errors
    ///
    /// The facade is not up, or the project is not in the engine or could
    /// not be read.
    pub async fn read_project(
        project: &str,
        name: &str,
        rpp_text: &str,
        chart_text: Option<&str>,
        waves: bool,
    ) -> eyre::Result<Self> {
        let mut stages = crate::stages::Stages::start();
        const SCENE: &str = "drum-mixing";
        // Say which step fails: `fetch_of` answers only yes or no.
        let facade =
            daw_control::Daw::try_get().ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
        let handle = facade
            .project(project.to_owned())
            .await
            .map_err(|e| eyre::eyre!("{name} is not in the engine: {e}"))?;
        let raw = daw_ui::studio::project::fetch_of(handle.clone())
            .await
            .ok_or_else(|| eyre::eyre!("could not read {name} back"))?;
        let fetch_ms = stages.mark("read.fetch_ms");
        // Its automation lanes: from this process's engine, or through
        // the facade from wherever the engine is.
        #[cfg(feature = "native")]
        let local = crate::open::local_engine().is_some();
        #[cfg(not(feature = "native"))]
        let local = false;
        if local {
            #[cfg(feature = "native")]
            crate::automation::refresh_in(project, &raw.tracks);
        } else {
            let (lanes, takes) = crate::automation::read_remote(&raw.tracks).await;
            crate::automation::adopt_remote(lanes, takes);
        }
        let automation_ms = stages.mark("read.automation_ms");
        let planner = Planner {
            raw: Arc::new(raw),
            scene: Some(SCENE),
            kinds: Arc::new({
                // A project file has no text to read them from: the engine
                // holds them, as it does for the song on screen.
                let from_text = crate::plan::Kinds::from_text(rpp_text);
                #[cfg(feature = "native")]
                let from_text = if from_text.is_empty() {
                    crate::open::local_engine()
                        .map(|daw| crate::plan::Kinds::from_engine(daw, project))
                        .unwrap_or(from_text)
                } else {
                    from_text
                };
                #[cfg(all(feature = "web", not(feature = "native")))]
                let from_text = if from_text.is_empty() {
                    crate::web_engine::kinds_of(project).unwrap_or(from_text)
                } else {
                    from_text
                };
                from_text
            }),
        };
        let kinds_ms = stages.mark("read.kinds_ms");
        let (studio, rows) = planner.plan(&planner.raw);
        let plan_ms = stages.mark("read.plan_ms");
        let previews = crate::midi::Previews::default();
        let items = || studio.0.items.values().flatten();
        // A song opened behind the one on screen joins its set first; its
        // pictures are read after ([`Self::fill_previews`]).
        if waves {
            previews
                .fill_in(
                    &handle,
                    items()
                        .filter(|item| studio.0.is_midi(&item.guid))
                        .map(|item| (item.guid.clone(), item.length.as_seconds()))
                        .collect(),
                )
                .await;
        }
        if waves {
            previews
                .fill_waves_in(
                    &handle,
                    items()
                        .filter(|item| !studio.0.is_midi(&item.guid))
                        .map(|item| item.guid.clone())
                        .collect(),
                )
                .await;
        }
        let previews_ms = stages.mark("read.previews_ms");
        tracing::info!(
            song.name = name,
            read.fetch_ms = fetch_ms,
            read.automation_ms = automation_ms,
            read.kinds_ms = kinds_ms,
            read.plan_ms = plan_ms,
            read.previews_ms = previews_ms,
            "song read back"
        );
        let chart = chart_text
            .and_then(|text| {
                keyflow::parse(text)
                    .inspect_err(|e| tracing::error!(error = %e, "chart: could not parse"))
                    .ok()
                    .map(Arc::new)
            })
            .or_else(|| chart_of_song(Some(project)));
        Ok(Self {
            project: studio,
            rows,
            previews,
            chart,
            chart_file: None,
            planner,
        })
    }
}

impl StudioSession {
    /// The song `project` of the engine this window is linked to
    /// (`crate::view_client`), read back as the studio lays it out: its
    /// tracks and items through the facade, its track kinds, chart,
    /// automation and waveforms as the engine has them — by the project, so
    /// reading it does not select it.
    ///
    /// # Errors
    ///
    /// The facade is not up, or the project is not in the engine or could
    /// not be read.
    #[cfg(feature = "native")]
    pub async fn read_linked(project: &str, name: &str) -> eyre::Result<Self> {
        const SCENE: &str = "drum-mixing";
        let mut stages = crate::stages::Stages::start();
        let facade =
            daw_control::Daw::try_get().ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
        let handle = facade
            .project(project.to_owned())
            .await
            .map_err(|e| eyre::eyre!("{name} is not in the engine: {e}"))?;
        let (raw, facts, automation) = futures_util::join!(
            daw_ui::studio::project::fetch_of(handle.clone()),
            crate::view_client::song_facts(project),
            crate::view_client::automation_of(project),
        );
        let raw = raw.ok_or_else(|| eyre::eyre!("could not read {name} back"))?;
        let facts = facts.unwrap_or_default();
        let fetch_ms = stages.mark("read.fetch_ms");
        if let Some(read) = automation {
            crate::automation::adopt_read(read);
        }
        let planner = Planner {
            raw: Arc::new(raw),
            scene: Some(SCENE),
            kinds: Arc::new(crate::plan::Kinds::from_entries(facts.kinds)),
        };
        let (studio, rows) = planner.plan(&planner.raw);
        let plan_ms = stages.mark("read.plan_ms");
        let previews = crate::midi::Previews::default();
        let items = || studio.0.items.values().flatten();
        previews
            .fill_in(
                &handle,
                items()
                    .filter(|item| studio.0.is_midi(&item.guid))
                    .map(|item| (item.guid.clone(), item.length.as_seconds()))
                    .collect(),
            )
            .await;
        // The waveforms come in behind: the song is shown at once, each
        // lane drawn as its waveform lands (`Previews` counts them).
        let audio: Vec<String> = items()
            .filter(|item| !studio.0.is_midi(&item.guid))
            .map(|item| item.guid.clone())
            .collect();
        if let Some(rt) = crate::open::runtime() {
            let previews = previews.clone();
            let song = project.to_owned();
            rt.spawn(async move { previews.fill_waves_linked(&song, audio).await });
        }
        let previews_ms = stages.mark("read.previews_ms");
        tracing::info!(
            song.name = name,
            read.fetch_ms = fetch_ms,
            read.plan_ms = plan_ms,
            read.previews_ms = previews_ms,
            "song read back over the link"
        );
        let chart = Some(facts.chart)
            .filter(|text| !text.is_empty())
            .and_then(|text| {
                keyflow::parse(text.as_str())
                    .inspect_err(
                        |e| tracing::warn!(error = %e, "chart: the engine's did not parse"),
                    )
                    .ok()
                    .map(Arc::new)
            });
        let chart_file = Some(std::path::PathBuf::from(&facts.path))
            .filter(|p| p.exists())
            .as_deref()
            .and_then(crate::prepare::chart_beside);
        Ok(Self {
            project: studio,
            rows,
            previews,
            chart,
            chart_file,
            planner,
        })
    }

    /// Read `project`'s pictures — its MIDI items' notes, its audio items'
    /// waveforms — into its previews, after the song has joined its set:
    /// read first, every song of a set waited seconds on them before it
    /// could be picked, and the reads held up the songs opening beside it.
    /// The window draws each as it lands (`Previews` counts them).
    pub async fn fill_previews(&self, project: &str) {
        let Some(facade) = daw_control::Daw::try_get() else {
            return;
        };
        let Ok(handle) = facade.project(project.to_owned()).await else {
            return;
        };
        let items = || self.project.0.items.values().flatten();
        let midi = items()
            .filter(|item| self.project.0.is_midi(&item.guid))
            .map(|item| (item.guid.clone(), item.length.as_seconds()))
            .collect();
        self.previews.fill_in(&handle, midi).await;
        let audio = items()
            .filter(|item| !self.project.0.is_midi(&item.guid))
            .map(|item| item.guid.clone())
            .collect();
        self.previews.fill_waves_in(&handle, audio).await;
    }
}

#[cfg(feature = "native")]
impl StudioSession {
    /// Open `path` on the process's engine with audio, run `prepare` on it
    /// (organize / chart / guide — see [`crate::prepare`]), and read it back
    /// the way the studio lays it out.
    ///
    /// # Errors
    ///
    /// The project could not be opened or read back. A prepare step that
    /// fails is logged and the session opens as it was.
    pub fn open(path: &std::path::Path, prepare: &crate::prepare::Prepare) -> eyre::Result<Self> {
        Self::open_as(path, prepare, true).map(|(session, _)| session)
    }

    /// [`Self::open`] for a song of a setlist: the first (`first`) stands
    /// the engine up and takes the audio; the rest load into the same
    /// engine beside it. Each is current while it is prepared and read —
    /// the guide and the read-back work on the current project — so the
    /// caller makes the song it wants current afterwards
    /// ([`crate::open::switch_to`]). Returns the song's project guid too.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: bool,
    ) -> eyre::Result<(Self, String)> {
        Self::open_first_as(
            path,
            prepare,
            first.then_some(true),
            crate::open::Media::All,
        )
    }

    /// [`Self::open_as`], where `first` is `Some(audio)` for the song that
    /// stands the engine up — with or without a device — and `None` for
    /// the rest; `media` is when its media loads.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_first_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: Option<bool>,
        media: crate::open::Media,
    ) -> eyre::Result<(Self, String)> {
        // A song's opening, its own unit of work: where the time went,
        // stage by stage (`PERF_RECENT`).
        let span = tracing::info_span!("song.open", song.tracks = tracing::field::Empty);
        let _in = span.enter();
        let mut stages = crate::stages::Stages::start();
        let plan = crate::open::song_plan(path, prepare);
        let opened = if let Some(audio) = first {
            crate::open::open_first_with(&plan.open, audio, media)?
        } else {
            let opened = crate::open::open_another_with(&plan.open, media)?;
            crate::open::switch_to(&opened.daw, &opened.project_guid, false);
            opened
        };
        let engine_ms = stages.mark("open.engine_ms");
        crate::open::prepare_and_save(crate::folder::disk(), &opened, &plan, prepare);
        // The song as opened (and prepared): what "unsaved changes" is
        // measured from.
        #[cfg(feature = "native")]
        crate::projects::mark_saved(&opened.daw, &opened.project_guid);
        let prepare_ms = stages.mark("open.prepare_ms");
        let session = Self::read_current(Some(plan.open.as_path()), prepare.chart.clone())?;
        let read_ms = stages.mark("open.read_ms");
        span.record("song.tracks", session.project.0.tracks.len());
        // One line per song: where its opening went.
        tracing::info!(
            song.name = %opened.name,
            song.tracks = session.project.0.tracks.len(),
            open.engine_ms = engine_ms,
            open.prepare_ms = prepare_ms,
            open.read_ms = read_ms,
            open.total_ms = stages.total_ms(),
            "song opened"
        );
        Ok((session, opened.project_guid))
    }

    /// The facade's current project, read the way the studio lays it out —
    /// whatever backend the facade points at. `path` is where the project
    /// sits on this machine, when it does (the track kinds the template
    /// wrote are read from it); `chart` the `.kf` to show beside it.
    ///
    /// What a Remote window's songs are made of: REAPER's project, read
    /// back rather than opened (see `crate::setlist::Setlist::attach`).
    ///
    /// # Errors
    ///
    /// The project could not be read back.
    pub fn read_current(
        path: Option<&std::path::Path>,
        chart: Option<std::path::PathBuf>,
    ) -> eyre::Result<Self> {
        const SCENE: &str = "drum-mixing";
        let raw = fetch().ok_or_else(|| {
            eyre::eyre!(
                "could not read {} back",
                path.map_or_else(
                    || "the current project".to_owned(),
                    |p| p.display().to_string()
                )
            )
        })?;
        let planner = Planner {
            raw: Arc::new(raw),
            scene: Some(SCENE),
            kinds: Arc::new({
                let engine = crate::open::local_engine()
                    .zip(crate::open::current_song())
                    .map(|(daw, song)| crate::plan::Kinds::from_engine(daw, &song))
                    .unwrap_or_default();
                if engine.is_empty() {
                    path.filter(|p| p.exists())
                        .and_then(|p| crate::open::project_text(p).ok())
                        .map(|read| crate::plan::Kinds::from_text(&read.text))
                        .unwrap_or_default()
                } else {
                    engine
                }
            }),
        };
        let planning = web_time::Instant::now();
        let (project, rows) = planner.plan(&planner.raw);
        let planned = planning.elapsed();
        let previews = previews_of(&project);
        tracing::info!(
            read.plan_ms = planned.as_millis(),
            read.previews_ms = (planning.elapsed() - planned).as_millis(),
            "song laid out"
        );
        let parsed = chart.as_deref().and_then(|chart_path| {
            let text = std::fs::read_to_string(chart_path)
                .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not read"))
                .ok()?;
            keyflow::parse(text.as_str())
                .inspect_err(|e| tracing::error!(error = %e, path = %chart_path.display(), "chart: could not parse"))
                .ok()
                .map(Arc::new)
        })
        .or_else(|| chart_of_song(None));
        Ok(Self {
            project,
            rows,
            previews,
            chart: parsed,
            chart_file: chart,
            planner,
        })
    }
}

/// The open project, as the studio's own refs: every visible row, laid out
/// by `scene` (the visual track manager's scene slug) when one is given.
#[cfg(feature = "native")]
#[must_use]
pub fn read_back(
    scene: Option<&str>,
    project_path: &std::path::Path,
) -> Option<(ProjectRef, RowsRef)> {
    Some(plan_rows(&fetch()?, scene, project_path))
}

/// Asked for when something changed the session in the ENGINE rather
/// than through the arrangement — a toolbar action, an edited chart laid
/// over the song — so the arrangement reads it back on its next frame.
static RESYNC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the read-back asked for must cover everything (the engine
/// changed something nobody named) or only items (the engine's journal
/// says which).
static ALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the window has read the engine's journal up to
/// ([`daw::standalone::journal::now`] when it last read the song).
#[cfg(feature = "native")]
static SEEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// What a read-back must cover.
#[derive(Debug)]
pub enum Resync {
    /// The whole song: something changed that nobody named.
    All,
    /// Only items: those the engine's journal says changed.
    Items,
}

/// Ask the arrangement to read the session back from the engine — all of
/// it: the safe ask, for a change that does not say what it touched.
pub fn request_resync() {
    request_resync_here();
    // And every window on the engine, over its link.
    #[cfg(feature = "native")]
    crate::studio_engine::view_host::changed(None, crate::view::Change::All);
}

/// [`request_resync`] for this window alone — what its link does when the
/// engine says something changed.
pub fn request_resync_here() {
    ALL.store(true, std::sync::atomic::Ordering::Release);
    RESYNC.store(true, std::sync::atomic::Ordering::Release);
}

/// Ask for the items that changed to be read back: an edit that changed
/// only items (moved, cut, made, removed, re-laned, their takes) and
/// nothing else of the song — not the tracks, not the ruler. Which items,
/// the engine's journal says.
pub fn request_resync_items() {
    request_resync_items_here();
    #[cfg(feature = "native")]
    crate::studio_engine::view_host::changed(None, crate::view::Change::Items);
}

/// [`request_resync_items`] for this window alone.
pub fn request_resync_items_here() {
    RESYNC.store(true, std::sync::atomic::Ordering::Release);
}

/// Whether a read-back is asked for; asking clears it.
#[must_use]
pub fn take_resync() -> bool {
    take_resync_scope().is_some()
}

/// Whether one is waiting, without taking it — what a panel's
/// `needs_redraw` asks.
#[must_use]
pub fn resync_pending() -> bool {
    RESYNC.load(std::sync::atomic::Ordering::Relaxed)
}

/// The read-back asked for, if one is; asking clears it.
#[must_use]
pub fn take_resync_scope() -> Option<Resync> {
    if !RESYNC.swap(false, std::sync::atomic::Ordering::AcqRel) {
        return None;
    }
    if ALL.swap(false, std::sync::atomic::Ordering::AcqRel) {
        Some(Resync::All)
    } else {
        Some(Resync::Items)
    }
}

/// The window has read the whole song as of now: the journal from here.
/// Taken BEFORE the read, so a write during it is read again next time.
#[cfg(feature = "native")]
pub fn mark_seen() {
    // The engine's journal, over the window's link: none, and every edit
    // reads the whole song back.
    SEEN.store(
        crate::view_client::journal_now().unwrap_or(0),
        std::sync::atomic::Ordering::Release,
    );
}

/// The items the engine's journal says changed since the window last
/// read, read again into `raw` in place: each where it now is (or gone),
/// its lane in order, its take's name and whether it is MIDI. The guids
/// of those still in the song, or `None` when the journal cannot say or
/// there is no engine here (read the whole song back instead).
#[cfg(feature = "native")]
#[must_use]
pub fn patch_items(raw: &mut daw_ui::studio::project::Project) -> Option<Vec<String>> {
    let song = crate::open::current_song()?;
    let seen = SEEN.load(std::sync::atomic::Ordering::Acquire);
    // Never read whole yet: nowhere to read on from.
    if seen == 0 {
        return None;
    }
    let changes = crate::view_client::item_changes(&song, seen)?;
    let changed = changes.changed?;
    SEEN.store(changes.cursor, std::sync::atomic::Ordering::Release);
    Some(patch_with(raw, changed, &changes.gone))
}

/// `raw` with `changed` (each item as it is now) put where it is and
/// `gone` taken out: each where it was, taken out; then each where it is,
/// put back, its lane in order, its take's name and whether it is MIDI.
/// The guids of those still in the song.
#[must_use]
pub fn patch_with(
    raw: &mut daw_ui::studio::project::Project,
    changed: Vec<crate::view::ItemNow>,
    gone: &[String],
) -> Vec<String> {
    if changed.is_empty() && gone.is_empty() {
        return Vec::new();
    }
    let named: std::collections::HashSet<&str> = changed
        .iter()
        .map(|c| c.item.guid.as_str())
        .chain(gone.iter().map(String::as_str))
        .collect();
    let mut lanes: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (track, items) in &mut raw.items {
        let before = items.len();
        items.retain(|i| !named.contains(i.guid.as_str()));
        if items.len() != before {
            lanes.insert(track.clone());
        }
    }
    for guid in &named {
        raw.names.remove(*guid);
        raw.midi.remove(*guid);
    }
    let mut now = Vec::new();
    for crate::view::ItemNow {
        item,
        take_name,
        midi,
    } in changed
    {
        let guid = item.guid.clone();
        if let Some(name) = take_name {
            raw.names.insert(guid.clone(), name);
        }
        if midi {
            raw.midi.insert(guid.clone());
        }
        lanes.insert(item.track_guid.clone());
        raw.items
            .entry(item.track_guid.clone())
            .or_default()
            .push(item);
        now.push(guid);
    }
    for track in &lanes {
        if let Some(items) = raw.items.get_mut(track) {
            daw_ui::studio::project::sort_lane(items);
            if items.is_empty() {
                raw.items.remove(track);
            }
        }
    }
    raw.item_count = raw.items.values().map(Vec::len).sum();
    let reach = raw
        .items
        .values()
        .flatten()
        .map(|i| i.position.as_seconds() + i.length.as_seconds())
        .fold(0.0_f64, f64::max);
    raw.length_secs = reach.max(60.0);
    now
}

/// The open project, as the engine has it.
#[cfg(feature = "native")]
#[must_use]
pub fn fetch() -> Option<daw_ui::studio::project::Project> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let mut stages = crate::stages::Stages::start();
    mark_seen();
    let project = rt.block_on(daw_ui::studio::project::fetch())?;
    stages.mark("fetch.read_ms");
    // The lanes as the engine reads them (the leveler's among them), over
    // the window's link; a REAPER driven has its lanes on the facade.
    match crate::open::current_song().and_then(|song| crate::view_client::automation(&song)) {
        Some(read) => crate::automation::adopt_read(read),
        None => {
            let (lanes, takes) = rt.block_on(crate::automation::read_remote(&project.tracks));
            crate::automation::adopt_remote(lanes, takes);
        }
    }
    stages.mark("fetch.automation_ms");
    Some(project)
}

/// No engine in a page's process to write a chart from.
#[cfg(not(feature = "native"))]
const fn chart_of_song(_project: Option<&str>) -> Option<Arc<keyflow::Chart>> {
    None
}

/// A chart for the song `project` (the current one when `None`) written
/// from its sections ([`crate::song_chart`]), when it is in this
/// process's engine and has sections: what the chart view shows for a
/// song with no chart of its own.
#[cfg(feature = "native")]
fn chart_of_song(project: Option<&str>) -> Option<Arc<keyflow::Chart>> {
    use daw::service::Projects as _;
    let daw = crate::open::local_engine()?;
    let guid = match project {
        Some(guid) => guid.to_owned(),
        None => crate::open::current_song().or_else(|| daw.current().map(|p| p.guid))?,
    };
    let text = crate::song_chart::of_song(daw, &daw::service::ProjectContext::Project(guid))?;
    let chart = keyflow::parse(text.as_str())
        .inspect_err(|e| tracing::warn!(error = %e, "chart: the song's sections did not make one"))
        .ok()?;
    tracing::info!(
        chart.sections = chart.sections.len(),
        chart.lines = text.lines().count(),
        "chart: written from the song's sections"
    );
    Some(Arc::new(chart))
}

/// The rows of `raw`, laid out by `scene`: the visible tracks (a hidden
/// folder hides what it holds), planned and folded the way the studio
/// shows them.
#[must_use]
pub fn plan_rows(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    project_path: &std::path::Path,
) -> (ProjectRef, RowsRef) {
    plan_rows_with(raw, scene, &crate::plan::Kinds::current(project_path))
}

/// [`plan_rows`], with the track kinds already read.
#[must_use]
pub fn plan_rows_with(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    kinds: &crate::plan::Kinds,
) -> (ProjectRef, RowsRef) {
    let mut stages = crate::stages::Stages::start();
    let project = ProjectRef(Arc::new(raw.clone()));
    stages.mark("plan.clone_ms");
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&project.tracks);
    let mut planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    // The session file carries no per-track height; the heights are a
    // layout the scene decides.
    let settings = crate::settings::Settings {
        folded_takes: std::env::var("FTS_BLITZ_FOLDED_TAKES").as_deref() != Ok("0"),
        ..crate::settings::Settings::default()
    };
    if let Some(slug) = scene.and_then(dynamic_template::scenes::scene) {
        planned = crate::plan::apply_scene(
            &planned,
            kinds,
            slug,
            crate::plan::Panel {
                surface: crate::plan::Surface::Arrange,
                mode: None,
                settings,
                extent: 1440.0,
                active_language: None,
            },
        );
    }
    // And the folders the window shut (`crate::folds`), over the scene's.
    planned = crate::folds::apply(planned);
    stages.mark("plan.rows_ms");
    // A folder the scene shut keeps its row and gets its children's items,
    // folded: the mix happens on the folder, so its name, fader and colour
    // stay, and so does what is in it.
    let mut project = project;
    {
        let shown: Vec<daw_proto::Track> = planned.iter().map(|(t, _)| t.clone()).collect();
        let folded = Arc::make_mut(&mut project.0);
        // A shut folder whose `+` is open shows every lane under it, lane
        // by lane (`crate::lanes::folder_view`).
        daw_ui::studio::folded::refold_lanes(
            folded,
            &shown,
            settings.folded_takes,
            &|tracks, folder| crate::lanes::folder_view(tracks, folder).is_some(),
        );
        stages.mark("plan.refold_ms");
        if !crate::folds::cuts_shown() {
            daw_ui::studio::folded::join_seams(folded);
        }
        stages.mark("plan.join_ms");
    }
    (project, RowsRef(Arc::new(planned)))
}

/// The strips of `raw` laid out by `scene` for the mixer — see
/// [`Planner::plan_strips`].
#[must_use]
pub fn plan_strips_with(
    raw: &daw_ui::studio::project::Project,
    scene: Option<&str>,
    kinds: &crate::plan::Kinds,
) -> Vec<(daw_proto::Track, u32)> {
    let (visible, depths) =
        daw_ui::components::folders::FolderState::default().visible(&raw.tracks);
    let planned: Vec<(daw_proto::Track, u32)> = visible.into_iter().zip(depths).collect();
    let Some(slug) = scene.and_then(dynamic_template::scenes::scene) else {
        return crate::folds::apply(planned);
    };
    let strips = crate::plan::apply_scene(
        &planned,
        kinds,
        slug,
        crate::plan::Panel {
            surface: crate::plan::Surface::Mixer,
            mode: None,
            settings: crate::settings::Settings::default(),
            extent: 1440.0,
            active_language: None,
        },
    );
    // The track as it was, not as the scene sized and selected it.
    let as_it_was: std::collections::HashMap<&str, &daw_proto::Track> = planned
        .iter()
        .map(|(track, _)| (track.guid.as_str(), track))
        .collect();
    crate::folds::apply(
        strips
            .into_iter()
            .map(|(track, depth)| {
                let track = as_it_was
                    .get(track.guid.as_str())
                    .map_or(track, |was| (*was).clone());
                (track, depth)
            })
            .collect(),
    )
}

/// Every MIDI item's notes, read before anything is drawn — an item drawn
/// from a waveform it does not have is why a chord track once looked like
/// a shaker.
#[must_use]
pub fn previews_of(project: &ProjectRef) -> crate::midi::Previews {
    let previews = crate::midi::Previews::default();
    previews.fill_blocking(
        project
            .0
            .items
            .values()
            .flatten()
            .filter(|item| project.0.is_midi(&item.guid))
            .map(|item| (item.guid.clone(), item.length.as_seconds()))
            .collect(),
    );
    let waves: Vec<String> = project
        .0
        .items
        .values()
        .flatten()
        .filter(|item| !project.0.is_midi(&item.guid))
        .map(|item| item.guid.clone())
        .collect();
    // The waveforms behind the window, each item drawn as it arrives (and
    // the bottom bar showing how far): reading them first held the window
    // still while a song of thousands of items opened.
    #[cfg(feature = "native")]
    {
        // Read as the song it is now: a set opening songs one after
        // another has moved on to the next by the time this runs.
        let song = crate::open::current_song();
        let reading = previews.clone();
        crate::wave_pool::spawn(move || reading.fill_waves_blocking_in(song, waves));
    }
    #[cfg(not(feature = "native"))]
    previews.fill_waves_blocking(waves);
    previews
}
