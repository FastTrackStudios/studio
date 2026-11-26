//! Opening a song on the process's engine and reading it back the way the
//! studio lays it out — `StudioSession`'s opening, here with the engine; the
//! model itself, what a window draws, is
//! `engine_runtime::client::song_model`.

use crate::song_model::StudioSession;

/// Opening a [`StudioSession`] on this process's engine.
pub trait OpenSession: Sized {
    /// Open `path` on the process's engine with audio, run `prepare` on it
    /// (organize / chart / guide — see [`crate::prepare`]), and read it back
    /// the way the studio lays it out.
    ///
    /// # Errors
    ///
    /// The project could not be opened or read back. A prepare step that
    /// fails is logged and the session opens as it was.
    fn open(path: &std::path::Path, prepare: &crate::prepare::Prepare) -> eyre::Result<Self>;

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
    fn open_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: bool,
    ) -> eyre::Result<(Self, String)>;

    /// [`Self::open_as`], where `first` is `Some(audio)` for the song that
    /// stands the engine up — with or without a device — and `None` for
    /// the rest; `media` is when its media loads.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    fn open_first_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: Option<bool>,
        media: crate::open::Media,
    ) -> eyre::Result<(Self, String)>;
}

impl OpenSession for StudioSession {
    fn open(path: &std::path::Path, prepare: &crate::prepare::Prepare) -> eyre::Result<Self> {
        Self::open_as(path, prepare, true).map(|(session, _)| session)
    }

    fn open_as(
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

    fn open_first_as(
        path: &std::path::Path,
        prepare: &crate::prepare::Prepare,
        first: Option<bool>,
        media: crate::open::Media,
    ) -> eyre::Result<(Self, String)> {
        // A song's opening, its own unit of work: where the time went,
        // stage by stage (`PERF_RECENT`), and `song.ready_ms`, from asked
        // to open to ready to play (its plugins loaded and activated).
        let span = tracing::info_span!(
            "song.open",
            song.ready_ms = tracing::field::Empty,
            song.tracks = tracing::field::Empty,
            open.engine_ms = tracing::field::Empty,
            open.prepare_ms = tracing::field::Empty,
            open.read_ms = tracing::field::Empty,
        );
        let _in = span.enter();
        let opening = web_time::Instant::now();
        let began = opening;
        let step = tracing::info_span!("open.engine").entered();
        let plan = crate::open::song_plan(path, prepare);
        let opened = if let Some(audio) = first {
            crate::open::open_first_with(&plan.open, audio, media)?
        } else {
            let opened = crate::open::open_another_with(&plan.open, media)?;
            crate::open::switch_to(&opened.daw, &opened.project_guid, false);
            opened
        };
        drop(step);
        let engine_ms = began.elapsed().as_secs_f64() * 1e3;
        let began = web_time::Instant::now();
        let step = tracing::info_span!("open.prepare").entered();
        crate::open::prepare_and_save(crate::folder::disk(), &opened, &plan, prepare);
        // The song as opened (and prepared): what "unsaved changes" is
        // measured from.
        #[cfg(feature = "native")]
        crate::projects::mark_saved(&opened.daw, &opened.project_guid);
        drop(step);
        let prepare_ms = began.elapsed().as_secs_f64() * 1e3;
        let began = web_time::Instant::now();
        let step = tracing::info_span!("open.read").entered();
        let session = Self::read_current(Some(plan.open.as_path()), prepare.chart.clone())?;
        drop(step);
        let read_ms = began.elapsed().as_secs_f64() * 1e3;
        span.record("song.tracks", session.project.0.tracks.len());
        span.record("open.engine_ms", engine_ms);
        span.record("open.prepare_ms", prepare_ms);
        span.record("open.read_ms", read_ms);
        span.record("song.ready_ms", opening.elapsed().as_secs_f64() * 1e3);
        // One line per song: where its opening went.
        tracing::info!(
            song.name = %opened.name,
            song.tracks = session.project.0.tracks.len(),
            open.engine_ms = engine_ms,
            open.prepare_ms = prepare_ms,
            open.read_ms = read_ms,
            open.total_ms = opening.elapsed().as_secs_f64() * 1e3,
            "song opened"
        );
        Ok((session, opened.project_guid))
    }
}
