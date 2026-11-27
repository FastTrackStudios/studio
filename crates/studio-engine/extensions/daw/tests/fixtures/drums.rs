//! An agent redoes a drum mix over MCP (studio#696): the `drum-kit`
//! fixture, mixed step by step as the `drum-mixing` skill has it, every
//! step an action through the engine's MCP door and every result measured
//! from rendered audio, never read from inside the engine.
//!
//! 1. hit markers from the triggers (`HITS_DETECT`);
//! 2. polarity and time alignment of each piece's mics (`LISTEN_ALIGN`):
//!    the sum gains;
//! 3. drum leveling (`DRUMS_LEVEL`): the strokes come closer together;
//! 4. gates keyed from the hit markers (a Signal `gate` keyed by the
//!    piece's trigger): the bleed between the piece's hits goes;
//! 5. the kit at −23 LUFS, locked (`MIX_MEASURE`, then the `Drums` fader);
//! 6. Signal tone on the kit (an EQ, level-matched on its phase's
//!    make-up): it changes the sound, not the level;
//! 7. the mix then changed by hand, as the mixer window changes it (its
//!    fader commit and its Signal editor send these same actions).
//!
//! Every answer goes into `target/<triple>/<profile>/fixtures/runs/
//! drum-mix/answers.json`, the checks into `fixtures/drum-mix.json`.

use std::path::Path;

use serde_json::{Value, json};

use crate::check::Checks;
use crate::engine::Engine;
use crate::fixture::Fixture;
use crate::mcp::Mcp;

/// The kit's integrated loudness once locked, LUFS, and how near.
const KIT_LUFS: f64 = -23.0;
const KIT_WITHIN: f64 = 0.1;
/// The kit's span: the whole song.
const FROM: &str = "Groove";
const TO: &str = "Groove 2 end";
/// The strokes each piece is hit (`fixture.toml`: 8 bars, twice a bar).
const STROKES: usize = 16;
/// The least an alignment must gain the piece's sum, dB of `score`.
const ALIGN_GAIN_DB: f64 = 1.0;
/// How much closer leveling must bring the strokes: the spread of their
/// peaks after, as a share of before.
const LEVELED_SHARE: f64 = 0.6;
/// The least a gate must take off the bleed between the piece's hits, dB.
const GATE_DROP_DB: f64 = 10.0;
/// A gate keyed by the hits must pass them: a stroke as loud through the
/// gate as before it, within this, dB.
const HIT_KEPT_DB: f64 = 1.0;
/// How long a newly keyed gate is given for Studio to send it the song's
/// placed hits (on its next turn for plugins, not inside the call that
/// keyed it): a render started sooner copies a gate that has none yet and
/// shuts every hit. The engine should hand a render's copy the hits itself.
const PLACED_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Where a piece's gate is heard: its folder over one of its strokes, and
/// where it hears only the other drum's bleed, early in the song and past
/// the ten seconds after which a gate that has lost its hits opens on its
/// own input (`fixture.toml`: kicks on 1 and 3, snares on 2 and 4, 0.5 s
/// beats).
struct GateWindows {
    piece: &'static str,
    hit: (&'static str, &'static str),
    bleed: [(&'static str, &'static str); 2],
}

const GATES: [GateWindows; 2] = [
    GateWindows {
        piece: "Kick",
        hit: ("0s", "0.15s"),
        bleed: [("0.55s", "0.95s"), ("12.55s", "12.95s")],
    },
    GateWindows {
        piece: "Snare",
        hit: ("0.5s", "0.65s"),
        bleed: [("1.05s", "1.45s"), ("13.05s", "13.45s")],
    },
];
/// Signal, as an agent adds it: built into the engine.
const SIGNAL: &str = "Signal Dev";

/// Every call and its answer, as the run went.
#[derive(Default)]
pub struct Answers(Vec<Value>);

