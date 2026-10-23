//! Every integration test of this crate, in one binary.
//!
//! Each former `tests/<name>.rs` is a module here: one binary links the
//! crate's dependencies once instead of once per file (see the
//! `rust-testing` skill). A module that needed cargo features has them as
//! a `cfg`; run one with `cargo nextest run -p <crate> -E 'test(/^<module>::/)'`.

mod aiff_decode;
mod aiff_real;
mod annotation_parity;
mod css_parse;
mod document_mode;
mod drumkit_parse;
mod gapless_reload;
mod gapless_switch;
mod hosted;
mod keyscape_rhodes;
mod live_params;
mod live_reload;
mod load_pack;
mod mm2_parse;
mod native_gate;
mod orchestral_parse;
mod pack_dump;
mod pitch_verify;
#[cfg(all(feature = "library"))]
mod plan_roundtrip;
mod playable;
mod probe_mics;
mod retag_verify;
mod stylus_parse;
mod switch_state;
mod tagging_metadata;
mod wavetable_parse;
mod zonemap_parse;
