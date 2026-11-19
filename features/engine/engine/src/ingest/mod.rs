//! Ingest: songs from this machine into an org's Task library, ready to
//! open anywhere — `studio ingest`, the welcome screen's Publish and the
//! Action List's `SESSION_INGEST` all run [`run`].
//!
//! 1. **Inputs** ([`inputs`]): a Live set (`.als`) is split into songs
//!    (`crate::show`); a song folder, `.session` or `.RPP` is one song; a
//!    folder of song folders (an album) is each of them, in order.
//! 2. **Prepare** ([`prepare`]), here: the project file folded whole, the
//!    proxies and peaks of what each song plays, its reference — each
//!    kept when it is already there and current.
//! 3. **Upload** ([`upload`]): each song's folder to a File Root at
//!    `session/<slug>` on the org's media volume — made by the server
//!    (`RootsService::create_on`), or the root the library already plays
//!    the song from. What the server has is never sent again
//!    ([`plan`]); what goes is streamed, a few files at once, resumable.
//!    Each root that changed is checkpointed.
//! 4. **Publish**: each song's `song:` doc, the setlist (kind `setlist`,
//!    the songs in input order), the event or album project with a child
//!    project per song, the setlist attached to it, and — asked — a live
//!    link to it.
//!
//! Every step finds what is there before it makes anything, so ingesting
//! again updates and checkpoints and never duplicates a root, a project
//! or a song in the setlist.

use std::path::{Path, PathBuf};
use web_time::Instant;

use facet::Facet;
pub use session::library::publish::Form;
use session::library::publish::{Made, Project, ProjectSpec, RemoteFile, SongRoot};
use session::library::{Library, ListKind};

pub mod actions;
pub mod inputs;
mod ledger;
pub mod plan;
pub mod prepare;
mod upload;

pub use plan::Media;

/// What to ingest, and how.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// Live sets, songs and folders of songs, in the order they play.
    pub inputs: Vec<PathBuf>,
    /// The event or album project the songs belong to, if any.
    pub project: Option<String>,
    /// That project's form (album, concert, …).
    pub form: Option<Form>,
    /// The setlist that plays them; the project's title (or the one show
    /// or folder's name) when left out.
    pub setlist: Option<String>,
    pub media: Media,
    /// Mint a public live link to the setlist.
    pub link: bool,
    /// The org's Storage Location for new roots; its one location when
    /// left out.
    pub location: Option<uuid::Uuid>,
    /// Where a Live set's songs are split to; `session/` beside the set
    /// when left out.
    pub into: Option<PathBuf>,
    /// Bounce each song's reference.
    pub reference: bool,
    /// Plan only: prepare, compare, and report what would be sent.
    pub dry_run: bool,
    /// Files sent at once.
    pub workers: usize,
}

impl Request {
    /// A request for `inputs` with the defaults: originals too, a
    /// reference per song, four files at once.
    #[must_use]
    pub fn new(inputs: Vec<PathBuf>) -> Self {
        Self {
            inputs,
            project: None,
            form: None,
            setlist: None,
            media: Media::Originals,
            link: false,
            location: None,
            into: None,
            reference: true,
            dry_run: false,
            workers: 4,
        }
    }
}

/// How an ingest is going.
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct Status {
    /// `reading`, `preparing`, `comparing`, `uploading`, `publishing`,
    /// `done` or `failed`.
    pub phase: String,
    /// The song being worked on.
    pub song: Option<String>,
    pub songs_done: u32,
    pub songs: u32,
    pub files_done: u32,
    pub files: u32,
    pub bytes_sent: u64,
    pub bytes: u64,
}

/// What an ingest did.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct Report {
    pub server: String,
    pub org: String,
    pub dry_run: bool,
    pub songs: Vec<SongReport>,
    pub setlist: Option<Named>,
    pub project: Option<Named>,
    pub link: Option<Link>,
    pub files_sent: u64,
    pub bytes_sent: u64,
    pub files_skipped: u64,
    pub failures: Vec<String>,
    pub seconds: f64,
}