impl Answers {
    /// Run `id` and keep what it answered.
    fn run(&mut self, mcp: &Mcp, id: &str, args: &Value) -> Result<Value, String> {
        let answer = mcp.run(id, args);
        self.0.push(match &answer {
            Ok(a) => json!({"id": id, "args": args, "answer": a}),
            Err(e) => json!({"id": id, "args": args, "error": e}),
        });
        answer
    }

    fn write(&self, file: &Path) {
        let _ = std::fs::write(file, format!("{:#}\n", Value::Array(self.0.clone())));
    }
}

/// Mix the opened `drum-kit` fixture, step by step.
pub fn mix(engine: &Engine, run: &Path, checks: &mut Checks) {
    let mut log = Answers::default();
    let mcp = &engine.mcp;
    let steps: [(&str, Step); 7] = [
        ("hit markers", hit_markers),
        ("alignment", alignment),
        ("leveling", leveling),
        ("gates", gates),
        ("lock", lock),
        ("tone", tone),
        ("by hand", by_hand),
    ];
    for (name, step) in steps {
        if let Err(why) = step(mcp, &mut log, checks) {
            checks.failed(name, why);
        }
    }
    log.write(&run.join("answers.json"));
}

type Step = fn(&Mcp, &mut Answers, &mut Checks) -> Result<(), String>;

/// The fixture the steps mix.
pub fn fixture() -> Fixture {
    Fixture::load("drum-kit").unwrap_or_else(|e| panic!("{e}"))
}

/// 1. Every stroke found once, from the triggers, as a hit marker.
fn hit_markers(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    log.run(mcp, "SESSION_MODE", &json!({"mode": "Mix"}))?;
    log.run(mcp, "MIX_SNAPSHOT", &json!({"name": "before-drum-mix"}))?;
    log.run(mcp, "HITS_DETECT", &json!({}))?;
    for piece in ["Kick", "Snare"] {
        let listed = log.run(
            mcp,
            "HITS_LIST",
            &json!({"pieces": [piece], "playing": true, "limit": 500}),
        )?;
        let n = rows(&listed).len();
        checks.add(
            format!("{piece} hits"),
            n == STROKES,
            format!("{n} hit markers (struck {STROKES} times)"),
        );
    }
    Ok(())
}

/// 2. Each piece's mics lined up and turned over where the sum gains: a
/// dry search first, Signal put on each mic whose fix is more than a
/// polarity (its `align` block makes the move), then the fixes applied
/// and heard again.
fn alignment(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    for piece in ["Kick", "Snare"] {
        let ask = |apply: bool| json!({"piece": piece, "from": FROM, "to": TO, "max_ms": 8, "apply": apply, "png": false});
        let found = log.run(mcp, "LISTEN_ALIGN", &ask(false))?;
        for mic in rows_of(&found, "mics") {
            let chosen = &mic["chosen"];
            let moves = chosen["delay_samples"].as_i64().is_some_and(|d| d != 0)
                || chosen["allpass"].as_array().is_some_and(|a| !a.is_empty());
            if let (true, Some(path)) = (moves, mic["path"].as_str()) {
                log.run(mcp, "FX_ADD", &json!({"track": path, "fx": SIGNAL}))?;
            }
        }
        let aligned = log.run(mcp, "LISTEN_ALIGN", &ask(true))?;
        let apply = &aligned["apply"];
        let gained = apply.pointer("/measured/score").and_then(Value::as_f64);
        checks.add(
            format!("{piece} aligned"),
            apply["kept"] == json!(true) && gained.is_some_and(|g| g >= ALIGN_GAIN_DB),
            format!(
                "the sum gained {} dB (level {}, low {}, attack {}; predicted {}), kept {}, not applied {}",
                fmt(gained),
                fmt(apply.pointer("/measured/level").and_then(Value::as_f64)),
                fmt(apply.pointer("/measured/low").and_then(Value::as_f64)),
                fmt(apply.pointer("/measured/attack").and_then(Value::as_f64)),
                fmt(aligned.pointer("/predicted/score").and_then(Value::as_f64)),
                apply["kept"],
                apply["not_applied"],
            ),
        );
    }
    log.run(mcp, "MIX_SNAPSHOT", &json!({"name": "aligned"}))?;
    Ok(())
}

