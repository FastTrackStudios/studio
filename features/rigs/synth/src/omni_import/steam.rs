//! **Soundsource zone maps from a STEAM `.db`.**
//!
//! An Omnisphere soundsource `.db` stores its audio once and *presents* it
//! through one or more `.zmap` files at the root of its manifest — and the
//! `.zmap` name is what a patch's `MS_IM_0 name=` asks for. One `.db` can
//! present several soundsources (`Virus Blue PWM Strings.db` holds `Blue PWM
//! Strings a`, `b` and `c`: three keymaps over one sample set), and the name
//! need not match the file (`Cello Tender Mutes.db` plays the sampled
//! instrument `Studio Cello Tender Mutes -12`). Reading the `.zmap` is how
//! every one of them becomes playable.
//!
//! ```text
//! [d5 50 1f 65] [u32 LE xml_len] [manifest XML ^ 0x8f] [binary region]
//!
//! <Name>.zmap  InstrumentMultisample
//!   MultisampleZone MinPitch MaxPitch MinVelocity MaxVelocity HitKind Volume Pitch
//!     SoundGroupWithNames SampledInstrumentName="<dir>" LibraryName="."
//! <dir>/Pitch N-M/HitBundle.xml   HitBundle BundleKind (= a zone's HitKind) Level
//! <dir>/Pitch N-M/<layer>.xml     LayerHitStack Level      — one per mic/layer
//!   HitVelocity Minimum Maximum Level
//!     SampleWaveform BaseNote AudioFilePath RoundRobinSequenceNum Level A440
//! ```
//!
//! Gains multiply down that chain and the zone's `Pitch` scales the sample's
//! `A440` ratio. Manifest offsets are relative to the binary region; the
//! entries read here are plain (unencrypted) XML.
//!
//! The audio itself is not touched: the zones name the files exactly as the
//! extraction (`sc-import steam`) wrote them — the manifest file name's stem,
//! sanitised, `_2`… on a collision, the `.db` stem when the `.db` holds one
//! sound — so a styx written here sits beside audio extracted long ago.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use super::xml::{XmlNode, omni_num, parse_xml};

const OMNI_MAGIC: [u8; 4] = [0xd5, 0x50, 0x1f, 0x65];
const MANIFEST_XOR: u8 = 0x8f;

/// One `<FILE>` of the manifest.
#[derive(Debug, Clone)]
struct DbFile {
    /// Enclosing directories, `/`-joined (`""` at the root).
    dir: String,
    name: String,
    offset: usize,
    size: usize,
}

impl DbFile {
    fn path(&self) -> String {
        if self.dir.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.dir, self.name)
        }
    }
}

/// An opened Omnisphere soundsource `.db`.
#[derive(Debug)]
pub struct SteamDb {
    data: Vec<u8>,
    base: usize,
    files: Vec<DbFile>,
}

impl SteamDb {
    /// Read and index `path`.
    ///
    /// # Errors
    ///
    /// The file cannot be read or is not an Omnisphere `.db`.
    pub fn open(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_bytes(data)
    }

    /// Index an Omnisphere `.db` already in memory.
    ///
    /// # Errors
    ///
    /// The magic is wrong or the manifest does not parse.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, String> {
        if data.get(..4) != Some(&OMNI_MAGIC[..]) {
            return Err("not an Omnisphere .db (magic)".into());
        }
        let len = data
            .get(4..8)
            .and_then(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or("truncated header")?;
        let len = usize::try_from(len).map_err(|_| "manifest length")?;
        let base = len.checked_add(8).ok_or("manifest length")?;
        let manifest: Vec<u8> = data
            .get(8..base)
            .ok_or("truncated manifest")?
            .iter()
            .map(|b| b ^ MANIFEST_XOR)
            .collect();
        let root = parse_xml(&String::from_utf8_lossy(&manifest))?;
        let mut files = Vec::new();
        collect_files(&root, &mut Vec::new(), &mut files);
        Ok(Self { data, base, files })
    }

    fn bytes(&self, f: &DbFile) -> Option<&[u8]> {
        let start = self.base.checked_add(f.offset)?;
        self.data.get(start..start.checked_add(f.size)?)
    }

    fn xml(&self, f: &DbFile) -> Option<XmlNode> {
        parse_xml(&String::from_utf8_lossy(self.bytes(f)?)).ok()
    }

