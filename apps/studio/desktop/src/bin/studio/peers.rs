//! `studio peers` — this machine's Studio engine on the account's mesh.
//!
//! ```console
//! studio peers                         # the engine, its peers and what each runs
//! studio peers follow "iPad"           # follow a peer's song and transport
//! studio peers unfollow
//! studio peers sets                    # the rig's setlists
//! studio peers song "Sad But True"     # put a song up
//! studio peers transport play
//! studio peers set "Recital Band Set" "DTLA Cinematic Intro" "Drive To LA" …
//! studio peers cues "Sr. Recital Showcase.als" --track "GP GTR +CLICK" --write
//! ```
//!
//! Every verb but `cues` without `--write` asks the running engine
//! (`studio-desktop --engine`, `SIGNAL_ENGINE_URL`, default
//! `ws://127.0.0.1:4040/vox`); what the engine keeps syncs to the account's
//! other engines by itself.

use std::process::ExitCode;

use clap::Subcommand;
use signal_rigs_proto::perform::service::RigPerformClient;
use studio_peers::{Cue, PeersControlClient, PeersStatus, RigCues, SongCues};

#[derive(Subcommand)]
pub enum Command {
    /// This engine, the account's other engines and what each runs.
    Status,
    /// Follow a peer: its song, section, rig cues and transport drive this
    /// engine's guitar rig. By endpoint id (or its start) or name.
    Follow { peer: String },
    /// Stop following.
    Unfollow,
    /// The guitar rig's setlists and their songs, the one up marked.
    Sets,
    /// Put a song up on this engine's guitar rig, from whichever set has it.
    Song { title: String },
    /// This engine's transport: `play`, `stop`, or `seek <seconds>`.
    Transport {
        what: String,
        seconds: Option<f64>,
    },
    /// Make (or remake) a setlist of the guitar rig, songs in order. Songs
    /// the library lacks are added.
    Set {
        name: String,
        songs: Vec<String>,
    },
    /// Read a Live show set's cue track (clips named for patches or
    /// scenes) as each song's rig cues; `--write` keeps them in the rig.
    Cues {
        als: std::path::PathBuf,
        #[arg(long, default_value = "GP GTR +CLICK")]
        track: String,
        #[arg(long)]
        write: bool,
    },
}

fn engine_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL").unwrap_or_else(|_| "ws://127.0.0.1:4040/vox".into())
}

async fn client<C: vox_core::FromVoxLane>() -> Result<C, String> {
    architect::connect::pool()
        .client(&architect::connect::Endpoint::url(engine_url()))
        .await
        .map_err(|e| format!("the engine at {}: {e} (is `studio-desktop --engine` up?)", engine_url()))
}

pub async fn run(command: Command) -> ExitCode {
    let outcome = match command {
        Command::Status => status().await,
        Command::Follow { peer } => follow(peer).await,
        Command::Unfollow => async {
            client::<PeersControlClient>()
                .await?
                .unfollow()
                .await
                .map_err(|e| format!("{e:?}"))?;
            println!("not following");
            Ok(())
        }
        .await,
        Command::Set { name, songs } => set(name, songs).await,
        Command::Sets => sets().await,
        Command::Song { title } => song(&title).await,
        Command::Transport { what, seconds } => transport(&what, seconds).await,
        Command::Cues { als, track, write } => cues(&als, &track, write).await,
    };
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("{why}");
            ExitCode::FAILURE
        }
    }
}

fn print_status(s: &PeersStatus) {
    if !s.up {
        println!("not on the mesh: {}", s.why);
        return;
    }
    println!(
        "this engine  {}  {} ({})  runs {}",
        &s.id[..s.id.len().min(12)],
        s.about.name,
        s.about.platform,
        s.about.roles.describe()
    );
    println!("account pass {}", if s.account.is_empty() { "pending" } else { &s.account });
    if !s.about.following.is_empty() {
        println!("following    {}", &s.about.following[..s.about.following.len().min(12)]);
    }
    if !s.now.song.is_empty() {
        println!(
            "playing      {} / {} [{}] {} {:.1}s {:.0} bpm",
            s.now.setlist,
            s.now.song,
            s.now.section,
            if s.now.playing { "▶" } else { "■" },
            s.now.position,
            s.now.bpm
        );
    }
    if s.peers.is_empty() {
        println!("no other engine on this account yet");
    }
    for p in &s.peers {
        let runs = if p.reachable {
            p.about.roles.describe()
        } else {
            "not reachable".into()
        };
        println!(
            "peer         {}  {} ({})  {}",
            &p.endpoint[..p.endpoint.len().min(12)],
            p.name,
            p.platform,
            runs
        );
    }
}

async fn status() -> Result<(), String> {
    let s = client::<PeersControlClient>()
        .await?
        .status()
        .await
        .map_err(|e| format!("{e:?}"))?;
    print_status(&s);
    Ok(())
}

async fn follow(peer: String) -> Result<(), String> {
    let why = client::<PeersControlClient>()
        .await?
        .follow(peer.clone())
        .await
        .map_err(|e| format!("{e:?}"))?;
    if why.is_empty() {
        println!("following {peer}");
        Ok(())
    } else {
        Err(why)
    }
}