/// 3. The strokes evened out toward each drum's median.
fn leveling(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    let before = spreads(mcp, log)?;
    let leveled = log.run(mcp, "DRUMS_LEVEL", &json!({}))?;
    let after = spreads(mcp, log)?;
    for ((piece, was), (_, now)) in before.iter().zip(&after) {
        checks.add(
            format!("{piece} leveled"),
            *now <= was * LEVELED_SHARE,
            format!("stroke peaks spread {was:.2} dB → {now:.2} dB"),
        );
    }
    let checkpoint = rows(&leveled)
        .iter()
        .find_map(|r| r["checkpoint"].as_str().map(str::to_owned))
        .or_else(|| leveled["checkpoint"].as_str().map(str::to_owned));
    checks.add(
        "leveling checkpoint",
        checkpoint.is_some(),
        format!("history mark {checkpoint:?}"),
    );
    Ok(())
}

/// How much each piece's strokes differ: the standard deviation of their
/// peaks, at the piece's sum, over the whole song.
fn spreads(mcp: &Mcp, log: &mut Answers) -> Result<Vec<(&'static str, f64)>, String> {
    ["Kick", "Snare"]
        .into_iter()
        .map(|piece| {
            let hits = log.run(
                mcp,
                "LISTEN_HITS",
                &json!({"piece": piece, "from": FROM, "to": TO, "png": false}),
            )?;
            let spread = hits
                .pointer("/sum/figures/spread/peak_db")
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("LISTEN_HITS {piece}: no spread in {}", short(&hits)))?;
            Ok((piece, spread))
        })
        .collect()
}

/// 4. Each piece folder gated, keyed from its trigger's hit markers: the
/// template's gate (keyed by the trigger's audio) switched off, a Signal
/// gate keyed by the trigger's lane (the hits Studio places) in its place.
/// Each folder heard over a stroke and over the other drum's bleed, before
/// and after.
fn gates(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    let heard = |log: &mut Answers, g: &GateWindows| -> Result<Vec<Option<f64>>, String> {
        let folder = format!("Drums/{}", g.piece);
        std::iter::once(g.hit)
            .chain(g.bleed)
            .map(|(from, to)| {
                log.run(
                    mcp,
                    "LISTEN",
                    &json!({"track": folder, "from": from, "to": to, "exact": true, "png": false}),
                )
                .map(|heard| heard.pointer("/figures/rms").and_then(Value::as_f64))
            })
            .collect()
    };
    for g in &GATES {
        template_gates_off(mcp, log, &format!("Drums/{}", g.piece))?;
    }
    let before = GATES
        .iter()
        .map(|g| heard(log, g))
        .collect::<Result<Vec<_>, _>>()?;
    for g in &GATES {
        let folder = format!("Drums/{}", g.piece);
        let trigger = format!("{folder}/Sum/Trig");
        log.run(mcp, "FX_ADD", &json!({"track": folder, "fx": SIGNAL}))?;
        log.run(
            mcp,
            "SIGNAL_RUN",
            &json!({"track": folder, "action": "SIGNAL_BLOCK_ADD",
                    "args": {"phase": "rescue", "kind": "gate", "key": trigger}}),
        )?;
        // Keyed by the trigger, not fallen back to its own input. Whether
        // the hits reach it is heard below: `via` says `placed` only once
        // the live instance has played, and the engine here never plays.
        let chain = log.run(mcp, "SIGNAL_CHAIN", &json!({"track": folder}))?;
        let key = find_block(&chain, "gate")
            .map(|b| b["key"].clone())
            .ok_or_else(|| format!("no gate on {folder}: {}", short(&chain)))?;
        checks.add(
            format!("{} gate keyed", g.piece),
            key["from"].as_str() == Some(&trigger) && key["via"] != "self",
            format!("its gate's key: {key}"),
        );
    }
    std::thread::sleep(PLACED_WAIT);
    for (g, was) in GATES.iter().zip(&before) {
        let now = heard(log, g)?;
        checks.add(
            format!("{} gate passes its hit", g.piece),
            was[0]
                .zip(now[0])
                .is_some_and(|(b, a)| (b - a).abs() <= HIT_KEPT_DB),
            format!(
                "over a stroke ({}..{}): {} → {} dBFS RMS",
                g.hit.0,
                g.hit.1,
                fmt(was[0]),
                fmt(now[0])
            ),
        );
        for ((span, b), a) in g.bleed.iter().zip(&was[1..]).zip(&now[1..]) {
            checks.add(
                format!("{} gate takes the bleed at {}", g.piece, span.0),
                b.zip(*a).is_some_and(|(b, a)| b - a >= GATE_DROP_DB),
                format!(
                    "between its hits ({}..{}): {} → {} dBFS RMS",
                    span.0,
                    span.1,
                    fmt(*b),
                    fmt(*a)
                ),
            );
        }
    }
    log.run(mcp, "MIX_SNAPSHOT", &json!({"name": "gated"}))?;
    Ok(())
}