/// One song of a [`Report`].
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct SongReport {
    pub title: String,
    pub slug: String,
    pub folder: String,
    pub root: String,
    /// The root's folder on the server.
    pub root_path: Option<String>,
    /// `found`, `created` or `updated` — the root, the `song:` doc, the
    /// song's project.
    pub root_made: String,
    pub song_made: String,
    pub project_made: Option<String>,
    pub proxies_made: u64,
    pub peaks_made: u64,
    /// `made`, `current` or `off`.
    pub reference: String,
    pub files: u64,
    /// Files the server lacks: sent, or with `dry_run` what would be.
    pub to_send: u64,
    pub sent: u64,
    pub skipped: u64,
    pub bytes: u64,
    pub checkpointed: bool,
    /// Media it plays from outside its folder, which does not travel.
    pub outside: Vec<String>,
    pub failed: Vec<String>,
}

/// A setlist or project of a [`Report`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct Named {
    pub id: String,
    pub title: String,
    pub made: String,
}

/// The live link of a [`Report`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct Link {
    pub token: String,
    pub url: String,
    pub made: String,
}

/// Ingest `request` into `library`'s org, as its signed-in person. The
/// ledger of what was sent is kept under `data` (`ledger`). Blocks: it
/// runs on a runtime of its own, so call it from a thread that may wait.
///
/// # Errors
///
/// An input is nothing ingest knows, two songs share a library id, a song
/// would not prepare, or the server cannot be reached or refused a root,
/// the setlist or a project. A file that fails to upload is reported in
/// the [`Report`], not fatal.
pub fn run(
    library: &Library,
    request: &Request,
    data: &Path,
    progress: &(dyn Fn(&Status) + Sync),
) -> eyre::Result<Report> {
    let started = Instant::now();
    let mut status = Status {
        phase: "reading".into(),
        ..Status::default()
    };
    progress(&status);
    let inputs: Vec<inputs::Input> = request
        .inputs
        .iter()
        .map(|p| inputs::classify(p))
        .collect::<eyre::Result<_>>()?;
    let (songs, prepared) = gather(&inputs, request, &mut status, progress)?;
    if let Some((a, b)) = inputs::clash(&songs) {
        eyre::bail!("{a:?} and {b:?} would be the same song in the library; rename one");
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(request.workers.clamp(2, 8))
        .thread_name("session-ingest")
        .enable_all()
        .build()?;
    let mut report = runtime.block_on(send_and_publish(
        library,
        request,
        &inputs,
        &songs,
        data,
        &mut status,
        progress,
    ))?;
    for (song, prepared) in report.songs.iter_mut().zip(prepared) {
        song.proxies_made = prepared.proxies.made as u64;
        song.peaks_made = prepared.peaks.made as u64;
        song.reference = done_word(prepared.reference).to_owned();
        song.outside = prepared.outside;
        song.failed.extend(prepared.proxies.failed);
        song.failed.extend(prepared.peaks.failed);
    }
    report.failures = report
        .songs
        .iter()
        .flat_map(|s| s.failed.iter().map(move |f| format!("{}: {f}", s.slug)))
        .collect();
    report.seconds = started.elapsed().as_secs_f64();
    status.phase = "done".into();
    progress(&status);
    Ok(report)
}

/// Every song of `inputs`, in order, prepared: shows split first.
fn gather(
    inputs: &[inputs::Input],
    request: &Request,
    status: &mut Status,
    progress: &(dyn Fn(&Status) + Sync),
) -> eyre::Result<(Vec<inputs::SongSource>, Vec<prepare::Prepared>)> {
    let mut songs = Vec::new();
    for input in inputs {
        match input {
            inputs::Input::Show(set) => {
                status.phase = "splitting".into();
                status.song = Some(input.name());
                progress(status);
                let into = request
                    .into
                    .clone()
                    .unwrap_or_else(|| set.parent().unwrap_or(Path::new(".")).join("session"));
                songs.extend(prepare::split(set, &into)?.0);
            }
            other => songs.extend(inputs::songs_of(other)?),
        }
    }
    status.phase = "preparing".into();
    status.songs = u32::try_from(songs.len()).unwrap_or(u32::MAX);
    let steps = prepare::Steps {
        reference: request.reference,
    };
    let mut prepared = Vec::with_capacity(songs.len());
    for (at, song) in songs.iter().enumerate() {
        status.song = Some(song.title.clone());
        status.songs_done = u32::try_from(at).unwrap_or(u32::MAX);
        progress(status);
        prepared.push(
            prepare::prepare(song, steps)
                .map_err(|e| eyre::eyre!("preparing {}: {e}", song.title))?,
        );
    }
    status.songs_done = status.songs;
    Ok((songs, prepared))
}

/// Steps 3 and 4: the roots, the uploads, the checkpoints, then the
/// library's songs, setlist, projects and link.
async fn send_and_publish(
    library: &Library,
    request: &Request,
    inputs: &[inputs::Input],
    songs: &[inputs::SongSource],
    data: &Path,
    status: &mut Status,
    progress: &(dyn Fn(&Status) + Sync),
) -> eyre::Result<Report> {
    status.phase = "comparing".into();
    status.song = None;
    progress(status);
    let files = library.files_client().await?;
    let mut batches = Vec::with_capacity(songs.len());
    let mut report = Report {
        server: library.server.clone(),
        org: library.org.clone(),
        dry_run: request.dry_run,
        ..Report::default()
    };
    for song in songs {
        status.song = Some(song.title.clone());
        progress(status);
        let root = if request.dry_run {
            // A dry run makes nothing: a song with no root yet would get
            // one, and every file would go.
            match library.find_song_root(&files, &song.slug).await? {
                Some(root) => Some(SongRoot {
                    root,
                    made: Made::Found,
                }),
                None => None,
            }
        } else {
            Some(
                library
                    .song_root(&files, &song.slug, &song.title, request.location)
                    .await?,
            )
        };
        let locals = upload::scan(&song.folder, request.media)?;
        let remote = match &root {
            Some(root) if root.made == Made::Found => {
                remote_files(library, &files, root.id(), &locals).await?
            }
            _ => Vec::new(),
        };
        let id = root
            .as_ref()
            .map(|r| r.id().to_string())
            .unwrap_or_default();
        let ledger_path = ledger::path(data, &library.server, &library.org, &id);
        let count = locals.len() as u64;
        let kept = root
            .as_ref()
            .map(|_| ledger::Ledger::load(&ledger_path))
            .unwrap_or_default();
        let (sends, skipped, kept) = upload::batch(locals, &remote, kept, &song.folder)?;
        report.songs.push(SongReport {
            title: song.title.clone(),
            slug: song.slug.clone(),
            folder: song.folder.display().to_string(),
            root: id,
            root_path: root.as_ref().and_then(|r| r.root.path.clone()),
            root_made: root
                .as_ref()
                .map_or("would create", |r| made_word(r.made))
                .to_owned(),
            files: count,
            to_send: sends.len() as u64,
            skipped: skipped as u64,
            bytes: sends.iter().map(|s| s.local.size).sum(),
            ..SongReport::default()
        });
        let Some(root) = root else { continue };
        batches.push(upload::Batch {
            root: root.id(),
            folder: song.folder.clone(),
            sends,
            skipped,
            ledger: kept,
            ledger_path,
        });
    }
    report.files_skipped = batches.iter().map(|b| b.skipped as u64).sum();
    if request.dry_run {
        return Ok(report);
    }
    for batch in &batches {
        // What the comparison learned (files the server has already) is
        // kept now, whatever happens to the uploads.
        if let Err(e) = batch.ledger.save(&batch.ledger_path) {
            tracing::warn!(error = %e, "ingest: the ledger was not saved");
        }
    }

    status.phase = "uploading".into();
    status.song = None;
    progress(status);
    let base = status.clone();
    let sent = upload::send_all(
        library,
        &mut batches,
        request.workers,
        &|s: upload::Sending| {
            progress(&Status {
                files_done: u32::try_from(s.files_done).unwrap_or(u32::MAX),
                files: u32::try_from(s.files).unwrap_or(u32::MAX),
                bytes_sent: s.bytes_sent,
                bytes: s.bytes,
                ..base.clone()
            });
        },
    )
    .await;
    for ((song, batch), sent) in report.songs.iter_mut().zip(&batches).zip(sent) {
        song.sent = sent.landed as u64;
        song.failed.extend(sent.failed);
        report.files_sent += sent.landed as u64;
        report.bytes_sent += sent.bytes;
        if sent.landed > 0 {
            library
                .checkpoint(batch.root, "studio ingest")
                .await
                .map_err(|e| eyre::eyre!("{}: {e}", song.title))?;
            song.checkpointed = true;
        }
    }

    status.phase = "publishing".into();
    progress(status);
    publish(library, request, inputs, songs, &mut report).await?;
    Ok(report)
}

/// What the server has in `root`, each file `locals` could match by size
/// carrying its content address: the catalogue's, else the one its last
/// checkpoint gave it.
async fn remote_files(
    library: &Library,
    files: &session::library::publish::FilesClient,
    root: session::library::publish::RootId,
    locals: &[plan::Local],
) -> eyre::Result<Vec<RemoteFile>> {
    let mut remote = library.catalogue(files, root).await?;
    let sizes: std::collections::HashMap<&str, u64> =
        locals.iter().map(|l| (l.rel.as_str(), l.size)).collect();
    let unaddressed: Vec<String> = remote
        .iter()
        .filter(|r| r.content.is_none() && sizes.get(r.path.as_str()) == Some(&r.size))
        .map(|r| r.path.clone())
        .collect();
    let found = library.addresses(root, &unaddressed).await?;
    for file in &mut remote {
        if file.content.is_none() {
            file.content = found.get(&file.path).cloned();
        }
    }
    Ok(remote)
}

/// The library side: songs, the setlist, the projects, the link.
async fn publish(
    library: &Library,
    request: &Request,
    inputs: &[inputs::Input],
    songs: &[inputs::SongSource],
    report: &mut Report,
) -> eyre::Result<()> {
    for (song, row) in songs.iter().zip(report.songs.iter_mut()) {
        row.song_made = made_word(library.ensure_song(&song.slug, &song.title).await?).to_owned();
    }
    let event = match &request.project {
        Some(title) => Some(
            library
                .ensure_project(&ProjectSpec {
                    title: title.clone(),
                    parent: None,
                    form: request.form,
                    details: format!("{} songs, ingested by `studio ingest`.", songs.len()),
                })
                .await?,
        ),
        None => None,
    };
    if let (Some(event), Some(title)) = (&event, &request.project) {
        report.project = Some(Named {
            id: event.id.to_string(),
            title: title.clone(),
            made: made_word(event.made).to_owned(),
        });
        for (song, row) in songs.iter().zip(report.songs.iter_mut()) {
            let child = library
                .ensure_project(&song_project(song, row, event))
                .await?;
            row.project_made = Some(made_word(child.made).to_owned());
        }
    }
    let Some(title) = setlist_title(request, inputs) else {
        return Ok(());
    };
    let slugs: Vec<String> = songs.iter().map(|s| s.slug.clone()).collect();
    let (list, made) = library.ensure_list(&title, ListKind::Set, &slugs).await?;
    if let Some(event) = &event {
        library.hold_list(event, &list).await?;
    }
    if request.link {
        let (token, made) = library.live_link(&list, &title).await?;
        report.link = Some(Link {
            url: library.live_url(&token),
            token,
            made: made_word(made).to_owned(),
        });
    }
    report.setlist = Some(Named {
        id: list,
        title,
        made: made_word(made).to_owned(),
    });
    Ok(())
}

/// The child project a song of `event` has: its title, and the root it
/// plays from in its details.
fn song_project(song: &inputs::SongSource, row: &SongReport, event: &Project) -> ProjectSpec {
    ProjectSpec {
        title: song.title.clone(),
        parent: Some(event.id),
        form: None,
        details: format!(
            "A song of this project. Session root: {} (session/{}).",
            row.root, song.slug
        ),
    }
}

/// What the setlist is called: as asked, else the project's title, else
/// the one show or folder ingested.
#[must_use]
pub fn setlist_title(request: &Request, inputs: &[inputs::Input]) -> Option<String> {
    let named = |s: &Option<String>| {
        s.as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    named(&request.setlist)
        .or_else(|| named(&request.project))
        .or_else(|| match inputs {
            [one @ (inputs::Input::Show(_) | inputs::Input::Folder(_))] => Some(one.name()),
            _ => None,
        })
}

/// `report` as JSON — what `studio ingest --json` prints.
///
/// # Errors
///
/// It would not serialize (it always does).
pub fn to_json(report: &Report) -> Result<String, String> {
    facet_json::to_string(report).map_err(|e| e.to_string())
}

fn made_word(made: Made) -> &'static str {
    match made {
        Made::Found => "found",
        Made::Created => "created",
        Made::Updated => "updated",
    }
}

fn done_word(done: prepare::Done) -> &'static str {
    match done {
        prepare::Done::Off => "off",
        prepare::Done::Current => "current",
        prepare::Done::Made => "made",
    }
}

#[cfg(test)]
mod tests;