async fn song(title: &str) -> Result<(), String> {
    let perform: RigPerformClient = client().await?;
    let perform = architect::scope_client!(perform, signal_rigs_proto::Rig::Guitar.slug());
    let err = |e| format!("{e:?}");
    let library = perform.library().await.map_err(err)?;
    let order: Vec<usize> = library
        .setlists
        .iter()
        .position(|s| s.active)
        .into_iter()
        .chain(0..library.setlists.len())
        .collect();
    for si in order {
        let set = &library.setlists[si];
        let names: Vec<String> = set.songs.iter().map(|s| s.name.clone()).collect();
        if let Some(i) = studio_peers::follow::find_song(&names, title) {
            perform.select_setlist(si as u32).await.map_err(err)?;
            perform.select_song(i as u32).await.map_err(err)?;
            println!("{} / {}", set.name, names[i]);
            return Ok(());
        }
    }
    Err(format!("no set of this rig has {title:?}"))
}

fn debug(e: impl std::fmt::Debug) -> String {
    format!("{e:?}")
}

async fn transport(what: &str, seconds: Option<f64>) -> Result<(), String> {
    use daw_proto::ProjectContext::Current;
    let t: daw_proto::TransportClient = client().await?;
    match what {
        "play" => t.play(Current).await.map_err(debug)?.map_err(debug)?,
        "stop" => t.stop(Current).await.map_err(debug)?.map_err(debug)?,
        "seek" => t
            .set_position(Current, seconds.ok_or("seek needs seconds")?)
            .await
            .map_err(debug)?
            .map_err(debug)?,
        other => return Err(format!("{other:?}: play, stop or seek")),
    }
    println!("{what}");
    Ok(())
}

async fn sets() -> Result<(), String> {
    let perform: RigPerformClient = client().await?;
    let perform = architect::scope_client!(perform, signal_rigs_proto::Rig::Guitar.slug());
    let library = perform.library().await.map_err(|e| format!("{e:?}"))?;
    for set in &library.setlists {
        println!("{} {}", if set.active { "▶" } else { " " }, set.name);
        for (i, song) in set.songs.iter().enumerate() {
            println!("    {:>2}. {}", i + 1, song.name);
        }
    }
    Ok(())
}

/// Remake setlist `name` of the guitar rig with `songs`, in order.
async fn set(name: String, songs: Vec<String>) -> Result<(), String> {
    let perform: RigPerformClient = client().await?;
    let perform = architect::scope_client!(perform, signal_rigs_proto::Rig::Guitar.slug());
    let err = |e| format!("{e:?}");
    let library = perform.library().await.map_err(err)?;
    for song in &songs {
        if !library.songs.iter().any(|s| s.name == *song) {
            perform.add_song(song.clone(), String::new(), 0).await.map_err(err)?;
        }
    }
    let index = match library.setlists.iter().position(|s| s.name == name) {
        Some(i) => {
            for entry in (0..library.setlists[i].songs.len()).rev() {
                perform
                    .remove_setlist_entry(i as u32, entry as u32)
                    .await
                    .map_err(err)?;
            }
            i
        }
        None => {
            perform.add_setlist(name.clone()).await.map_err(err)?;
            let library = perform.library().await.map_err(err)?;
            library
                .setlists
                .iter()
                .position(|s| s.name == name)
                .ok_or_else(|| format!("the rig did not make {name:?}"))?
        }
    };
    for song in &songs {
        perform
            .add_setlist_entry(index as u32, song.clone())
            .await
            .map_err(err)?;
    }
    println!("{name}: {} songs", songs.len());
    Ok(())
}

/// The cues on `track` of the show at `als`, per song (its locators), in
/// seconds into each song.
fn read_cues(als: &std::path::Path, track: &str) -> Result<RigCues, String> {
    let set = ableton::read_live_set(als).map_err(|e| format!("{}: {e}", als.display()))?;
    let midi = set
        .midi_tracks
        .iter()
        .find(|t| t.common.effective_name.trim() == track.trim())
        .ok_or_else(|| format!("no MIDI track {track:?} in the set"))?;
    let map = ableton::tempo::TempoMap::of(&set);
    let mut clips: Vec<(f64, String)> = midi
        .arrangement_clips
        .iter()
        .filter(|c| !c.common.disabled && !c.common.name.trim().is_empty())
        .map(|c| (c.common.time, c.common.name.trim().to_owned()))
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    let songs = ableton::setlist::songs(&set)
        .into_iter()
        .filter_map(|song| {
            let cues: Vec<Cue> = clips
                .iter()
                .filter(|(beat, _)| *beat >= song.start_beats - 1e-6 && *beat < song.end_beats)
                .map(|(beat, name)| Cue {
                    at: (map.seconds_at(*beat) - song.start_seconds).max(0.0),
                    name: name.clone(),
                })
                .collect();
            (!cues.is_empty()).then(|| SongCues {
                song: song.name,
                cues,
            })
        })
        .collect();
    Ok(RigCues { songs })
}

async fn cues(als: &std::path::Path, track: &str, write: bool) -> Result<(), String> {
    let cues = read_cues(als, track)?;
    for song in &cues.songs {
        println!("{}: {} cues", song.song, song.cues.len());
        for cue in &song.cues {
            println!("  {:>7.2}s  {}", cue.at, cue.name);
        }
    }
    if write {
        let why = client::<PeersControlClient>()
            .await?
            .set_cues(cues)
            .await
            .map_err(|e| format!("{e:?}"))?;
        if !why.is_empty() {
            return Err(why);
        }
        println!("kept in the guitar rig's book");
    }
    Ok(())
}
