//! `studio setlist …`: bring songs into a library and engrave them.
//!
//! Setlists, browsing and sharing are the extension's actions, run in the
//! app (`STUDIO_SETLISTS_*`: `architect-desktop actions run
//! STUDIO_SETLISTS_CREATE …`).

use std::fmt::Write as _;
use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::chart::{SongFiles, import_song, reengrave};
use crate::layout::Library;
use crate::song::Song;
use crate::vault::Vault;

/// The vault and the library.
#[derive(Debug, Args)]
pub struct Cli {
    /// The vault (a folder of notes) the library is in.
    #[arg(long, env = "STUDIO_VAULT")]
    pub vault: PathBuf,
    /// The song library (`Libraries/<name>/`).
    #[arg(long, short, env = "STUDIO_LIBRARY")]
    pub library: String,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Bring a song in (or update it): its chart engraved, its files
    /// copied into its folder.
    Import {
        /// A chart, a .kf: the song's own, or `--arrangement`'s.
        chart: Option<PathBuf>,
        /// The arrangement the chart is (`Acoustic`).
        #[arg(long)]
        arrangement: Option<String>,
        #[arg(long)]
        lrc: Option<PathBuf>,
        /// Its session, .RPP or .session.
        #[arg(long)]
        session: Option<PathBuf>,
        /// Anything else of the song's, into its Media/ (repeat it).
        #[arg(long)]
        media: Vec<PathBuf>,
        /// A category of the library to put it in (repeat it).
        #[arg(long = "category")]
        categories: Vec<String>,
        /// Its title (default: the chart's).
        #[arg(long)]
        title: Option<String>,
    },
    /// Engrave a song's charts again after an edit to a .kf.
    Engrave { title: String },
    /// The library's songs.
    Songs,
}

fn line(s: &Song) -> String {
    let mut out = s.title.clone();
    let summary = s.summary();
    if !summary.is_empty() {
        let _ = write!(out, " — {summary}");
    }
    if !s.categories.is_empty() {
        let _ = write!(out, " [{}]", s.categories.join(", "));
    }
    for a in &s.arrangements {
        let _ = write!(out, " +{}", a.name);
    }
    out
}

impl Cli {
    /// Run it: what it did, a line a song.
    ///
    /// # Errors
    ///
    /// A file could not be read or written, or a chart did not engrave.
    pub fn run(self) -> crate::Result<String> {
        let vault = Vault::at(&self.vault);
        let lib = Library::named(&self.library);
        match self.command {
            Command::Import {
                chart,
                arrangement,
                lrc,
                session,
                media,
                categories,
                title,
            } => {
                let media: Vec<&std::path::Path> = media.iter().map(PathBuf::as_path).collect();
                let files = SongFiles {
                    chart: chart.as_deref(),
                    arrangement: arrangement.as_deref(),
                    lrc: lrc.as_deref(),
                    session: session.as_deref(),
                    media: &media,
                    categories: &categories,
                };
                import_song(&vault, &lib, &files, title.as_deref()).map(|s| line(&s))
            }
            Command::Engrave { title } => reengrave(&vault, &lib, &title).map(|s| line(&s)),
            Command::Songs => Ok(vault
                .songs(&lib)
                .iter()
                .map(line)
                .collect::<Vec<_>>()
                .join("\n")),
        }
    }
}
