//! `studio ingest` — songs from this machine into an org's Task library,
//! ready to open anywhere (`session_engine::ingest`, the same code the
//! welcome screen's Publish and `SESSION_INGEST` run).
//!
//! ```text
//! studio ingest "Sr. Recital Showcase.als" --org fasttrackaudio \
//!     --project "JT's Senior Recital" --form concert --link
//! studio ingest Crescendum/ --org fasttrackaudio --project "Crescendum" --form album
//! studio ingest Song/ --org fasttrackaudio --dry-run --json
//! ```

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use session_engine::ingest::{self, Media, Report, Request, Status};

/// `studio ingest`'s arguments.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// A Live set (`.als`, split into its songs), a song's folder,
    /// `.session` or `.RPP`, or a folder of song folders (an album) — in
    /// the order they play.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    /// The org whose library gets them (FTS_TASK_ORG when left out).
    #[arg(long)]
    pub org: Option<String>,
    /// The event or album project that holds them, with a child project
    /// per song.
    #[arg(long)]
    pub project: Option<String>,
    /// The project's form: album, concert, ep, lp, single, live-set.
    #[arg(long, requires = "project", value_parser = parse_form)]
    pub form: Option<ingest::Form>,
    /// The setlist that plays them, in input order (default: the
    /// project's name, else the one show's or folder's).
    #[arg(long)]
    pub setlist: Option<String>,
    /// Upload the WAVs too (the default).
    #[arg(long, conflicts_with = "proxies_only")]
    pub originals: bool,
    /// Leave the WAVs here: upload the projects, proxies and peaks only.
    #[arg(long)]
    pub proxies_only: bool,
    /// Mint a public live link to the setlist and print the browser's URL.
    #[arg(long)]
    pub link: bool,
    /// The Task server (default: the one this machine is signed in to).
    #[arg(long)]
    pub server: Option<String>,
    /// The Storage Location new roots go on (default: the org's one).
    #[arg(long)]
    pub location: Option<uuid::Uuid>,
    /// Where a Live set's songs are split to (default: `session/` beside
    /// the set).
    #[arg(long)]
    pub into: Option<PathBuf>,
    /// Do not bounce each song's reference.
    #[arg(long)]
    pub no_reference: bool,
    /// Prepare and compare only: say what would be uploaded.
    #[arg(long)]
    pub dry_run: bool,
    /// Files uploaded at once.
    #[arg(long, default_value_t = 4)]
    pub jobs: usize,
    /// Print the report as JSON.
    #[arg(long)]
    pub json: bool,
}

fn parse_form(raw: &str) -> Result<ingest::Form, String> {
    ingest::Form::parse(raw).ok_or_else(|| {
        format!("`{raw}` is not a project form: album, concert, ep, lp, single, live-set or song")
    })
}

/// Run `studio ingest`; blocks (call it from a thread that may wait).
///
/// # Errors
///
/// Nobody is signed in to the server, or the ingest failed (it says why).
pub fn run(args: &Args) -> eyre::Result<()> {
    use session_engine::task_account::AccountLibrary as _;
    let account = match &args.server {
        Some(server) => fts_account::Store::shared().and_then(|s| {
            s.load(&fts_account::normalize_server(server))
                .ok()
                .flatten()
        }),
        None => fts_account::signed_in(),
    }
    .ok_or_else(|| {
        eyre::eyre!(
            "not signed in to {} on this machine — sign in from the app first",
            args.server.clone().unwrap_or_else(fts_account::server)
        )
    })?;
    let org = args
        .org
        .clone()
        .or_else(|| std::env::var("FTS_TASK_ORG").ok())
        .filter(|o| !o.trim().is_empty())
        .ok_or_else(|| eyre::eyre!("which org? `--org <slug>` (or FTS_TASK_ORG)"))?;
    let library = account.library(&org);
    let data = dirs::data_local_dir().ok_or_else(|| eyre::eyre!("no data folder here"))?;
    let shown = Shown::new();
    let report = ingest::run(&library, &request_of(args), &data, &|s| shown.show(s))?;
    shown.done();
    if args.json {
        println!(
            "{}",
            ingest::to_json(&report).map_err(|e| eyre::eyre!("{e}"))?
        );
    } else {
        print_summary(&report);
    }
    if report.failures.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} file(s) did not upload",
            report.failures.len()
        ))
    }
}

