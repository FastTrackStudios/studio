//! The example themes (`themes/workbench`, `themes/control-room`,
//! `themes/graphite`), each layered over `themes/console` the way
//! `FTS_THEME` loads them: they compile against the host's schema, draw
//! every role the console does, keep every control the console lets a
//! click reach, and a click lands on the control drawn there.
//!
//! The files are read at run time, so editing a theme and re-running this
//! needs no rebuild.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use daw_proto::Track;
use vello::kurbo::Point;

use super::{RACKBACK, ROW, RowShown, STEP, STRIP, Shown};
use crate::arrangement::Palette;

const EXAMPLES: [&str; 3] = ["workbench", "control-room", "graphite"];

fn themes() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("themes")
}

fn layered(name: &str) -> frame::theme::runtime::Theme {
    let paths = [themes().join("console"), themes().join(name)];
    super::compile(&super::stamp(&paths), None)
        .unwrap_or_else(|why| panic!("{name} does not compile: {why}"))
}

fn console() -> frame::theme::runtime::Theme {
    super::compile(&super::stamp(&[themes().join("console")]), None)
        .unwrap_or_else(|why| panic!("console does not compile: {why}"))
}

fn palette() -> Palette {
    Palette::from_theme(&daw_ui::theming::Theme::dark())
}

fn track() -> Track {
    Track {
        armed: true,
        fx_count: 2,
        send_count: 1,
        ..Track::new("{guid}".into(), 0, "Lead Vocal".into())
    }
}

fn strip_fields(mixing: bool) -> super::StripFields {
    super::fields(
        &palette(),
        &track(),
        Shown {
            width: 86.0,
            mixer_h: 520.0,
            mixing,
            recording: !mixing,
            ..Shown::default()
        },
    )
}

fn row_fields(height: f64) -> super::StripFields {
    super::row_fields(
        &palette(),
        &track(),
        RowShown {
            height,
            ..RowShown::default()
        },
    )
}

fn names(
    theme: &frame::theme::runtime::Theme,
    role: &str,
    size: (f64, f64),
    fields: &super::StripFields,
) -> BTreeSet<String> {
    theme.targets(role, size, fields).into_iter().collect()
}

#[test]
fn every_example_compiles_over_console_and_draws_its_roles() {
    for name in EXAMPLES {
        let theme = layered(name);
        for role in [STRIP, RACKBACK, STEP, ROW, "rails", "rail.button"] {
            assert!(theme.has_role(role), "{name} has no {role}");
        }
    }
}

#[test]
fn every_example_keeps_the_consoles_controls() {
    let base = console();
    for name in EXAMPLES {
        let theme = layered(name);
        let strip = strip_fields(false);
        assert_eq!(
            names(&theme, STRIP, (86.0, 520.0), &strip),
            names(&base, STRIP, (86.0, 520.0), &strip),
            "{name}: strip controls",
        );
        let row = row_fields(70.0);
        assert_eq!(
            names(&theme, ROW, (343.0, 70.0), &row),
            names(&base, ROW, (343.0, 70.0), &row),
            "{name}: row controls",
        );
    }
}

/// A click in the middle of a control lands on that control, in the
/// sizes the views use: the record-view strip, the mixing strip, and a
/// tall and a short track row.
#[test]
fn every_example_hits_what_it_draws() {
    for name in EXAMPLES {
        let theme = layered(name);
        let cases: [(&str, (f64, f64), super::StripFields, &[&str]); 4] = [
            (
                STRIP,
                (86.0, 520.0),
                strip_fields(false),
                &["fx", "pan", "mute", "solo", "arm", "fader", "name"],
            ),
            (
                STRIP,
                (86.0, 520.0),
                strip_fields(true),
                &["fx", "pan", "mute", "solo", "fader", "name"],
            ),
            (
                ROW,
                (343.0, 70.0),
                row_fields(70.0),
                &["mute", "solo", "volume", "pan", "name", "arm"],
            ),
            (
                ROW,
                (343.0, 28.0),
                row_fields(28.0),
                &["mute", "solo", "name"],
            ),
        ];
        for (role, size, fields, controls) in cases {
            for control in controls {
                let rect = theme
                    .target_rect(role, size, &fields, control)
                    .unwrap_or_else(|| panic!("{name} {role} {size:?}: no {control} shown"));
                assert!(
                    rect.width() >= 8.0 && rect.height() >= 8.0,
                    "{name} {role} {size:?}: {control} is {rect:?}"
                );
                assert!(
                    rect.x0 >= 0.0
                        && rect.y0 >= 0.0
                        && rect.x1 <= size.0 + 0.5
                        && rect.y1 <= size.1 + 0.5,
                    "{name} {role} {size:?}: {control} at {rect:?} is outside the instance",
                );
                let hit = theme.hit(
                    role,
                    size,
                    &fields,
                    Point::new(rect.center().x, rect.center().y),
                );
                assert_eq!(
                    hit.as_deref(),
                    Some(*control),
                    "{name} {role} {size:?}: the middle of {control}"
                );
            }
        }
    }
}

/// Switching themes live, then breaking the one on screen: the broken
/// file is refused and the last good theme stays; fixing it takes again.
#[test]
fn a_broken_example_keeps_the_last_good_one() {
    let dir = std::env::temp_dir().join(format!("fts-theme-examples-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for f in std::fs::read_dir(themes().join("workbench")).unwrap() {
        let f = f.unwrap().path();
        std::fs::copy(&f, dir.join(f.file_name().unwrap())).unwrap();
    }
    let tokens = dir.join("tokens.frame");
    let good = std::fs::read_to_string(&tokens).unwrap();

    super::load(
        vec![themes().join("console"), themes().join("graphite")],
        None,
    )
    .unwrap();
    let graphite = super::current().unwrap();
    super::load(vec![themes().join("console"), dir.clone()], None).unwrap();
    let workbench = super::current().unwrap();
    assert!(
        !std::sync::Arc::ptr_eq(&graphite, &workbench),
        "the switch took"
    );
    assert_ne!(
        graphite.token("accent").and_then(|v| v.color()),
        workbench.token("accent").and_then(|v| v.color()),
    );

    let before = super::version();
    std::fs::write(&tokens, good.replace("</Variables>", "<Var name=")).unwrap();
    std::thread::sleep(super::POLL + std::time::Duration::from_millis(50));
    assert!(!super::poll(), "a broken file is not taken");
    assert_eq!(super::version(), before);
    assert!(
        std::sync::Arc::ptr_eq(&super::current().unwrap(), &workbench),
        "the last good one stays"
    );

    std::fs::write(&tokens, good.replace("#4fc3a1", "#ff00ff")).unwrap();
    std::thread::sleep(super::POLL + std::time::Duration::from_millis(50));
    assert!(super::poll(), "the fixed file is taken");
    assert_eq!(
        super::current()
            .unwrap()
            .token("accent")
            .and_then(|v| v.color())
            .map(|c| c.to_rgba8()),
        Some([0xff, 0x00, 0xff, 0xff]),
    );
    let _ = std::fs::remove_dir_all(&dir);
}