/// Switch off the gates the template put on `folder` (keyed by its
/// trigger's audio), as an agent does before keying one from the hits.
fn template_gates_off(mcp: &Mcp, log: &mut Answers, folder: &str) -> Result<(), String> {
    let state = log.run(
        mcp,
        "MIX_STATE",
        &json!({"track": folder, "depth": 0, "params": false, "from": "1", "to": "1"}),
    )?;
    for fx in rows_of(&state["tracks"][0], "fx") {
        if fx["on"] == json!(true) && fx["name"].as_str().is_some_and(|n| n.contains("Gate")) {
            log.run(
                mcp,
                "FX_ENABLE",
                &json!({"track": folder, "fx": fx["place"], "on": false}),
            )?;
        }
    }
    Ok(())
}

/// The kit's loudness over the whole song, LUFS.
fn kit_lufs(mcp: &Mcp, log: &mut Answers) -> Result<f64, String> {
    let measured = log.run(
        mcp,
        "MIX_MEASURE",
        &json!({"from": FROM, "to": TO, "tracks": ["Drums"], "master": false}),
    )?;
    rows_of(&measured, "tracks")
        .iter()
        .find(|t| t["path"] == "Drums")
        .and_then(|t| t.pointer("/numbers/lufs").and_then(Value::as_f64))
        .ok_or_else(|| format!("MIX_MEASURE: no Drums loudness in {}", short(&measured)))
}

/// The `Drums` fader, dB.
fn drums_fader(mcp: &Mcp, log: &mut Answers) -> Result<f64, String> {
    let state = log.run(
        mcp,
        "MIX_STATE",
        &json!({"track": "Drums", "depth": 0, "params": false, "from": "1", "to": "1"}),
    )?;
    volume_db(&state).ok_or_else(|| format!("MIX_STATE Drums: no volume in {}", short(&state)))
}

/// 5. The kit set to −23 LUFS on its folder's fader.
fn lock(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    let was = kit_lufs(mcp, log)?;
    let fader = drums_fader(mcp, log)?;
    let set = fader + (KIT_LUFS - was);
    log.run(
        mcp,
        "TRACK_SET",
        &json!({"track": "Drums", "volume_db": set}),
    )?;
    let now = kit_lufs(mcp, log)?;
    checks.add(
        "kit at -23 LUFS",
        (now - KIT_LUFS).abs() <= KIT_WITHIN,
        format!("{was:.2} → {now:.2} LUFS, the Drums fader {fader:.2} → {set:.2} dB"),
    );
    log.run(mcp, "MIX_SNAPSHOT", &json!({"name": "locked"}))?;
    Ok(())
}