    fn file(&self, path: &str) -> Option<&DbFile> {
        self.files.iter().find(|f| f.path() == path)
    }

    /// Manifest path → the file name the extraction gave its audio, for
    /// every audio entry (data starting `SpCA` → `.flac`, `RIFF` → `.wav`,
    /// `FORM` → `.aif`), in manifest order with the extractor's collision
    /// suffixes. A `.db` holding one sound names it after `db_stem`.
    fn extracted_names(&self, db_stem: &str) -> HashMap<String, String> {
        let audio: Vec<(&DbFile, &str)> = self
            .files
            .iter()
            .filter_map(|f| {
                let ext = match self.bytes(f)?.get(..4)? {
                    b"SpCA" => "flac",
                    b"RIFF" => "wav",
                    b"FORM" => "aif",
                    _ => return None,
                };
                Some((f, ext))
            })
            .collect();
        if let [(f, ext)] = audio.as_slice() {
            return HashMap::from([(f.path(), format!("{db_stem}.{ext}"))]);
        }
        let mut taken = HashSet::new();
        let mut out = HashMap::new();
        for (f, ext) in audio {
            // The extractor drops manifest tags that are not valid UTF-8.
            if f.name.contains('\u{fffd}') {
                continue;
            }
            // The extractor names files from the manifest's raw attribute
            // text, entities and all (`CHO_Ah &#60;&#62; D_2.flac`).
            let raw = raw_attr(&f.name);
            let stem = Path::new(&raw)
                .file_stem()
                .map_or_else(|| raw.clone(), |s| s.to_string_lossy().into_owned());
            let safe = sanitize(&stem);
            let mut name = format!("{safe}.{ext}");
            let mut n = 2usize;
            while !taken.insert(name.clone()) {
                name = format!("{safe}_{n}.{ext}");
                n = n.saturating_add(1);
            }
            out.insert(f.path(), name);
        }
        out
    }
}

fn collect_files(node: &XmlNode, dirs: &mut Vec<String>, out: &mut Vec<DbFile>) {
    for c in &node.children {
        match c.tag.as_str() {
            "DIR" => {
                dirs.push(c.attr("name").unwrap_or_default().to_string());
                collect_files(c, dirs, out);
                dirs.pop();
            }
            "FILE" => {
                let num = |k| c.attr(k).and_then(|v| v.parse::<usize>().ok());
                if let (Some(name), Some(offset), Some(size)) =
                    (c.attr("name"), num("offset"), num("size"))
                {
                    out.push(DbFile {
                        dir: dirs.join("/"),
                        name: name.to_string(),
                        offset,
                        size,
                    });
                }
            }
            _ => collect_files(c, dirs, out),
        }
    }
}

/// An attribute value as the manifest writes it (numeric entities).
fn raw_attr(decoded: &str) -> String {
    decoded
        .replace('&', "&#38;")
        .replace('<', "&#60;")
        .replace('>', "&#62;")
        .replace('"', "&#34;")
}

/// The extractor's file-name sanitising: anything non-ASCII or reserved → `_`.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() && !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `a/./b/../c` → `a/c`.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

// ── Soundsources ─────────────────────────────────────────────────────────────

/// One playable zone of a soundsource.
#[derive(Debug, Clone, PartialEq)]
pub struct ZmapZone {
    /// The extracted audio file, relative to the `.db`'s extraction dir.
    pub file: String,
    pub key_min: u8,
    pub key_max: u8,
    pub root_key: u8,
    pub vel_min: u8,
    pub vel_max: u8,
    pub rr_index: u32,
    pub gain_db: f32,
    pub tune_cents: f32,
    /// The layer file it came from (`Default Layer`, `Stereo Mics`, …).
    pub layer: String,
}

/// One soundsource a `.db` presents — a `.zmap`.
#[derive(Debug, Clone, Default)]
pub struct ZmapSoundsource {
    /// The name a patch asks for (the `.zmap` stem).
    pub name: String,
    pub zones: Vec<ZmapZone>,
    /// Zones whose sampled instrument, bundle or sample this `.db` lacks
    /// (one in another library) — counted, not played.
    pub unresolved: usize,
}

impl ZmapSoundsource {
    /// The distinct layers its zones come from, in first-seen order.
    #[must_use]
    pub fn layers(&self) -> Vec<&str> {
        let mut seen = Vec::new();
        for z in &self.zones {
            if !seen.contains(&z.layer.as_str()) {
                seen.push(z.layer.as_str());
            }
        }
        seen
    }
}