fn request_of(args: &Args) -> Request {
    let mut request = Request::new(args.inputs.clone());
    request.project.clone_from(&args.project);
    request.form = args.form;
    request.setlist.clone_from(&args.setlist);
    request.media = if args.proxies_only {
        Media::ProxiesOnly
    } else {
        Media::Originals
    };
    request.link = args.link;
    request.location = args.location;
    request.into.clone_from(&args.into);
    request.reference = !args.no_reference;
    request.dry_run = args.dry_run;
    request.workers = args.jobs.max(1);
    request
}

/// Progress on stderr: a line when the phase or song changes, and the
/// upload's count at most a few times a second.
struct Shown {
    last: std::sync::Mutex<(String, Option<String>, Instant)>,
}

impl Shown {
    fn new() -> Self {
        Self {
            last: std::sync::Mutex::new((String::new(), None, Instant::now())),
        }
    }

    fn show(&self, s: &Status) {
        let Ok(mut last) = self.last.lock() else {
            return;
        };
        let moved = last.0 != s.phase || last.1 != s.song;
        if !moved && last.2.elapsed() < Duration::from_millis(500) {
            return;
        }
        let line = match (s.phase.as_str(), &s.song) {
            ("uploading", _) => format!(
                "uploading: {}/{} files, {} of {}",
                s.files_done,
                s.files,
                megabytes(s.bytes_sent),
                megabytes(s.bytes)
            ),
            ("preparing", Some(song)) => {
                format!("preparing {} ({}/{})", song, s.songs_done + 1, s.songs)
            }
            (phase, Some(song)) => format!("{phase}: {song}"),
            (phase, None) => phase.to_owned(),
        };
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "{line}");
        *last = (s.phase.clone(), s.song.clone(), Instant::now());
    }

    fn done(&self) {
        let _ = std::io::stderr().flush();
    }
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

fn print_summary(report: &Report) {
    println!(
        "{} {} songs into {} on {}{} in {:.1}s",
        if report.dry_run {
            "would ingest"
        } else {
            "ingested"
        },
        report.songs.len(),
        report.org,
        report.server,
        if report.dry_run { " (dry run)" } else { "" },
        report.seconds
    );
    for song in &report.songs {
        println!(
            "  {:<28} root {} ({}), {} files: {} {}, {} already there{}",
            song.title,
            if song.root.is_empty() {
                "-"
            } else {
                &song.root
            },
            song.root_made,
            song.files,
            if report.dry_run {
                song.to_send
            } else {
                song.sent
            },
            if report.dry_run { "to send" } else { "sent" },
            song.skipped,
            if song.checkpointed {
                ", checkpointed"
            } else {
                ""
            },
        );
        if !song.outside.is_empty() {
            println!(
                "    {} media outside the song's folder do not travel: {}",
                song.outside.len(),
                song.outside.join(", ")
            );
        }
        for why in &song.failed {
            println!("    failed: {why}");
        }
    }
    println!(
        "  sent {} files, {}; {} already there",
        report.files_sent,
        megabytes(report.bytes_sent),
        report.files_skipped
    );
    if let Some(project) = &report.project {
        println!(
            "  project {} ({}, {})",
            project.title, project.id, project.made
        );
    }
    if let Some(list) = &report.setlist {
        println!("  setlist {} ({}, {})", list.title, list.id, list.made);
    }
    if let Some(link) = &report.link {
        println!("  live link ({}): {}", link.made, link.url);
    }
}
