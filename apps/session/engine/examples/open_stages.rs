//! Opening a song's project file into the engine, timed by stage — the
//! store, the doc, the model into the engine — and then whole, the way the
//! app opens it (`project_file::load`), one at a time and several at once:
//! every song given, all at once, or the first `--parallel` times.
//!
//! ```text
//! cargo run -p session-daw --example open_stages -- <Song.session> [more.session …] [--parallel N] [--rounds N]
//! cargo run -p session-daw --example open_stages --profile release-fast -- …
//! ```
//!
//! The file is copied into a temp dir first (opening a store writes
//! sidecars beside it); what sits beside the original (its media) is
//! linked in, read-only, so sources resolve as they do in place.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use daw::standalone::Standalone;
use session::sync::engine::{self, Blobs, Direct, EngineIds, MediaRoot};
use session::sync::{SessionDoc, diff};
use session_format::store::{MAIN, Store};

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut songs = Vec::new();
    let mut parallel = 4usize;
    let mut rounds = 3usize;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--parallel" => parallel = args.next().and_then(|n| n.parse().ok()).unwrap_or(4),
            "--rounds" => rounds = args.next().and_then(|n| n.parse().ok()).unwrap_or(3),
            _ => songs.push(PathBuf::from(arg)),
        }
    }
    let song = songs
        .first()
        .cloned()
        .ok_or_else(|| eyre::eyre!("usage: open_stages <Song.session> …"))?;
    let scratch = tempfile::tempdir()?;
    println!("song      {}", song.display());
    println!(
        "profile   {}",
        if cfg!(debug_assertions) {
            "dev"
        } else {
            "optimised"
        }
    );

    for round in 0..rounds {
        let copy = copy_song(&song, &scratch.path().join(format!("staged-{round}")))?;
        stages(&copy)?;
    }

    let daw = engine_with_fx();
    for round in 0..rounds {
        let copy = copy_song(&song, &scratch.path().join(format!("whole-{round}")))?;
        let t = Instant::now();
        session_engine::project_file::load(&daw, "song", &copy, MAIN).map_err(|e| eyre::eyre!(e))?;
        println!("whole     project_file::load            {:>8.1} ms", ms(t));
    }

    let set: Vec<&PathBuf> = if songs.len() > 1 {
        songs.iter().collect()
    } else {
        std::iter::repeat_n(&song, parallel).collect()
    };
    let parallel = set.len();
    let copies = set
        .iter()
        .enumerate()
        .map(|(i, song)| copy_song(song, &scratch.path().join(format!("parallel-{i}"))))
        .collect::<eyre::Result<Vec<_>>>()?;
    let t = Instant::now();
    let each: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = copies
            .iter()
            .map(|copy| {
                let daw = daw.clone();
                scope.spawn(move || {
                    let t = Instant::now();
                    session_engine::project_file::load(&daw, "song", copy, MAIN)
                        .map(|_| ms(t))
                        .map_err(|e| eyre::eyre!(e))
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok().and_then(Result::ok))
            .collect()
    });
    println!(
        "parallel  {parallel} at once: wall {:>8.1} ms, each {:?}",
        ms(t),
        each.iter().map(|m| m.round()).collect::<Vec<_>>()
    );
    Ok(())
}

/// One open, stage by stage, into a fresh engine.
fn stages(file: &Path) -> eyre::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_stack_size(16 * 1024 * 1024)
        .enable_all()
        .build()?;
    let mut t = Instant::now();
    let mut lap = |label: &str| {
        println!("stage     {label:<32} {:>8.1} ms", ms(t));
        t = Instant::now();
    };
    let store = Arc::new(Mutex::new(Store::open(file)?));
    lap("store open");
    let loaded = store.lock().map_err(|_| eyre::eyre!("store"))?.load(MAIN)?;
    lap("branch load (loro)");
    let doc = SessionDoc::from_loro(loaded);
    let target = doc.read();
    lap("doc read (model)");
    println!(
        "          {} tracks, {} items, {} markers, {} regions",
        target.tracks.len(),
        target.items.len(),
        target.markers.len(),
        target.regions.len()
    );
    let mut fx: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for track in &target.tracks {
        for f in track.fx.iter().chain(&track.input_fx) {
            *fx.entry(f.plugin.as_str()).or_default() += 1;
        }
    }
    for f in target
        .project
        .master
        .fx
        .iter()
        .chain(&target.project.monitoring_fx)
    {
        *fx.entry(f.plugin.as_str()).or_default() += 1;
    }
    println!("          fx: {fx:?}");
    let daw = engine_with_fx();
    let guid = daw.seed_project(daw_proto::ProjectInfo {
        guid: "song".into(),
        name: "song".into(),
        path: file.to_string_lossy().into_owned(),
    });
    let media = MediaRoot(file.parent().map(Path::to_path_buf).unwrap_or_default());
    let blobs = StoreBlobs(store);
    let direct: Arc<dyn Direct> = Arc::new(daw.clone());
    rt.block_on(async {
        let facade = daw::standalone::bootstrap::build_in_process_daw(daw.clone()).await?;
        let project = facade.daw.project(guid).await?;
        lap("facade");
        let mut ids = EngineIds::default();
        let empty = engine::read(&project, &media, &blobs, &ids, Some(direct.as_ref())).await?;
        lap("engine read (empty)");
        let changes = diff(&empty, &target);
        lap("diff");
        engine::apply(
            &project,
            &media,
            &changes,
            &target,
            &blobs,
            &mut ids,
            Some(direct.as_ref()),
        )
        .await?;
        lap("model -> engine (apply)");
        let shown =
            engine::read_keeping(&project, &media, &blobs, &ids, Some(direct.as_ref()), None)
                .await?;
        lap("engine read-back");
        eyre::ensure!(shown.tracks.len() == target.tracks.len(), "tracks lost");
        Ok::<_, eyre::Report>(())
    })
}

/// An engine with the FTS processors built in, as the app's has.
fn engine_with_fx() -> Standalone {
    let daw = Standalone::new();
    daw.set_fx_factory(Arc::new(builtin::engine::BuiltinFx));
    daw
}

/// `song` copied into `dir`, everything beside it linked in.
fn copy_song(song: &Path, dir: &Path) -> eyre::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = song
        .file_name()
        .ok_or_else(|| eyre::eyre!("no file name"))?;
    let folder = song.parent().ok_or_else(|| eyre::eyre!("no folder"))?;
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let entry_name = entry.file_name();
        let text = entry_name.to_string_lossy();
        if text.starts_with(&*name.to_string_lossy()) {
            // The file itself and its sidecars: copied (the shm is not —
            // a fresh one is made).
            if !text.ends_with("-tshm") {
                std::fs::copy(entry.path(), dir.join(&entry_name))?;
            }
        } else {
            std::os::unix::fs::symlink(entry.path(), dir.join(&entry_name))?;
        }
    }
    Ok(dir.join(name))
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// The store's blobs, as the app's bridge keeps FX state.
struct StoreBlobs(Arc<Mutex<Store>>);

impl Blobs for StoreBlobs {
    fn put(&self, bytes: &[u8]) -> String {
        let name = engine::blob_name(bytes);
        if let Ok(store) = self.0.lock() {
            let _ = store.put_blob("fx", bytes);
        }
        name
    }
    fn get(&self, name: &str) -> Option<Vec<u8>> {
        self.0.lock().ok()?.blob(name).ok()
    }
}
