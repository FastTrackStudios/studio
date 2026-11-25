//! `frame-surface`: a process extension (`extension.toml`) whose view, of
//! the `frame.surface` kind, is a Frame document's surface, and whose
//! actions load the document and move and read its params and feeds
//! (`frame_view::shown`). Started by a host; run by hand it says so and
//! exits. `frame-surface --manifest` prints its manifest, for a host that
//! runs it as a development copy.
//!
//! It shows the document at `FRAME_SURFACE_DOCUMENT` when that is set,
//! the built-in knob and meter otherwise, until `FRAME_SURFACE_LOAD`
//! loads one; a loaded document is watched, and a save reloads it.

use std::path::Path;

use architect_extensions::guest::{Guest, GuestError};
use frame_view::shown;
use frame_view::view::FrameSurface;
use serde_json::Value;

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| {
        format!(
            "`{key}` is a string, not {}",
            args.get(key).unwrap_or(&Value::Null)
        )
    })
}

fn main() -> Result<(), GuestError> {
    if std::env::args().nth(1).as_deref() == Some("--manifest") {
        print!("{}", frame_view::MANIFEST);
        return Ok(());
    }
    if let Some(path) = std::env::var_os("FRAME_SURFACE_DOCUMENT")
        && let Err(e) = shown::load(Path::new(&path))
    {
        eprintln!("frame-surface: {e}");
    }
    shown::watch();
    Guest::new()
        .view("surface", FrameSurface)
        .action("FRAME_SURFACE_LOAD", |args: Value| async move {
            shown::load(Path::new(text(&args, "path")?))
        })
        .action("FRAME_SURFACE_PARAM_SET", |args: Value| async move {
            let addr = text(&args, "addr")?;
            let value = args
                .get("value")
                .and_then(Value::as_f64)
                .filter(|v| (0.0..=1.0).contains(v))
                .ok_or_else(|| {
                    format!(
                        "`value` is a number from 0 to 1, not {}",
                        args.get("value").unwrap_or(&Value::Null)
                    )
                })?;
            shown::set_param(addr, value)
        })
        .action("FRAME_SURFACE_FEED", |args: Value| async move {
            let id = text(&args, "id")?;
            let values: Vec<f64> = match args.get("values") {
                Some(Value::Array(vs)) => vs
                    .iter()
                    .map(|v| {
                        v.as_f64()
                            .ok_or_else(|| format!("a reading is dB, not {v}"))
                    })
                    .collect::<Result<_, _>>()?,
                Some(Value::Number(n)) => n.as_f64().into_iter().collect(),
                v => {
                    return Err(format!(
                        "`values` is dB, a number per channel, not {}",
                        v.unwrap_or(&Value::Null)
                    ));
                }
            };
            shown::feed(id, &values)
        })
        .action("FRAME_SURFACE_STATE", |_| async { Ok(shown::state()) })
        .serve()
}
