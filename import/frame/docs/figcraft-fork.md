# The figcraft fork

Frame's editor runs on figcraft's document model and engine. We depend on a fork,
FastTrackStudios/figcraft, branch `fts`, **by tag** `fts-v<upstream>-<n>` (now `fts-v0.14.0-1`).
Only `frame-doc` (the doc-level crates) and `frame-editor` (the engine) depend on it; `frame`
core never does (`just boundary`).

- The engine is used slim: `figcraft-engine` with `default-features = false` leaves out PDF export
  (krilla), wasm plugins (wasmi), font downloads (craft-font-store) and VectorDrawable. `cargo tree
  -p frame-editor` shows none of them.
- The fork's patch list and policy are its `FORK.md`. Nothing goes upstream without Cody's yes.
- Licence: Apache-2.0 for figcraft; `NOTICE` names it and storytold/craft-libs.

## Linux consumers using fontique 0.12

The editor's figcraft dependency brings in `craft-font-catalog` and fontique 0.11 with
`fontconfig-dlopen`. Both fontique 0.11 and 0.12 use `yeslogic-fontconfig-sys`, whose features
Cargo unifies across those versions. A Linux consumer using fontique 0.12 with linked fontconfig
symbols must align it to `fontconfig-dlopen` when the editor is in the same dependency graph.
Studio verified this configuration by compiling its combined Frame/Editor graph and running its
four theme examples:

```toml
# Workspace dependencies
fontique = { version = "0.12", features = ["fontconfig-dlopen"] }

# In the Linux UI crate's target-specific dependencies
[target.'cfg(target_os = "linux")'.dependencies]
fontique.workspace = true
```

This adjustment applies to consumers that combine these dependencies; Frame's core remains free
of figcraft. Other consumers should check their own resolved feature graph before applying it.

## Bumping to a new upstream release

The fork rebases onto upstream **release tags**, never `main` (upstream ships ~120 commits a day),
at most every two weeks or when we need a fix.

1. On the fork: `git rebase --onto vX.Y.Z <old-base> fts`; run `cargo xtask layers`, upstream's tests
   and `cargo check -p figcraft-engine --no-default-features`.
2. Tag `fts-vX.Y.Z-1` and push the branch and the tag.
3. Here, in its own PR: change the tag in the `figcraft-*` entries of the root `Cargo.toml`, then run
   `just check`, `just boundary` and the parity suite (`just parity`).
