//! `ThemeHost`: last good, every error, drafts, save, revert, capture.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use frame::theme::host::{ErrorKind, Reload, ThemeHost};
use frame::theme::runtime::{Schema, Value};

const TOKENS: &str = r##"<Document>
  <Variables modes="dark light">
    <Var name="ground" dark="#202020" light="#e8e8e8" />
    <Var name="mute.on" value="#d04040" />
  </Variables>
</Document>
"##;

const STRIP: &str = r##"<Document>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=20 bind:fill="$ground">
      <Frame name="@mute" x=0 y=0 w=20 h=20 bind:fill="track.mute ? $mute.on : $ground" />
    </Frame>
  </Page>
</Document>
"##;

/// The strip with two problems the compile finds: a token and a field
/// the host does not have.
const BROKEN: &str = r##"<Document>
  <Page name="mcp.strip">
    <Frame name="Strip" w=40 h=20 bind:fill="$nope">
      <Frame name="@mute" x=0 y=0 w=20 h=20 bind:fill="track.nope ? $mute.on : $ground" />
    </Frame>
  </Page>
</Document>
"##;

fn schema() -> Schema {
    Schema::new().field("track.name").live("track.mute")
}

/// A fresh folder with the two layers in it, by name: `a-tokens.frame`
/// under `b-strip.frame`.
fn theme_dir() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "frame-theme-host-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a-tokens.frame"), TOKENS).unwrap();
    std::fs::write(dir.join("b-strip.frame"), STRIP).unwrap();
    dir
}