/// One sample of a bundle, before a zmap zone places it.
#[derive(Debug, Clone)]
struct BundleSample {
    path: String,
    root: u8,
    vel: (u8, u8),
    rr: u32,
    gain: f32,
    ratio: f32,
    layer: String,
}

/// Every soundsource `db` presents, one per root-level `.zmap`. `db_stem`
/// is the `.db` file's stem (it names the audio of a one-sound `.db`).
#[must_use]
pub fn soundsources(db: &SteamDb, db_stem: &str) -> Vec<ZmapSoundsource> {
    let names = db.extracted_names(db_stem);
    let bundles = bundles(db);
    db.files
        .iter()
        .filter(|f| f.dir.is_empty() && f.name.ends_with(".zmap"))
        .filter_map(|f| {
            let name = f.name.strip_suffix(".zmap")?.to_string();
            let root = db.xml(f)?;
            Some(realize(name, &root, &bundles, &names))
        })
        .collect()
}

/// `(sampled instrument, BundleKind)` → that bundle's samples.
fn bundles(db: &SteamDb) -> HashMap<(String, i32), Vec<BundleSample>> {
    let mut out: HashMap<(String, i32), Vec<BundleSample>> = HashMap::new();
    for hb in db.files.iter().filter(|f| f.name == "HitBundle.xml") {
        let Some(node) = db.xml(hb) else { continue };
        let Some(bundle) = node.find("HitBundle") else {
            continue;
        };
        let Some(kind) = bundle
            .attr("BundleKind")
            .and_then(|k| k.parse::<i32>().ok())
        else {
            continue;
        };
        let level = bundle.num("Level").unwrap_or(1.0);
        // The instrument is the directory above `Pitch N-M`.
        let instrument = hb.dir.rsplit_once('/').map_or("", |(i, _)| i).to_string();
        let samples = out.entry((instrument.clone(), kind)).or_default();
        for layer_file in db
            .files
            .iter()
            .filter(|f| f.dir == hb.dir && f.name.ends_with(".xml") && f.name != "HitBundle.xml")
        {
            let Some(stack) = db.xml(layer_file) else {
                continue;
            };
            let layer = layer_file.name.trim_end_matches(".xml").to_string();
            samples.extend(layer_samples(&stack, &instrument, &layer, level));
        }
    }
    out
}

fn layer_samples(
    stack: &XmlNode,
    instrument: &str,
    layer: &str,
    bundle_level: f32,
) -> Vec<BundleSample> {
    let Some(stack) = stack.find("LayerHitStack") else {
        return Vec::new();
    };
    let stack_level = bundle_level * stack.num("Level").unwrap_or(1.0);
    let byte =
        |n: &XmlNode, k: &str, d: u8| n.attr(k).and_then(|v| v.parse::<u8>().ok()).unwrap_or(d);
    let mut out = Vec::new();
    for hv in stack.children_tagged("HitVelocity") {
        let hv_level = stack_level * hv.num("Level").unwrap_or(1.0);
        let vel = (byte(hv, "Minimum", 0), byte(hv, "Maximum", 127));
        for sw in hv.children_tagged("SampleWaveform") {
            let Some(audio) = sw.attr("AudioFilePath") else {
                continue;
            };
            out.push(BundleSample {
                path: normalize(&format!("{instrument}/{audio}")),
                root: byte(sw, "BaseNote", 60),
                vel,
                rr: sw
                    .attr("RoundRobinSequenceNum")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                gain: hv_level * sw.attr("Level").map_or(1.0, omni_num),
                ratio: sw.attr("A440").map_or(1.0, omni_num),
                layer: layer.to_string(),
            });
        }
    }
    out
}

