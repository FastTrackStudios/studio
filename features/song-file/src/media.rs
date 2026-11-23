//! A song folder's media, in the layout the owner set:
//!
//! ```text
//! Songs/<Title - Artist>/
//!   <Title>.session
//!   Media/                 originals (audio and video)
//!     Reference/x.wav      raw WAV references
//!     Proxy/               generated Ogg Vorbis proxies, mirroring Media/:
//!       Reference/x.ogg    the proxy of Media/Reference/x.wav
//!       <rel>.ogg          the proxy of Media/<rel>.<ext>
//! ```
//!
//! Before it, a proxy sat in a `Proxies/` folder beside its original
//! (`Media/Proxies/Bass.ogg` for `Media/Bass.wav`), and the reference's at
//! `Media/Proxies/Reference.ogg`. Readers take both until a folder is
//! converted ([`proxy_candidates`]); converting copies each old proxy to
//! its new place ([`copy_into_layout`]) — copied, never moved, and never
//! over a file that is there.

use std::path::{Path, PathBuf};

use crate::Result;

/// The song's media folder, under its folder.
pub const MEDIA: &str = "Media";
/// The proxies' folder, under `Media/`.
pub const PROXY: &str = "Proxy";
/// The references' folder, under `Media/`.
pub const REFERENCE: &str = "Reference";
/// The old proxies' folder, beside what they stand in for.
pub const OLD_PROXIES: &str = "Proxies";

/// Where the proxy of `Media/<rel>` goes: `Media/Proxy/<rel less its
/// extension>.ogg` (`rel` relative to `Media/`).
#[must_use]
pub fn proxy_of(rel: &Path) -> PathBuf {
    Path::new(MEDIA).join(PROXY).join(rel.with_extension("ogg"))
}

/// Where a song folder's reference proxy may be, newest layout first:
/// `Media/Proxy/Reference/<x>.ogg` for each `Media/Reference/<x>.wav`,
/// then the old `Media/Proxies/Reference.ogg`. Relative to the folder.
#[must_use]
pub fn reference_candidates(folder: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = wavs(&folder.join(MEDIA).join(REFERENCE))
        .into_iter()
        .map(|stem| proxy_of(&Path::new(REFERENCE).join(format!("{stem}.wav"))))
        .collect();
    out.push(Path::new(MEDIA).join(OLD_PROXIES).join("Reference.ogg"));
    out
}

/// Where the proxy of `Media/<rel>` may be, newest layout first: the new
/// `Media/Proxy/<rel>.ogg`, then the old `Proxies/<stem>.ogg` beside it.
#[must_use]
pub fn proxy_candidates(rel: &Path) -> [PathBuf; 2] {
    let stem = rel
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = rel.parent().unwrap_or_else(|| Path::new(""));
    [
        proxy_of(rel),
        Path::new(MEDIA)
            .join(dir)
            .join(OLD_PROXIES)
            .join(format!("{stem}.ogg")),
    ]
}

/// The stems of the WAVs in `dir`, sorted.
fn wavs(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")))
        .filter_map(|p| Some(p.file_stem()?.to_string_lossy().into_owned()))
        .collect();
    out.sort();
    out
}

/// A proxy's file (its audio, or its page index `.ogg.idx`), and the rest
/// of its name after its stem.
fn proxy_name(name: &str) -> Option<(&str, &str)> {
    for suffix in [".ogg.idx", ".ogg"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return Some((stem, suffix));
        }
    }
    None
}

/// Every old proxy in `folder`'s media and where the new layout puts it:
/// (from, to), absolute. Only those whose new place is free.
///
/// # Errors
///
/// A folder would not list.
pub fn plan(folder: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let media = folder.join(MEDIA);
    let mut out = Vec::new();
    if !media.is_dir() {
        return Ok(out);
    }
    // The reference's name in the new layout: its WAV's, when there is
    // one in `Media/Reference/`, else `Reference`.
    let references = wavs(&media.join(REFERENCE));
    let reference = match references.as_slice() {
        [one] => one.clone(),
        _ => REFERENCE.to_owned(),
    };
    let mut stack = vec![media.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                // The new layout's own folder is not read from.
                if path != media.join(PROXY) {
                    stack.push(path);
                }
                continue;
            }
            if dir.file_name().is_none_or(|n| n != OLD_PROXIES) {
                continue;
            }
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let Some((stem, suffix)) = proxy_name(&name) else {
                continue;
            };
            let Some(owner) = dir.parent() else { continue };
            let Ok(rel_dir) = owner.strip_prefix(&media) else {
                continue;
            };
            let to = if rel_dir.as_os_str().is_empty()
                && let Some(rest) = stem.strip_prefix(REFERENCE)
            {
                // `Reference.ogg`, `Reference Preview.ogg`: the reference's.
                media
                    .join(PROXY)
                    .join(REFERENCE)
                    .join(format!("{reference}{rest}{suffix}"))
            } else {
                media
                    .join(PROXY)
                    .join(rel_dir)
                    .join(format!("{stem}{suffix}"))
            };
            if !to.exists() {
                out.push((path, to));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Copy every old proxy in `folder` to its place in the new layout (never
/// moving one, never over a file that is there). What was copied.
///
/// # Errors
///
/// A folder would not list, or a copy failed.
pub fn copy_into_layout(folder: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let planned = plan(folder)?;
    for (from, to) in &planned {
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // `create_new`: a file that appeared meanwhile is not written over.
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)?;
        std::io::copy(&mut std::fs::File::open(from)?, &mut out)?;
    }
    Ok(planned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"OggS").unwrap();
    }

    #[test]
    fn old_proxies_are_copied_into_the_mirror() {
        let dir = tempfile::tempdir().unwrap();
        let song = dir.path();
        touch(&song.join("Media/Reference/Drive To LA.wav"));
        touch(&song.join("Media/Proxies/Reference.ogg"));
        touch(&song.join("Media/Proxies/Reference.ogg.idx"));
        touch(&song.join("Media/Proxies/Reference Preview.ogg"));
        touch(&song.join("Media/Proxies/Bass.ogg"));
        touch(&song.join("Media/Drums/Proxies/Kick.ogg"));
        touch(&song.join("Media/Proxies/Peaks/Bass.sessionpeaks"));
        let copied = copy_into_layout(song).unwrap();
        let to: Vec<_> = copied
            .iter()
            .map(|(_, t)| t.strip_prefix(song).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            to,
            [
                "Media/Proxy/Drums/Kick.ogg",
                "Media/Proxy/Bass.ogg",
                "Media/Proxy/Reference/Drive To LA Preview.ogg",
                "Media/Proxy/Reference/Drive To LA.ogg",
                "Media/Proxy/Reference/Drive To LA.ogg.idx",
            ]
            .map(PathBuf::from)
        );
        // Copied, not moved; and a second run copies nothing.
        assert!(song.join("Media/Proxies/Reference.ogg").is_file());
        assert!(copy_into_layout(song).unwrap().is_empty());
        assert_eq!(
            reference_candidates(song),
            [
                PathBuf::from("Media/Proxy/Reference/Drive To LA.ogg"),
                PathBuf::from("Media/Proxies/Reference.ogg"),
            ]
        );
        assert_eq!(
            proxy_candidates(Path::new("Drums/Kick.wav")),
            [
                PathBuf::from("Media/Proxy/Drums/Kick.ogg"),
                PathBuf::from("Media/Drums/Proxies/Kick.ogg"),
            ]
        );
    }
}
