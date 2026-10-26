//! Split a show recorded as one Live set into its songs (see
//! `session_daw::show`): one folder per song under `<into>`, each holding the
//! song's own project file and links to the show's media. Prints each song
//! as `<slug>\t<name>\t<project file>\t<start s>\t<end s>`, in show order.
//!
//! ```sh
//! show-split "Show/Show.als" <into>
//! ```

use std::path::PathBuf;

fn main() -> eyre::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,session_daw=info")),
        )
        .init();
    let mut args = std::env::args_os().skip(1).map(PathBuf::from);
    let (Some(set), Some(into)) = (args.next(), args.next()) else {
        eyre::bail!("usage: show-split <show.als> <into>");
    };
    for song in session_daw::show::split(&set, &into)? {
        println!(
            "{}\t{}\t{}\t{:.3}\t{:.3}",
            song.slug,
            song.name,
            song.file.display(),
            song.start,
            song.end
        );
    }
    Ok(())
}
