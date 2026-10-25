//! `patch_coverage` — how much of the Omnisphere library Signal can play.
//!
//! Walks every `.prt_omn` under the patch root, parses it, and asks of each
//! enabled layer the questions that decide whether the patch sounds like
//! itself: does its soundsource resolve against the local extraction, do its
//! wavetables, and does it lean on a module that is still a pass-through
//! (sample-mode FM / Ring Mod, Granular)? Each FX slot is tiered as
//! *calibrated* (the unit's own settings drive native DSP), *typed* (a native
//! block of the right kind at its defaults) or *pass-through*.
//!
//! ```text
//! cargo run -p synth --release --example patch_coverage -- [patch_root] [--top 40] [--tsv out.tsv]
//! ```
//!
//! The patch root defaults to the index's (`FTS_OMNISPHERE_PATCHES`, re-based
//! by `FTS_SAMPLED_ROOT`); the soundsources resolve through
//! `SoundsourceIndex::scan_default`, so set `FTS_OMNISPHERE_ROOT` /
//! `FTS_SAMPLED_ROOT` to point it at a drive mounted elsewhere.

use std::collections::{BTreeMap, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};

use synth::omni_import::effects::{effect_block, effect_chain};
use synth::omni_import::{OmniPatch, SoundsourceIndex, parse_patch, parse_xml, wavetable_path};

/// Why a patch would not sound like itself; a patch is *complete* with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Gap {
    MissingSoundsource,
    MissingWavetable,
    SampleFm,
    SampleRing,
    Granular,
    PassThroughFx,
}

impl Gap {
    const ALL: [Self; 6] = [
        Self::MissingSoundsource,
        Self::MissingWavetable,
        Self::SampleFm,
        Self::SampleRing,
        Self::Granular,
        Self::PassThroughFx,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::MissingSoundsource => "soundsource not extracted",
            Self::MissingWavetable => "wavetable not extracted",
            Self::SampleFm => "sample-mode FM (inert)",
            Self::SampleRing => "sample-mode Ring Mod (inert)",
            Self::Granular => "Granular (inert)",
            Self::PassThroughFx => "pass-through FX unit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FxTier {
    Calibrated,
    Typed,
    PassThrough,
}

fn fx_tier(name: &str, p: &[f32; 15]) -> Option<FxTier> {
    if name.is_empty() || name == "No Effect" {
        return None;
    }
    if effect_chain(name, p).is_some() {
        return Some(FxTier::Calibrated);
    }
    let block = effect_block(name, p)?;
    Some(
        if block.block_type == signal_proto::block::BlockType::Custom {
            FxTier::PassThrough
        } else if block.params.is_empty() {
            FxTier::Typed
        } else {
            FxTier::Calibrated
        },
    )
}

/// Per-voice `grnOn` (the parsed model does not carry Granular yet).
fn granular_voices(xml: &str) -> Vec<bool> {
    let Ok(root) = parse_xml(xml) else {
        return Vec::new();
    };
    let Some(engine) = root.find("SYNTHENG") else {
        return Vec::new();
    };
    engine
        .children_tagged("VOICE")
        .map(|v| {
            v.child("OSC")
                .and_then(|o| o.num("grnOn"))
                .is_some_and(|g| g > 0.0)
        })
        .collect()
}

#[derive(Default)]
struct Tally {
    patches: usize,
    parse_failures: Vec<(PathBuf, String)>,
    complete: usize,
    sounding: usize,
    gap_patches: BTreeMap<Gap, usize>,
    missing_ss: HashMap<String, (String, usize)>,
    missing_wt: HashMap<String, usize>,
    fx: HashMap<String, (FxTier, usize)>,
    by_library: BTreeMap<String, (usize, usize)>,
}

struct Row {
    path: PathBuf,
    gaps: Vec<Gap>,
}

fn audit(
    patch: &OmniPatch,
    granular: &[bool],
    index: &SoundsourceIndex,
    t: &mut Tally,
) -> (Vec<Gap>, bool) {
    let mut gaps = Vec::new();
    let mut any_sound = false;
    for (i, layer) in patch.layers.iter().take(4).enumerate() {
        if !layer.enabled {
            continue;
        }
        if layer.soundsource.is_empty() {
            any_sound = true;
            if let Some((a, b)) = &layer.waves {
                for w in [a, b] {
                    if wavetable_path(w).is_none() {
                        gaps.push(Gap::MissingWavetable);
                        *t.missing_wt.entry(w.clone()).or_default() += 1;
                    }
                }
            }
        } else if index.find_source(&layer.soundsource).is_some() {
            any_sound = true;
            if layer.fm_depth > 0.0 {
                gaps.push(Gap::SampleFm);
            }
            if layer.ring_mix > 0.0 {
                gaps.push(Gap::SampleRing);
            }
        } else {
            gaps.push(Gap::MissingSoundsource);
            let e = t
                .missing_ss
                .entry(layer.soundsource.clone())
                .or_insert_with(|| (layer.ss_library.clone(), 0));
            e.1 += 1;
        }
        if granular.get(i).copied().unwrap_or(false) {
            gaps.push(Gap::Granular);
        }
        for (name, p) in layer.fx.iter().zip(&layer.fx_params) {
            note_fx(name, p, &mut gaps, t);
        }
    }
    for (name, p) in patch
        .common_fx
        .iter()
        .zip(&patch.common_fx_params)
        .chain(patch.aux_fx.iter().zip(&patch.aux_fx_params))
    {
        note_fx(name, p, &mut gaps, t);
    }
    gaps.sort();
    gaps.dedup();
    (gaps, any_sound)
}

fn note_fx(name: &str, p: &[f32; 15], gaps: &mut Vec<Gap>, t: &mut Tally) {
    if let Some(tier) = fx_tier(name, p) {
        t.fx.entry(name.to_string()).or_insert((tier, 0)).1 += 1;
        if tier == FxTier::PassThrough {
            gaps.push(Gap::PassThroughFx);
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "prt_omn") {
            out.push(p);
        }
    }
}

fn default_patch_root() -> PathBuf {
    if let Ok(v) = std::env::var("FTS_OMNISPHERE_PATCHES") {
        return v.into();
    }
    let base = std::env::var("FTS_SAMPLED_ROOT")
        .unwrap_or_else(|_| "/run/media/AudioHaven/Sampled".into());
    Path::new(&base).join("Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root = None;
    let mut top = 40usize;
    let mut tsv = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--top" => top = it.next().ok_or("--top N")?.parse()?,
            "--tsv" => tsv = Some(PathBuf::from(it.next().ok_or("--tsv FILE")?)),
            other => root = Some(PathBuf::from(other)),
        }
    }
    let root = root.unwrap_or_else(default_patch_root);

