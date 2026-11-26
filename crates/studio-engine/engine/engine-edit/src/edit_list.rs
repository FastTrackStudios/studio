//! Hit change lists (`studio.drum-edit/1`): a drum edit written down so
//! it can be replayed after the hits are found again.
//!
//! Hit ids change with every detection run, so a list names each hit by
//! what does not change: its drum (`piece`), its take (`take`, the hit
//! id's code after `~`) and where it was recorded (`recorded`, seconds).
//! `to` is where it plays in the edit, `at` the same as bar.beat (for a
//! reader). `DRUMS_EDIT_EXPORT` writes the edit as it plays; `DRUMS_APPLY
//! {list}` matches a list to the markers ([`match_list`], ±2 ms) and
//! applies it: a detected hit the list does not have was a false
//! detection the edit took out, and is removed.
//!
//! Bar.beat addresses ([`parse_at`], [`format_at`]) are how the owner
//! names every correction: `118.4.25` is bar 118, beat 4 and a quarter.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::ProjectContext;
use facet::Facet;

use crate::actions::meta;
use crate::drum_apply::Stroke;

/// The list format's name.
pub const FORMAT: &str = "studio.drum-edit/1";
/// How near its recorded time a detected hit matches a list entry, s.
pub const MATCH: f64 = 0.002;

/// One hit of a list.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ListHit {
    pub piece: String,
    /// The hit id's take code (after `~`); empty for an id without one.
    pub take: String,
    pub recorded: f64,
    pub to: f64,
    #[serde(default)]
    pub at: String,
}

/// A section, for a reader.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ListSection {
    pub name: String,
    pub from: f64,
    pub to: f64,
    #[serde(default)]
    pub bars: Vec<i32>,
}

/// The song's tempo where bar 1 starts, for a reader.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ListTempo {
    pub bpm: f64,
    pub meter: String,
    pub bar_1_at: f64,
}

/// A change list.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EditList {
    pub format: String,
    #[serde(default)]
    pub song: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub made: String,
    #[serde(default)]
    pub about: String,
    #[serde(default)]
    pub tempo: Option<ListTempo>,
    #[serde(default)]
    pub sections: Vec<ListSection>,
    pub hits: Vec<ListHit>,
}

/// Read a list.
///
/// # Errors
/// It cannot be read, is not JSON of the format, or names another.
pub fn read(path: &Path) -> Result<EditList, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let list: EditList =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if list.format != FORMAT {
        return Err(format!(
            "{}: format {:?}, not {FORMAT}",
            path.display(),
            list.format
        ));
    }
    Ok(list)
}

/// A hit id's take code: what follows its `~`.
#[must_use]
pub fn take_of(id: &str) -> &str {
    id.rsplit_once(crate::hits::TAKE_TAG).map_or("", |(_, t)| t)
}

/// What a list matched among the strokes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListMatch {
    /// Hit id → where the list has it play.
    pub targets: BTreeMap<String, f64>,
    /// List entries no stroke matched.
    pub unmatched: Vec<ListHit>,
    /// Strokes no entry matched: removed.
    pub removed: Vec<String>,
}

/// Match `list` to `strokes` one to one: each entry to the stroke of its
/// drum and take recorded nearest it, within [`MATCH`]; the nearest pairs
/// first, so two close hits never swap.
#[must_use]
pub fn match_list(list: &EditList, strokes: &[Stroke]) -> ListMatch {
    // Strokes by (piece, take), by recorded time.
    let mut by_key: BTreeMap<(String, &str), Vec<(f64, usize)>> = BTreeMap::new();
    for (k, s) in strokes.iter().enumerate() {
        by_key
            .entry((s.piece.to_lowercase(), take_of(&s.id)))
            .or_default()
            .push((s.recorded, k));
    }
    for v in by_key.values_mut() {
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
    }
    // Every candidate pair within reach, nearest first.
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (e, h) in list.hits.iter().enumerate() {
        let Some(cands) = by_key.get(&(h.piece.to_lowercase(), h.take.as_str())) else {
            continue;
        };
        let from = cands.partition_point(|(t, _)| *t < h.recorded - MATCH);
        for (t, k) in cands.iter().skip(from) {
            if *t > h.recorded + MATCH {
                break;
            }
            pairs.push(((t - h.recorded).abs(), e, *k));
        }
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut used_e, mut used_k) = (HashSet::new(), HashSet::new());
    let mut out = ListMatch::default();
    for (_, e, k) in pairs {
        if used_e.contains(&e) || used_k.contains(&k) {
            continue;
        }
        used_e.insert(e);
        used_k.insert(k);
        if let (Some(h), Some(s)) = (list.hits.get(e), strokes.get(k)) {
            out.targets.insert(s.id.clone(), h.to);
        }
    }
    out.unmatched = list
        .hits
        .iter()
        .enumerate()
        .filter(|(e, _)| !used_e.contains(e))
        .map(|(_, h)| h.clone())
        .collect();
    out.removed = strokes
        .iter()
        .enumerate()
        .filter(|(k, _)| !used_k.contains(k))
        .map(|(_, s)| s.id.clone())
        .collect();
    out
}