/// 6. Signal tone on the kit, gain neutral: the lock holds.
fn tone(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    log.run(mcp, "FX_ADD", &json!({"track": "Drums", "fx": SIGNAL}))?;
    log.run(
        mcp,
        "SIGNAL_RUN",
        &json!({"track": "Drums", "action": "SIGNAL_BLOCK_ADD", "args": {"phase": "tone", "kind": "eq"}}),
    )?;
    let chain = log.run(mcp, "SIGNAL_CHAIN", &json!({"track": "Drums"}))?;
    let eq = find_block(&chain, "eq")
        .and_then(|b| b["id"].as_str().map(str::to_owned))
        .ok_or_else(|| format!("no eq on Drums: {}", short(&chain)))?;
    // A smile: the kick's sub up, the snare's box down, air up.
    log.run(
        mcp,
        "SIGNAL_BLOCK_SET",
        &json!({"track": "Drums", "block": eq, "params": {
            "band.1.used": true, "band.1.shape": "Low Shelf", "band.1.freq": 60, "band.1.gain": 3,
            "band.2.used": true, "band.2.shape": "Bell", "band.2.freq": 450, "band.2.gain": -4,
            "band.3.used": true, "band.3.shape": "High Shelf", "band.3.freq": 8000, "band.3.gain": 3,
        }}),
    )?;
    let heard = log.run(
        mcp,
        "LISTEN_BLOCK",
        &json!({"track": "Drums", "block": eq, "from": FROM, "to": TO, "exact": true}),
    )?;
    let lufs = heard
        .pointer("/difference/lufs")
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("LISTEN_BLOCK: no difference in {}", short(&heard)))?;
    let bands = heard
        .pointer("/difference/bands")
        .and_then(Value::as_array)
        .map(|b| b.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
        .unwrap_or_default();
    let reshaped = bands.iter().copied().map(f64::abs).fold(0.0_f64, f64::max);
    checks.add(
        "tone changes the sound",
        reshaped >= 1.5,
        format!("the EQ moves a band by up to {reshaped:.1} dB, the level {lufs:+.2} LU"),
    );
    log.run(
        mcp,
        "SIGNAL_RUN",
        &json!({"track": "Drums", "action": "SIGNAL_SET", "args": {"phase": "tone", "makeup_db": -lufs}}),
    )?;
    let now = kit_lufs(mcp, log)?;
    checks.add(
        "kit still at -23 LUFS",
        (now - KIT_LUFS).abs() <= KIT_WITHIN,
        format!(
            "{now:.2} LUFS with the tone, level-matched by {:+.2} dB",
            -lufs
        ),
    );
    log.run(mcp, "MIX_SNAPSHOT", &json!({"name": "drum-mix"}))?;
    Ok(())
}