/// Rewrite `file` so its mtime moves even on a coarse clock.
fn rewrite(file: &Path, text: &str) {
    let before = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    loop {
        std::fs::write(file, text).unwrap();
        if std::fs::metadata(file).and_then(|m| m.modified()).ok() != before {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn loaded(dir: &Path) -> ThemeHost {
    let host = ThemeHost::new(schema());
    host.load(vec![dir.to_path_buf()], None).expect("compiles");
    host
}

#[test]
fn a_bad_theme_keeps_the_last_good_one() {
    let dir = theme_dir();
    let host = loaded(&dir);
    let good = host.current().expect("a theme");
    let version = host.version();

    // A file changed on disk to something broken: polled, kept, told once.
    rewrite(&dir.join("b-strip.frame"), BROKEN);
    let Some(Reload::Kept(errors)) = host.poll() else {
        panic!("a broken reload is told");
    };
    assert!(!errors.is_empty());
    assert!(Arc::ptr_eq(&good, &host.current().unwrap()));
    assert_eq!(host.version(), version);
    assert_eq!(host.poll(), None, "nothing changed since");

    // An edit that does not compile: refused, nothing changes.
    let strip = dir.join("b-strip.frame");
    assert!(host.edit(&strip, "<Document><Page".into()).is_err());
    assert!(Arc::ptr_eq(&good, &host.current().unwrap()));
    assert!(host.inspect().files.iter().all(|f| !f.draft));

    // Fixed on disk: the next poll takes it.
    rewrite(&strip, &STRIP.replace("w=20 h=20", "w=10 h=20"));
    assert_eq!(host.poll(), Some(Reload::Applied));
    assert!(host.version() > version);
    assert!(host.errors().is_empty());

    // Another theme that does not load leaves this one on screen too.
    let now = host.current().unwrap();
    assert!(host.load(vec![dir.join("missing.frame")], None).is_err());
    assert!(Arc::ptr_eq(&now, &host.current().unwrap()));
}

#[test]
fn every_error_is_listed() {
    let dir = theme_dir();
    let host = ThemeHost::new(schema());

    // Two files that do not parse: both, each with its file and line.
    std::fs::write(dir.join("a-tokens.frame"), "<Document>\n  <Variables").unwrap();
    std::fs::write(dir.join("b-strip.frame"), "<Document>\n\n  <Page name=").unwrap();
    let errors = host.load(vec![dir.clone()], None).unwrap_err();
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert!(
        errors
            .iter()
            .all(|e| e.kind == ErrorKind::Parse && e.line.is_some())
    );
    assert!(
        errors[0]
            .file
            .as_deref()
            .unwrap()
            .ends_with("a-tokens.frame")
    );
    assert!(
        errors[1]
            .file
            .as_deref()
            .unwrap()
            .ends_with("b-strip.frame")
    );
    assert_eq!(host.errors(), errors);
    assert!(host.current().is_none());

    // Two compile problems in one file: both, each with its role and node.
    std::fs::write(dir.join("a-tokens.frame"), TOKENS).unwrap();
    std::fs::write(dir.join("b-strip.frame"), BROKEN).unwrap();
    let errors = host.reload().unwrap_err();
    assert!(errors.len() >= 2, "{errors:#?}");
    assert!(errors.iter().all(|e| e.kind == ErrorKind::Compile));
    let says = |what: &str, node: &str| {
        errors.iter().any(|e| {
            e.message.contains(what)
                && e.role.as_deref() == Some("mcp.strip")
                && e.node.as_deref() == Some(node)
        })
    };
    assert!(says("nope", "Strip"), "{errors:#?}");
    assert!(says("track.nope", "@mute"), "{errors:#?}");
    assert_eq!(host.inspect().errors, errors);

    // Fixed: no errors left.
    std::fs::write(dir.join("b-strip.frame"), STRIP).unwrap();
    host.reload().expect("compiles");
    assert!(host.errors().is_empty());
}

#[test]
fn revert_restores_the_files_byte_for_byte() {
    let dir = theme_dir();
    let host = loaded(&dir);
    let files = |dir: &Path| {
        ["a-tokens.frame", "b-strip.frame"].map(|f| std::fs::read(dir.join(f)).unwrap())
    };
    let original = files(&dir);
    let before = host.inspect();

    // An edit is a draft: the theme changes, the file does not.
    let strip = dir.join("b-strip.frame");
    let edited = STRIP.replace("$ground\">", "$mute.on\">");
    host.edit(&strip, edited.clone()).expect("compiles");
    assert_eq!(files(&dir), original);
    assert!(host.inspect().files.iter().any(|f| f.draft));

    // Saved: the file is the edit.
    let written = host.save().expect("saved");
    assert_eq!(written.len(), 1);
    assert_eq!(std::fs::read_to_string(&strip).unwrap(), edited);
    assert_eq!(host.poll(), None, "a save is not a change to reload");

    // Changed on disk by someone else too.
    rewrite(
        &dir.join("a-tokens.frame"),
        &TOKENS.replace("#202020", "#303030"),
    );
    assert_eq!(host.poll(), Some(Reload::Applied));

    // Reverted: every file as it was loaded, and the theme it was.
    let written = host.revert().expect("reverted");
    assert_eq!(written.len(), 2);
    assert_eq!(files(&dir), original);
    let after = host.inspect();
    assert_eq!(after.files, before.files);
    assert_eq!(after.tokens, before.tokens);
    assert!(after.version > before.version);
    assert!(host.revert().unwrap().is_empty(), "nothing left to revert");
}

#[test]
fn save_preserves_external_changes_and_drafts() {
    let dir = theme_dir();
    let host = loaded(&dir);
    let strip = dir.join("b-strip.frame");
    let edited = STRIP.replace("w=20 h=20", "w=10 h=20");
    host.edit(&strip, edited).unwrap();
    let external = STRIP.replace("w=20 h=20", "w=15 h=20");
    rewrite(&strip, &external);

    let errors = host.save().unwrap_err();
    assert!(errors[0].message.contains("changed since"));
    assert_eq!(std::fs::read_to_string(&strip).unwrap(), external);
    assert!(host.inspect().files.iter().any(|f| f.draft));
    assert_eq!(host.poll(), Some(Reload::Applied));
}

#[test]
fn saving_one_file_does_not_hide_another_files_change() {
    let dir = theme_dir();
    let host = loaded(&dir);
    host.edit(
        &dir.join("b-strip.frame"),
        STRIP.replace("w=20 h=20", "w=10 h=20"),
    )
    .unwrap();
    rewrite(
        &dir.join("a-tokens.frame"),
        &TOKENS.replace("#202020", "#303030"),
    );
    host.save().unwrap();
    assert_eq!(host.poll(), Some(Reload::Applied));
}

#[test]
fn capture_draws_the_current_theme_on_the_cpu() {
    let dir = theme_dir();
    let host = loaded(&dir);
    let muted = host.fields(&[("track.mute", Value::Bool(true))]);
    let shot = host.capture("mcp.strip", None, 2.0, &muted).expect("drawn");
    assert_eq!((shot.pixels.width, shot.pixels.height), (80, 40));
    // The mute in `$mute.on`, the rest in `$ground`.
    assert_eq!(shot.pixels.at(10, 20), [0xd0, 0x40, 0x40, 0xff]);
    assert_eq!(shot.pixels.at(60, 20), [0x20, 0x20, 0x20, 0xff]);
    let png = shot.pixels.png().expect("encoded");
    assert_eq!(&png[1..4], b"PNG");

    let err = host.capture("tcp.row", None, 1.0, &muted).unwrap_err();
    assert!(err.message.contains("mcp.strip"), "{err}");
}

#[test]
fn drawing_never_waits_for_a_compile_on_another_thread() {
    let dir = theme_dir();
    let host = Arc::new(loaded(&dir));
    let strip = dir.join("b-strip.frame");
    let worker = {
        let host = host.clone();
        std::thread::spawn(move || {
            let mut applied = 0;
            for i in 0..20 {
                rewrite(&strip, &STRIP.replace("w=20", &format!("w={}", 10 + i)));
                if host.poll() == Some(Reload::Applied) {
                    applied += 1;
                }
            }
            applied
        })
    };
    while !worker.is_finished() {
        assert!(host.current().is_some_and(|t| t.has_role("mcp.strip")));
    }
    assert_eq!(worker.join().unwrap(), 20);
}

#[test]
fn a_failed_load_keeps_watching_the_good_theme_and_a_broken_start_is_watched() {
    let dir = theme_dir();
    let host = loaded(&dir);
    assert!(host.load(vec![dir.join("missing.frame")], None).is_err());
    assert_eq!(host.inspect().paths, vec![dir.display().to_string()]);
    host.reload().expect("still the good files");

    // Started broken: no theme, and fixing the file is picked up.
    let strip = dir.join("b-strip.frame");
    rewrite(&strip, BROKEN);
    let host = ThemeHost::new(schema());
    assert!(host.load(vec![dir.clone()], None).is_err());
    assert!(host.current().is_none());
    rewrite(&strip, STRIP);
    assert_eq!(host.poll(), Some(Reload::Applied));
    assert!(host.current().is_some());
}
