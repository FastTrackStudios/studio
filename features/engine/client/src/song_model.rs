//! The song as the window reads it back from the engine: the project, the
//! rows planned from it, the item previews and the chart
//! ([`StudioSession`]), and the read-back the engine asks for after an edit
//! ([`request_resync`]).
//!
//! Engine side: nothing here draws. The arrangement panel that stands on it
//! is `crate::studio`, which re-exports all of this.

use std::sync::Arc;

use daw_ui::studio::{ProjectRef, RowsRef};

/// The song as a view reads it and the read-back an engine asks for after
/// an edit: the views' contract's (`daw_view::song`).
pub use daw_view::song::{
    Planner, Plans, Resync, SongView, patch_with, previews_of, request_resync, request_resync_here,
    request_resync_items, request_resync_items_here, resync_pending, take_resync,
    take_resync_scope,
};
#[cfg(feature = "native")]
pub use daw_view::song::{fetch, mark_seen, patch_items};

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

/// How Studio plans a panel's rows: by its template's scenes, over the
/// track kinds the template wrote into the song (read once, at open).
pub struct TemplatePlans {
    pub kinds: Arc<crate::plan::Kinds>,
}

impl Plans for TemplatePlans {
    fn rows(
        &self,
        raw: &daw_ui::studio::project::Project,
        scene: Option<&str>,
    ) -> (ProjectRef, RowsRef) {
        plan_rows_with(raw, scene, &self.kinds)
    }

    fn strips(&self, raw: &daw_ui::studio::project::Project, scene: Option<&str>) -> RowsRef {
        RowsRef(Arc::new(plan_strips_with(raw, scene, &self.kinds)))
    }