// ── bar.beat ──────────────────────────────────────────────────────────

/// A bar.beat address: bar, beat (from 1) and the fraction of the beat.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarBeat {
    pub bar: i32,
    pub beat: i32,
    pub fraction: f64,
}

/// `118.4.25` → bar 118, beat 4, a quarter of it; `150.1` → bar 150 beat
/// 1; `8.3.444` → beat 3 and 0.444. The digits after the second dot are
/// a decimal fraction of the beat.
///
/// # Errors
/// Not `bar.beat[.fraction]`, or a beat below 1.
pub fn parse_at(text: &str) -> Result<BarBeat, String> {
    let bad = || format!("{text:?} is not bar.beat[.fraction] (118.4.25)");
    let mut parts = text.trim().splitn(3, '.');
    let bar: i32 = parts.next().ok_or_else(bad)?.parse().map_err(|_| bad())?;
    let beat: i32 = match parts.next() {
        Some(b) => b.parse().map_err(|_| bad())?,
        None => 1,
    };
    let fraction = match parts.next() {
        Some(f) if !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()) => {
            format!("0.{f}").parse::<f64>().map_err(|_| bad())?
        }
        Some(_) => return Err(bad()),
        None => 0.0,
    };
    if beat < 1 {
        return Err(bad());
    }
    Ok(BarBeat {
        bar,
        beat,
        fraction,
    })
}

/// `bar`, `beat` and `fraction` written as [`parse_at`] reads them, the
/// fraction to a thousandth: `118.4.25`, `150.1`.
#[must_use]
pub fn format_at(bar: i32, beat: i32, fraction: f64) -> String {
    let thousandths = (fraction * 1_000.0).round();
    if thousandths <= 0.0 || thousandths >= 1_000.0 {
        return format!("{bar}.{beat}");
    }
    let digits = format!(
        "{:03}",
        num_traits::ToPrimitive::to_u32(&thousandths).unwrap_or(0)
    );
    format!("{bar}.{beat}.{}", digits.trim_end_matches('0'))
}

/// Where `at` plays, project seconds.
#[must_use]
pub fn time_of(daw: &daw::standalone::Standalone, project: &ProjectContext, at: BarBeat) -> f64 {
    crate::song_bars::time(daw, project, at.bar, at.beat, at.fraction)
}

/// `t` as bar.beat.
#[must_use]
pub fn at_of(daw: &daw::standalone::Standalone, project: &ProjectContext, t: f64) -> String {
    let (bar, beat, fraction) = crate::song_bars::musical(daw, project, t);
    format_at(bar, beat, fraction)
}

// ── export ────────────────────────────────────────────────────────────

static EXPORT: ActionMeta = meta(
    "DRUMS_EDIT_EXPORT",
    "drums_edit_export",
    "Write the drum edit as a change list",
    "Write every hit that plays now as a change list (`studio.drum-edit/1`): its drum (`piece`), its take (`take`, the hit id's code after `~`), where it was recorded (`recorded`, s) and where it plays (`to`, s; `at` as bar.beat), with the song's sections and tempo. Hit ids change with every detection; the list does not, so DRUMS_APPLY `list` replays it after HITS_DETECT. `to`: the file (default: the song's Backups folder, `<song>.drum-edit.<date>.json`). Says where it wrote and how many hits. Read-only for the song.",
    "Drums",
    "",
);