/// 7. The agent's mix changed by hand: the mixer window's fader commit
/// (`TRACK_SET` by GUID, `mixer_panel::commit_turn`) and its Signal
/// editor's parameter edit (`SIGNAL_RUN` of Signal's own action,
/// `signal::engine::signal_action`), each read back as the window reads
/// it.
fn by_hand(mcp: &Mcp, log: &mut Answers, checks: &mut Checks) -> Result<(), String> {
    let guid = track_guid(mcp, log, "Drums/Snare")?;
    let was = volume_db(&log.run(
        mcp,
        "MIX_STATE",
        &json!({"track": guid, "depth": 0, "params": false, "from": "1", "to": "1"}),
    )?)
    .unwrap_or(0.0);
    let wanted = was - 1.5;
    log.run(
        mcp,
        "TRACK_SET",
        &json!({"track": guid, "volume_db": wanted}),
    )?;
    let now = volume_db(&log.run(
        mcp,
        "MIX_STATE",
        &json!({"track": guid, "depth": 0, "params": false, "from": "1", "to": "1"}),
    )?);
    checks.add(
        "fader by hand",
        now.is_some_and(|n| (n - wanted).abs() < 0.01),
        format!("Snare {was:.2} → {} dB (asked {wanted:.2})", fmt(now)),
    );
    let drums = track_guid(mcp, log, "Drums")?;
    let chain = log.run(mcp, "SIGNAL_CHAIN", &json!({"track": drums}))?;
    let eq = find_block(&chain, "eq")
        .and_then(|b| b["id"].as_str().map(str::to_owned))
        .ok_or("no eq on Drums")?;
    log.run(
        mcp,
        "SIGNAL_RUN",
        &json!({"track": drums, "action": "SIGNAL_BLOCK_SET",
                "args": {"block": eq, "params": {"band.3.gain": 1.5}}}),
    )?;
    let chain = log.run(mcp, "SIGNAL_CHAIN", &json!({"track": drums}))?;
    let gain = find_block(&chain, "eq").and_then(|b| param(b, "band.3.gain"));
    checks.add(
        "Signal by hand",
        gain.is_some_and(|g| (g - 1.5).abs() < 0.01),
        format!("the kit's air band now {} dB (asked 1.5)", fmt(gain)),
    );
    Ok(())
}

/// The GUID of the track at `path`, as the mixer window names a track.
fn track_guid(mcp: &Mcp, log: &mut Answers, path: &str) -> Result<String, String> {
    let tracks = log.run(mcp, "SESSION_TRACKS", &json!({"filter": path}))?;
    let name = path.rsplit('/').next().unwrap_or(path);
    rows(&tracks)
        .iter()
        .find(|t| t["path"] == path || (t["name"] == name && t["folder"] == json!(true)))
        .and_then(|t| t["guid"].as_str().map(str::to_owned))
        .ok_or_else(|| format!("SESSION_TRACKS: no {path} in {}", short(&tracks)))
}

// ── Reading answers ─────────────────────────────────────────────────────

/// An answer's rows: itself when a list, else its `rows`, `hits` or
/// `pieces`.
fn rows(v: &Value) -> Vec<Value> {
    v.as_array()
        .or_else(|| v["hits"].as_array())
        .or_else(|| v["rows"].as_array())
        .or_else(|| v["pieces"].as_array())
        .cloned()
        .unwrap_or_default()
}

fn rows_of(v: &Value, key: &str) -> Vec<Value> {
    v[key].as_array().cloned().unwrap_or_else(|| rows(v))
}

/// The first block of `kind` in a signal.chain/1 answer.
fn find_block(chain: &Value, kind: &str) -> Option<Value> {
    let chain = chain.get("chain").unwrap_or(chain);
    let phases = chain["phases"]
        .as_array()
        .cloned()
        .or_else(|| {
            chain
                .pointer("/lanes/0/phases")
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default();
    phases
        .iter()
        .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
        .find(|b| b["kind"].as_str() == Some(kind))
}

/// A block's parameter value, by its id.
fn param(block: Value, id: &str) -> Option<f64> {
    block["params"]
        .as_array()?
        .iter()
        .find(|p| p["id"].as_str() == Some(id))
        .and_then(|p| p["value"].as_f64().or_else(|| p["plain"].as_f64()))
}

/// A MIX_STATE answer's fader, dB.
fn volume_db(state: &Value) -> Option<f64> {
    state.pointer("/tracks/0/volume_db").and_then(Value::as_f64)
}

fn fmt(v: Option<f64>) -> String {
    v.map_or_else(|| "none".to_owned(), |v| format!("{v:.2}"))
}

/// An answer, cut short for a message.
fn short(v: &Value) -> String {
    let s = v.to_string();
    if s.len() > 400 {
        format!("{}…", &s[..s.floor_char_boundary(400)])
    } else {
        s
    }
}