    fn scene_name(&self, slug: &str) -> Option<&'static str> {
        dynamic_template::scenes::scene(slug).map(|s| s.name.as_str())
    }

    fn groups(
        &self,
        names: Vec<String>,
    ) -> Result<std::collections::HashMap<String, Vec<String>>, String> {
        dynamic_template::visibility::groups(names).map_err(|e| e.to_string())
    }

    fn group_key(&self, group: &str) -> String {
        dynamic_template::visibility::normalize_key(group)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A planner over `raw` that plans by Studio's template ([`TemplatePlans`]).
#[must_use]
pub fn template_planner(
    raw: Arc<daw_ui::studio::project::Project>,
    scene: Option<&'static str>,
    kinds: Arc<crate::plan::Kinds>,
) -> Planner {
    Planner::new(raw, scene, Arc::new(TemplatePlans { kinds }))
}

/// What Studio plans that a view does not ask of every app: the mixer laid
/// out by a scene, and the track kinds.
pub trait TemplatePlanner {
    /// The track kinds the planner plans with (none when it does not plan
    /// by Studio's template).
    fn kinds(&self) -> Arc<crate::plan::Kinds>;

    /// The mixer's rows for `raw`: the scene laid out for the mixer — each
    /// track as wide as the scene opens it, the one it focuses selected —
    /// and every folder as the scene leaves it, the way the scene renders
    /// (`tests/golden_scenes.rs`), then as the window's folds leave it.
    /// `extent` is the panel's height.
    fn plan_mixer(
        &self,
        raw: &daw_ui::studio::project::Project,
        scene: Option<&str>,
        settings: crate::settings::Settings,
        extent: f64,
    ) -> RowsRef;
}

impl TemplatePlanner for Planner {
    fn kinds(&self) -> Arc<crate::plan::Kinds> {
        self.plans()
            .as_any()
            .downcast_ref::<TemplatePlans>()
            .map_or_else(
                || Arc::new(crate::plan::Kinds::default()),
                |t| Arc::clone(&t.kinds),
            )
    }

    fn plan_mixer(
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
            &self.kinds(),
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
    /// The song as a view reads it: what Studio hands the arrangement, the
    /// mixer and the editor.
    #[must_use]
    pub fn song(&self) -> SongView {
        SongView {
            project: self.project.clone(),
            rows: self.rows.clone(),
            previews: self.previews.clone(),
            planner: self.planner.clone(),
        }
    }

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
        // Each step its own span (an async step's future instrumented with
        // it, a guard is not held across an await), and its time on the
        // read's line.
        let at = web_time::Instant::now();
        const SCENE: &str = "drum-mixing";
        let (handle, raw) = tracing::Instrument::instrument(
            async {
                // Say which step fails: `fetch_of` answers only yes or no.
                let facade = daw_control::Daw::try_get()
                    .ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
                let handle = facade
                    .project(project.to_owned())
                    .await
                    .map_err(|e| eyre::eyre!("{name} is not in the engine: {e}"))?;
                let raw = daw_ui::studio::project::fetch_of(handle.clone())
                    .await
                    .ok_or_else(|| eyre::eyre!("could not read {name} back"))?;
                Ok::<_, eyre::Report>((handle, raw))
            },
            tracing::info_span!("read.fetch"),
        )
        .await?;
        let fetch_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = web_time::Instant::now();
        tracing::Instrument::instrument(
            async {
                // Its automation lanes: from this process's engine, or
                // through the facade from wherever the engine is.
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
            },
            tracing::info_span!("read.automation"),
        )
        .await;
        let automation_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = web_time::Instant::now();
        let step = tracing::info_span!("read.kinds").entered();
        let planner = template_planner(
            Arc::new(raw),
            Some(SCENE),
            Arc::new({
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
                    crate::body::studio_body()
                        .web_kinds_of(project)
                        .unwrap_or(from_text)
                } else {
                    from_text
                };
                from_text
            }),
        );
        drop(step);
        let kinds_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = web_time::Instant::now();
        let step = tracing::info_span!("read.plan").entered();
        let (studio, rows) = planner.plan(&planner.raw);
        drop(step);
        let plan_ms = at.elapsed().as_secs_f64() * 1e3;
        tracing::Span::current().record("song.tracks", studio.0.tracks.len());
        let at = web_time::Instant::now();
        let previews = crate::midi::Previews::default();
        let items = || studio.0.items.values().flatten();
        tracing::Instrument::instrument(
            async {
                // A song opened behind the one on screen joins its set
                // first; its pictures are read after
                // ([`Self::fill_previews`]).
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
            },
            tracing::info_span!("read.previews"),
        )
        .await;
        let previews_ms = at.elapsed().as_secs_f64() * 1e3;
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
    // A read-back, its own unit of work: the facade calls it makes (and the
    // engine's spans for them, over the link) are its children.
    #[tracing::instrument(
        name = "song.read_back",
        skip_all,
        fields(song.name = name, song.tracks = tracing::field::Empty)
    )]
    pub async fn read_linked(project: &str, name: &str) -> eyre::Result<Self> {
        const SCENE: &str = "drum-mixing";
        // Each step its own span (an async step's future instrumented with
        // it, a guard is not held across an await), and its time on the
        // read's line.
        let read_back = tracing::Span::current();
        let at = web_time::Instant::now();
        let (handle, raw, facts, automation) = tracing::Instrument::instrument(
            async {
                let facade = daw_control::Daw::try_get()
                    .ok_or_else(|| eyre::eyre!("the daw facade is not up"))?;
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
                Ok::<_, eyre::Report>((handle, raw, facts, automation))
            },
            tracing::info_span!("read.fetch"),
        )
        .await?;
        let fetch_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = web_time::Instant::now();
        let step = tracing::info_span!("read.plan").entered();
        if let Some(read) = automation {
            crate::automation::adopt_read(read);
        }
        let planner = template_planner(
            Arc::new(raw),
            Some(SCENE),
            Arc::new(crate::plan::Kinds::from_entries(facts.kinds)),
        );
        let (studio, rows) = planner.plan(&planner.raw);
        drop(step);
        let plan_ms = at.elapsed().as_secs_f64() * 1e3;
        let at = web_time::Instant::now();
        let previews = crate::midi::Previews::default();
        let items = || studio.0.items.values().flatten();
        let step = tracing::info_span!("read.previews");
        tracing::Instrument::instrument(
            previews.fill_in(
                &handle,
                items()
                    .filter(|item| studio.0.is_midi(&item.guid))
                    .map(|item| (item.guid.clone(), item.length.as_seconds()))
                    .collect(),
            ),
            step.clone(),
        )
        .await;
        let step = step.entered();
        // The waveforms come in behind: the song is shown at once, each
        // lane drawn as its waveform lands (`Previews` counts them).
        let audio: Vec<String> = items()
            .filter(|item| !studio.0.is_midi(&item.guid))
            .map(|item| item.guid.clone())
            .collect();
        if let Some(rt) = crate::open::runtime() {
            let previews = previews.clone();
            let song = project.to_owned();
            // Behind the read, but still its work: its trace (under the
            // read itself, not the step that spawns it).
            let waves = tracing::info_span!(
                parent: &read_back,
                "song.waves",
                song.items = audio.len()
            );
            rt.spawn(tracing::Instrument::instrument(
                async move { previews.fill_waves_linked(&song, audio).await },
                waves,
            ));
        }
        drop(step);
        let previews_ms = at.elapsed().as_secs_f64() * 1e3;
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
    // A song's pictures, its own unit of work: the peaks reads it makes
    // (`PeaksRpc/take_peaks`) land under it.
    #[tracing::instrument(name = "song.pictures", skip_all, fields(song.project = project))]
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
        let planner = template_planner(
            Arc::new(raw),
            Some(SCENE),
            Arc::new({
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
        );
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
    let step = tracing::info_span!("plan.clone").entered();
    let project = ProjectRef(Arc::new(raw.clone()));
    drop(step);
    let step = tracing::info_span!("plan.rows").entered();
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
    drop(step);
    let step = tracing::info_span!("plan.refold").entered();
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
        drop(step);
        let step = tracing::info_span!("plan.join").entered();
        if !crate::folds::cuts_shown() {
            daw_ui::studio::folded::join_seams(folded);
        }
        drop(step);
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
