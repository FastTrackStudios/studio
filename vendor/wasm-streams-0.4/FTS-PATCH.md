# wasm-streams 0.4 → a re-export of 0.5

**What:** this crate is named `wasm-streams` at version 0.4.3 and is one
line, `pub use streams::*;`, over the real `wasm-streams` 0.5. The root
`[patch.crates-io]` puts it in place of 0.4.2.

**Why:** Studio's build of Architect's web app (`apps/architect/web`)
links two copies of `wasm-streams` into one module: 0.4.2 through
`reqwest` 0.12's `stream` feature (Architect's `http-client`) and 0.5.0
through `dioxus-web`. Each exports the same `#[wasm_bindgen]` classes
(`IntoUnderlyingByteSource`, `IntoUnderlyingSink`, `IntoUnderlyingSource`),
so every library in the graph built as a `cdylib` without LTO
(`daw-standalone`, `architect-web`: cargo links a dependency's `cdylib`
too) fails with `rust-lld: error: duplicate symbol:
__wbg_intounderlyingbytesource_free`.

0.5.0 changed no API that 0.4's users call (its changelog: `panic=unwind`
support, an error state kept, lifetime warnings, newer `wasm-bindgen` and
`web-sys`); `reqwest` uses only `ReadableStream::from_raw(..).into_stream()`.

**Drop it** when `reqwest` 0.12 is gone from the browser graph (Architect
on `reqwest` 0.13, which takes `wasm-streams` 0.5), or `dioxus-web` and
`reqwest` agree on one version again.