#[derive(Facet)]
struct ExportArg {
    to: Option<String>,
}

#[derive(Facet)]
struct Exported {
    path: String,
    hits: u32,
    moved: u32,
}

/// The current edit as a list.
#[must_use]
pub fn export_list(daw: &daw::standalone::Standalone, project: &ProjectContext) -> EditList {
    let strokes = crate::drum_apply::strokes(daw, project);
    let mut hits: Vec<ListHit> = strokes
        .iter()
        .map(|s| ListHit {
            piece: s.piece.clone(),
            take: take_of(&s.id).to_owned(),
            recorded: round6(s.recorded),
            to: round6(s.now),
            at: at_of(daw, project, s.now),
        })
        .collect();
    hits.sort_by(|a, b| a.to.total_cmp(&b.to));
    let sections = crate::moment::sections(daw, project)
        .into_iter()
        .map(|s| ListSection {
            name: s.name,
            from: s.start,
            to: s.end,
            bars: vec![s.first_bar, s.last_bar],
        })
        .collect();
    let bar_1 = crate::song_bars::time(daw, project, 1, 1, 0.0);
    let guid = match project {
        ProjectContext::Project(g) => g.clone(),
        ProjectContext::Current => String::new(),
    };
    let session = daw
        .read_project(&guid, |p| p.info.path.clone())
        .unwrap_or_default();
    EditList {
        format: FORMAT.to_owned(),
        song: daw.read_project(&guid, |p| p.info.name.clone()).unwrap_or_default(),
        session: Path::new(&session)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        made: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%z").to_string(),
        about: "Every hit that plays in the drum edit: its drum, its take (the hit id's take code), where it was recorded and where it plays (project seconds). Replay: DRUMS_APPLY {list}, after HITS_DETECT.".to_owned(),
        tempo: Some(ListTempo {
            bpm: daw::service::TempoMap::get_tempo_at(daw, project.clone(), bar_1 + 1e-6),
            meter: String::new(),
            bar_1_at: bar_1,
        }),
        sections,
        hits,
    }
}

fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Where a list is written by default: beside the song, in Backups.
fn default_path(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Option<PathBuf> {
    let ProjectContext::Project(guid) = project else {
        return None;
    };
    let path = daw.read_project(guid, |p| p.info.path.clone())?;
    let path = Path::new(&path);
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let date = chrono::Local::now().format("%Y-%m-%d");
    Some(
        path.parent()?
            .join("Backups")
            .join(format!("{stem}.drum-edit.{date}.json")),
    )
}

/// Register `DRUMS_EDIT_EXPORT`.
///
/// # Errors
/// When it is already registered.
pub fn register(list: &ActionList) -> Result<(), architect::action::ActionError> {
    list.register(&EXPORT, Effect::ReadOnly, |a: ExportArg| {
        let span = tracing::info_span!(
            "DRUMS_EDIT_EXPORT",
            hits = tracing::field::Empty,
            bytes = tracing::field::Empty
        );
        let _in = span.enter();
        let (daw, project) = crate::actions::song()?;
        let list = {
            let _s = tracing::info_span!("DRUMS_EDIT_EXPORT.read").entered();
            export_list(daw, &project)
        };
        let path =
            a.to.map(PathBuf::from)
                .or_else(|| default_path(daw, &project))
                .ok_or("say where: `to` (the song has no file to write beside)")?;
        let _s = tracing::info_span!("DRUMS_EDIT_EXPORT.write").entered();
        let text = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, &text).map_err(|e| format!("{}: {e}", path.display()))?;
        span.record("hits", list.hits.len());
        span.record("bytes", text.len());
        Ok(Exported {
            path: path.display().to_string(),
            hits: u32::try_from(list.hits.len()).unwrap_or(u32::MAX),
            moved: u32::try_from(
                list.hits
                    .iter()
                    .filter(|h| (h.to - h.recorded).abs() > 0.000_5)
                    .count(),
            )
            .unwrap_or(u32::MAX),
        })
    })?;
    Ok(())
}
