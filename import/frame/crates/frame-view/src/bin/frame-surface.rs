//! `frame-surface`: a process extension (`extension.toml`) whose view, of
//! the `frame.surface` kind, is a knob and a meter from a Frame document,
//! and whose actions move and read them. Started by a host; run by hand it
//! says so and exits. `frame-surface --manifest` prints its manifest, for a
//! host that runs it as a development copy.

use architect_extensions::guest::{Guest, GuestError};
use frame_view::surface::State;
use frame_view::view::{self, FrameSurface};
use serde_json::{Value, json};

fn state() -> Value {
    let s = view::state();
    json!({ "knob": s.knob, "meter": s.meter })
}

fn main() -> Result<(), GuestError> {
    if std::env::args().nth(1).as_deref() == Some("--manifest") {
        print!("{}", frame_view::MANIFEST);
        return Ok(());
    }
    Guest::new()
        .view("surface", FrameSurface)
        .action("FRAME_SURFACE_SET", |args: Value| async move {
            let now = view::state();
            let read = |key: &str, was: f64| -> Result<f64, String> {
                match args.get(key) {
                    None | Some(Value::Null) => Ok(was),
                    Some(v) => v
                        .as_f64()
                        .filter(|v| (0.0..=1.0).contains(v))
                        .ok_or_else(|| format!("`{key}` is a number from 0 to 1, not {v}")),
                }
            };
            view::set(State {
                knob: read("knob", now.knob)?,
                meter: read("meter", now.meter)?,
            });
            Ok(state())
        })
        .action("FRAME_SURFACE_STATE", |_| async { Ok(state()) })
        .serve()
}
