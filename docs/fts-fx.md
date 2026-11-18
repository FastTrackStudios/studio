# The effects are in fts-fx

Since studio#510 the effects are their own repo,
[FastTrackStudios/fts-fx](https://github.com/FastTrackStudios/fts-fx): they
build and release there, and Studio links them.

## What is where

| In fts-fx | Still here |
|---|---|
| `dsp-core`, `dsp` (`features/dsp`) | `signal-nam` (`signal:features/dsp/nam`): Signal's, in FastTrackStudios/signal. `builtin`, the effects as engine blocks, went with the engine to FastTrackStudios/daw (`daw:features/dsp/builtin`): it needs the engine, and the engine needs `dsp`, so it cannot live in fts-fx without the two repos pinning each other. |
| `fts-audio-ui`, `plugin-core` (`features/ui`), `preset-browser` | |
| The effect plugins: comp, delay, eq, gate, level, limiter, meter, mic, modulation, pitch, reverb, saturate, trigger, tune, unison, and `eq-standalone` | `guide-plugin` (the session), `fts-clap-host` (the engine). `nam-plugin`, `signal-plugin` and `signal-sampler-clap` (the sampler) are in FastTrackStudios/signal. |
| The bundler recipes for those (`just plugins-bundle`, `plugins-link`, `plugins-verify`) | The same recipes, for guide (and in FastTrackStudios/signal, for nam and signal). |

The comparison tools that drive a commercial plugin against ours
(`eq_match`, `reverb_match`, the probes; they need `signal-plugin-host`
and `builtin`) are examples of `builtin`, in the daw repo
(`cargo run -p builtin --example eq_match` there). The one that reads
Pro-Q's presets with `signal-import` is that crate's example here:
`cargo run -p signal-import --example proq_params`.

## The pin

The root `Cargo.toml` takes every fts-fx crate from one `rev`. To move it,
change the rev on every line (they are together under the comment that
names fts-fx) and run `cargo metadata` to update `Cargo.lock`. fts-fx
declares the same versions, git pins and `[patch]` entries as this repo
(blitz, dioxus, architect, nice-plug, wgpu): a bump of one of those lands
in both repos, fts-fx first.

fts-fx never depends on this repo, nor on FastTrackStudios/daw: the engine there
depends on fts-fx, one way. Move the fts-fx rev here and in the daw repo
together.

## Working on both at once

Check fts-fx out beside this repo and add to the root `Cargo.toml`, without
ever committing it (the paths are machine-specific):

```toml
[patch."https://github.com/FastTrackStudios/fts-fx"]
dsp = { path = "../fts-fx/features/dsp" }
dsp-core = { path = "../fts-fx/features/dsp/core" }
fts-audio-ui = { path = "../fts-fx/features/ui/fts-audio-ui" }
plugin-core = { path = "../fts-fx/features/ui/plugin-core" }
preset-browser = { path = "../fts-fx/features/preset-browser" }
comp-plugin = { path = "../fts-fx/apps/plugins/comp" }
delay-plugin = { path = "../fts-fx/apps/plugins/delay" }
eq-plugin = { path = "../fts-fx/apps/plugins/eq" }
gate-plugin = { path = "../fts-fx/apps/plugins/gate" }
level-plugin = { path = "../fts-fx/apps/plugins/level" }
limiter-plugin = { path = "../fts-fx/apps/plugins/limiter" }
meter-plugin = { path = "../fts-fx/apps/plugins/meter" }
mic-plugin = { path = "../fts-fx/apps/plugins/mic" }
modulation-plugin = { path = "../fts-fx/apps/plugins/modulation" }
pitch-plugin = { path = "../fts-fx/apps/plugins/pitch" }
reverb-plugin = { path = "../fts-fx/apps/plugins/reverb" }
saturate-plugin = { path = "../fts-fx/apps/plugins/saturate" }
tune-plugin = { path = "../fts-fx/apps/plugins/tune" }
unison-plugin = { path = "../fts-fx/apps/plugins/unison" }
```

Patch all of them, not one: a crate left on the git rev brings a second
copy of `dsp` with it. `Cargo.lock` changes while the patch is there; do
not commit that either. When the work is done, merge the fts-fx side, then
move the rev here.

## FTS Mic's models

`mic-plugin` builds the default mic in when it finds the private models at
build time. As a git dependency it no longer sees `apps/plugins/mic/models`:
it finds them in `$FTS_MIC_MODELS` or `~/.local/share/fts/mic-models`
(`just mic-models-install`).