fn realize(
    name: String,
    zmap: &XmlNode,
    bundles: &HashMap<(String, i32), Vec<BundleSample>>,
    names: &HashMap<String, String>,
) -> ZmapSoundsource {
    let mut ss = ZmapSoundsource {
        name,
        ..ZmapSoundsource::default()
    };
    let Some(multi) = zmap.find("InstrumentMultisample") else {
        return ss;
    };
    let byte =
        |n: &XmlNode, k: &str, d: u8| n.attr(k).and_then(|v| v.parse::<u8>().ok()).unwrap_or(d);
    for zone in multi.children_tagged("MultisampleZone") {
        // `LibraryName` is `.` or a library label (`Core Library IL
        // Current`) — either way the instrument is usually in this `.db`;
        // one that is not is counted unresolved.
        let instrument = zone
            .child("SoundGroupWithNames")
            .and_then(|g| g.attr("SampledInstrumentName"))
            .unwrap_or_default();
        let kind = zone.attr("HitKind").and_then(|k| k.parse::<i32>().ok());
        let samples = kind.and_then(|k| bundles.get(&(instrument.to_string(), k)));
        let Some(samples) = samples.filter(|s| !s.is_empty()) else {
            ss.unresolved = ss.unresolved.saturating_add(1);
            continue;
        };
        let keys = (byte(zone, "MinPitch", 0), byte(zone, "MaxPitch", 127));
        let vel = (byte(zone, "MinVelocity", 0), byte(zone, "MaxVelocity", 127));
        let volume = zone.num("Volume").unwrap_or(1.0);
        let pitch = zone.num("Pitch").unwrap_or(1.0);
        for s in samples {
            let (lo, hi) = (vel.0.max(s.vel.0), vel.1.min(s.vel.1));
            if lo > hi {
                continue;
            }
            let Some(file) = names.get(&s.path) else {
                ss.unresolved = ss.unresolved.saturating_add(1);
                continue;
            };
            ss.zones.push(ZmapZone {
                file: file.clone(),
                key_min: keys.0,
                key_max: keys.1,
                root_key: s.root,
                vel_min: lo,
                vel_max: hi,
                rr_index: s.rr,
                gain_db: 20.0 * (volume * s.gain).max(1e-6).log10(),
                // `A440` is the recording's measured tuning, so playback
                // divides by it (Zim'bira Screw Tap, A440 0.981: dividing
                // lands 5 cents from Omnisphere, multiplying 60 cents flat);
                // the zone's `Pitch` scales playback.
                tune_cents: 1200.0 * (pitch / s.ratio.max(1e-6)).max(1e-6).log2(),
                layer: s.layer.clone(),
            });
        }
    }
    ss
}