    let index = SoundsourceIndex::scan_default();
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();

    let mut t = Tally::default();
    let mut rows = Vec::new();
    for path in &files {
        t.patches += 1;
        let xml = match std::fs::read(path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(e) => {
                t.parse_failures.push((path.clone(), e.to_string()));
                continue;
            }
        };
        let patch = match parse_patch(&xml) {
            Ok(p) => p,
            Err(e) => {
                t.parse_failures.push((path.clone(), e));
                continue;
            }
        };
        let (gaps, sounding) = audit(&patch, &granular_voices(&xml), &index, &mut t);
        let lib = path
            .strip_prefix(&root)
            .ok()
            .and_then(|r| {
                r.iter()
                    .take(2)
                    .map(|c| c.to_string_lossy())
                    .reduce(|a, b| format!("{a}/{b}").into())
            })
            .map_or_else(String::new, |c| c.into_owned());
        let e = t.by_library.entry(lib).or_default();
        e.0 += 1;
        if gaps.is_empty() {
            t.complete += 1;
            e.1 += 1;
        }
        if sounding {
            t.sounding += 1;
        }
        for g in &gaps {
            *t.gap_patches.entry(*g).or_default() += 1;
        }
        rows.push(Row {
            path: path.clone(),
            gaps,
        });
    }

    report(&t, &index, &root, top);
    if let Some(out) = tsv {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&out)?);
        writeln!(f, "patch\tgaps")?;
        for r in &rows {
            let g: Vec<_> = r.gaps.iter().map(|g| format!("{g:?}")).collect();
            writeln!(f, "{}\t{}", r.path.display(), g.join(","))?;
        }
    }
    Ok(())
}

fn pct(n: usize, of: usize) -> f64 {
    if of == 0 {
        0.0
    } else {
        100.0 * n as f64 / of as f64
    }
}

fn report(t: &Tally, index: &SoundsourceIndex, root: &Path, top: usize) {
    let parsed = t.patches - t.parse_failures.len();
    println!("patch root   {}", root.display());
    println!("soundsources {} indexed", index.len());
    println!(
        "patches      {}  parsed {}  failures {}",
        t.patches,
        parsed,
        t.parse_failures.len()
    );
    println!(
        "sounding     {} ({:.1}%) — at least one layer realizes",
        t.sounding,
        pct(t.sounding, parsed)
    );
    println!(
        "complete     {} ({:.1}%) — no gap below",
        t.complete,
        pct(t.complete, parsed)
    );
    println!("\npatches hit by each gap:");
    for g in Gap::ALL {
        let n = t.gap_patches.get(&g).copied().unwrap_or(0);
        println!("  {:>6} {:>5.1}%  {}", n, pct(n, parsed), g.label());
    }

    println!("\nby library (complete / patches):");
    for (lib, (n, ok)) in &t.by_library {
        println!("  {ok:>6} / {n:<6} {:>5.1}%  {lib}", pct(*ok, *n));
    }

    let mut ss: Vec<_> = t.missing_ss.iter().collect();
    ss.sort_by(|a, b| b.1.1.cmp(&a.1.1).then(a.0.cmp(b.0)));
    println!(
        "\nmissing soundsources: {} distinct (top {top} by layers):",
        ss.len()
    );
    for (name, (lib, n)) in ss.iter().take(top) {
        println!("  {n:>6}  {name}  [{lib}]");
    }

    let mut wt: Vec<_> = t.missing_wt.iter().collect();
    wt.sort_by(|a, b| b.1.cmp(a.1));
    println!("\nmissing wavetables: {} distinct (top 10):", wt.len());
    for (name, n) in wt.iter().take(10) {
        println!("  {n:>6}  {name}");
    }

    let mut fx: Vec<_> = t.fx.iter().collect();
    fx.sort_by(|a, b| b.1.1.cmp(&a.1.1));
    for (tier, label) in [
        (FxTier::PassThrough, "pass-through"),
        (FxTier::Typed, "typed (native defaults)"),
        (FxTier::Calibrated, "calibrated"),
    ] {
        let units: Vec<_> = fx.iter().filter(|(_, (k, _))| *k == tier).collect();
        let slots: usize = units.iter().map(|(_, (_, n))| n).sum();
        println!(
            "\nFX {label}: {} units, {slots} slots (top {top}):",
            units.len()
        );
        for (name, (_, n)) in units.iter().take(top) {
            println!("  {n:>6}  {name}");
        }
    }

    if !t.parse_failures.is_empty() {
        println!("\nparse failures (first 10):");
        for (p, e) in t.parse_failures.iter().take(10) {
            println!("  {}: {e}", p.display());
        }
    }
}