// ── Styx ─────────────────────────────────────────────────────────────────────

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The soundsource as a zone-mode `library.styx`. `file_prefix` is prepended
/// to every zone's file (`"<db stem>/"` when the styx sits beside, not in,
/// the extraction dir). A soundsource of several layers tags each zone with
/// its layer as the mic, as the Keyscape soundsources do; one layer needs no
/// mic.
#[must_use]
pub fn write_styx(ss: &ZmapSoundsource, vendor: &str, file_prefix: &str) -> String {
    let multi_layer = ss.layers().len() > 1;
    let mut out = String::new();
    let _ = writeln!(out, "name {}", quote(&ss.name));
    let _ = writeln!(out, "version \"1.0\"");
    let _ = writeln!(out, "vendor {}", quote(vendor));
    let _ = writeln!(out, "\nzones (");
    for z in &ss.zones {
        let _ = writeln!(out, "    {{");
        let _ = writeln!(
            out,
            "        file       {}",
            quote(&format!("{file_prefix}{}", z.file))
        );
        let _ = writeln!(out, "        key_min    {}", z.key_min);
        let _ = writeln!(out, "        key_max    {}", z.key_max);
        let _ = writeln!(out, "        root_key   {}", z.root_key);
        let _ = writeln!(out, "        vel_min    {}", z.vel_min);
        let _ = writeln!(out, "        vel_max    {}", z.vel_max);
        let _ = writeln!(out, "        rr_index   {}", z.rr_index);
        let _ = writeln!(out, "        gain_db    {:.3}", z.gain_db);
        let _ = writeln!(out, "        tune_cents {:.3}", z.tune_cents);
        if multi_layer {
            let _ = writeln!(out, "        mic        {}", quote(&z.layer));
        }
        let _ = writeln!(out, "    }}");
    }
    let _ = writeln!(out, ")");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "<?xml version=\"1.0\" standalone=\"no\"?>\n<!DOCTYPE s1 SYSTEM \"sbk:/style/dtd/document.dtd\">\n";

    /// Assemble a `.db` from `(dir, name, bytes)` entries.
    fn db(entries: &[(&str, &str, Vec<u8>)]) -> SteamDb {
        let mut blob = Vec::new();
        let mut manifest = String::from("<FileSystem>");
        let mut open: Vec<&str> = Vec::new();
        for (dir, name, bytes) in entries {
            let want: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
            let keep = open.iter().zip(&want).take_while(|(a, b)| a == b).count();
            while open.len() > keep {
                open.pop();
                manifest.push_str("</DIR>");
            }
            for d in &want[keep..] {
                manifest.push_str(&format!("<DIR name=\"{d}\">"));
                open.push(d);
            }
            manifest.push_str(&format!(
                "<FILE name=\"{name}\" offset=\"{}\" size=\"{}\"/>",
                blob.len(),
                bytes.len()
            ));
            blob.extend_from_slice(bytes);
        }
        for _ in &open {
            manifest.push_str("</DIR>");
        }
        manifest.push_str("</FileSystem>");
        let mut data = OMNI_MAGIC.to_vec();
        data.extend_from_slice(&u32::try_from(manifest.len()).unwrap().to_le_bytes());
        data.extend(manifest.bytes().map(|b| b ^ MANIFEST_XOR));
        data.extend(blob);
        SteamDb::from_bytes(data).unwrap()
    }

    fn zmap(zones: &str) -> Vec<u8> {
        format!(
            "{HEAD}<InstrumentMultisample ATTRIB_VALUE_DATA=\"\">{zones}</InstrumentMultisample>"
        )
        .into_bytes()
    }

    fn zone(lo: u8, hi: u8, kind: u8, instrument: &str) -> String {
        format!(
            "<MultisampleZone MinPitch=\"{lo}\" MaxPitch=\"{hi}\" MinVelocity=\"0\" MaxVelocity=\"127\" HitKind=\"{kind}\" Volume=\"3f800000\" Pitch=\"3f800000\" fixVel=\"-1\"><SoundGroupWithNames UnderKitSession=\"0\" LibraryName=\".\" SampledInstrumentName=\"{instrument}\"></SoundGroupWithNames></MultisampleZone>"
        )
    }

    fn bundle(kind: u8) -> Vec<u8> {
        format!("{HEAD}<HitBundle BundleKind=\"{kind}\" Level=\"3f800000\"></HitBundle>")
            .into_bytes()
    }

    fn layer(root: u8, file: &str, level: &str) -> Vec<u8> {
        format!("{HEAD}<LayerHitStack Level=\"3f800000\"><HitVelocity Level=\"3f800000\" Minimum=\"0\" Maximum=\"127\"><SampleWaveform RoundRobinSequenceNum=\"0\" BaseNote=\"{root}\" AudioFilePath=\"{file}\" Level=\"{level}\" A440=\"3f800000\"></SampleWaveform></HitVelocity></LayerHitStack>").into_bytes()
    }

    fn audio() -> Vec<u8> {
        b"SpCA....".to_vec()
    }

    #[test]
    fn each_zmap_is_a_soundsource_named_for_itself() {
        // One instrument, two keymaps over it — and an instrument dir whose
        // name is not the soundsource's.
        let db = db(&[
            (
                "",
                "Blue a.zmap",
                zmap(&format!(
                    "{}{}",
                    zone(0, 40, 37, "Inst -12"),
                    zone(41, 127, 49, "Inst -12")
                )),
            ),
            ("", "Blue b.zmap", zmap(&zone(0, 127, 49, "Inst -12"))),
            ("Inst -12/AudioFiles", "s 37.wav", audio()),
            ("Inst -12/AudioFiles", "s 49.wav", audio()),
            ("Inst -12/Pitch 37-48", "HitBundle.xml", bundle(37)),
            (
                "Inst -12/Pitch 37-48",
                "Stereo Mics.xml",
                layer(37, "AudioFiles/s 37.wav", "3f800000"),
            ),
            ("Inst -12/Pitch 49-49", "HitBundle.xml", bundle(49)),
            (
                "Inst -12/Pitch 49-49",
                "Stereo Mics.xml",
                layer(49, "AudioFiles/s 49.wav", "40000000"),
            ),
        ]);
        let ss = soundsources(&db, "Virus Blue");
        let names: Vec<_> = ss.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Blue a", "Blue b"]);

        let a = &ss[0];
        assert_eq!(a.unresolved, 0);
        assert_eq!(a.zones.len(), 2);
        // The zmap's key range places the bundle, not the pitch dir's name.
        assert_eq!(
            (a.zones[0].key_min, a.zones[0].key_max, a.zones[0].root_key),
            (0, 40, 37)
        );
        assert_eq!(a.zones[0].file, "s 37.flac");
        // A sample Level of 2.0 is +6 dB.
        assert!((a.zones[1].gain_db - 6.0206).abs() < 1e-3);
        assert_eq!(ss[1].zones.len(), 1);
        assert_eq!(ss[1].zones[0].file, "s 49.flac");
    }

    #[test]
    fn a_library_label_still_finds_the_instrument_here() {
        let labelled = zone(0, 127, 0, "Glock").replace(
            "LibraryName=\".\"",
            "LibraryName=\"Core Library IL Current\"",
        );
        let db = db(&[
            ("", "Glock.zmap", zmap(&labelled)),
            ("Glock/AudioFiles", "g1.wav", audio()),
            ("Glock/AudioFiles", "g2.wav", audio()),
            ("Glock/Pitch 0-127", "HitBundle.xml", bundle(0)),
            (
                "Glock/Pitch 0-127",
                "Default Layer.xml",
                layer(60, "AudioFiles/g1.wav", "3f800000"),
            ),
        ]);
        let ss = soundsources(&db, "Glock");
        assert_eq!((ss[0].zones.len(), ss[0].unresolved), (1, 0));
    }

    #[test]
    fn a_flat_recording_is_tuned_up() {
        // A440 0.981 (a recording ~33 cents flat) plays ~33 cents up.
        let flat = String::from_utf8(layer(60, "AudioFiles/f.wav", "3f800000"))
            .unwrap()
            .replace("A440=\"3f800000\"", "A440=\"3f7b295f\"");
        let db = db(&[
            ("", "F.zmap", zmap(&zone(0, 127, 60, "F"))),
            ("F/AudioFiles", "f.wav", audio()),
            ("F/AudioFiles", "g.wav", audio()),
            ("F/Pitch 60-60", "HitBundle.xml", bundle(60)),
            ("F/Pitch 60-60", "Default Layer.xml", flat.into_bytes()),
        ]);
        let cents = soundsources(&db, "F")[0].zones[0].tune_cents;
        assert!((cents - 33.0).abs() < 0.5, "{cents}");
    }

    #[test]
    fn a_one_sound_db_names_its_audio_after_the_db() {
        let db = db(&[
            ("", "Greek Celli.zmap", zmap(&zone(0, 127, 0, "Greek"))),
            ("Greek/AudioFiles", "gc.wav", audio()),
            ("Greek/Pitch 0-127", "HitBundle.xml", bundle(0)),
            (
                "Greek/Pitch 0-127",
                "Default Layer.xml",
                layer(60, "AudioFiles/gc.wav", "3f800000"),
            ),
        ]);
        let ss = soundsources(&db, "Greek Celli");
        assert_eq!(ss[0].zones[0].file, "Greek Celli.flac");
        assert!(!write_styx(&ss[0], "v", "").contains("mic"));
    }

    #[test]
    fn layers_become_mics_only_when_there_are_several() {
        let db = db(&[
            ("", "Guitaret.zmap", zmap(&zone(0, 127, 60, "G"))),
            ("G/AudioFiles", "a.wav", audio()),
            ("G/AudioFiles", "b.wav", audio()),
            ("G/Pitch 60-60", "HitBundle.xml", bundle(60)),
            (
                "G/Pitch 60-60",
                "Direct Pickup.xml",
                layer(60, "AudioFiles/a.wav", "3f800000"),
            ),
            (
                "G/Pitch 60-60",
                "149.xml",
                layer(60, "AudioFiles/b.wav", "3f800000"),
            ),
        ]);
        let ss = soundsources(&db, "Guitaret");
        assert_eq!(ss[0].layers(), ["Direct Pickup", "149"]);
        let styx = write_styx(&ss[0], "v", "");
        assert!(styx.contains("mic        \"Direct Pickup\""));
        assert!(styx.contains("mic        \"149\""));
    }

    #[test]
    fn a_zone_from_another_library_is_counted_not_played() {
        let other =
            zone(0, 127, 0, "Elsewhere").replace("LibraryName=\".\"", "LibraryName=\"Keyscape\"");
        let db = db(&[("", "X.zmap", zmap(&other))]);
        let ss = soundsources(&db, "X");
        assert_eq!((ss[0].zones.len(), ss[0].unresolved), (0, 1));
    }
}
