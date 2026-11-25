# Manifest merge report

- members: 232; excluded: 13
- member manifests: 17 dependency entries rewritten
- root: 123 entries rewritten, 131 added from the imported workspaces

## Path deps that resolve to nothing (1)

- `plugin-faces` (../../examples/plugin-faces → docs/imported/frame/examples/plugin-faces) in crates/studio-core/frame/frame-doc/Cargo.toml [dev-dependencies]

## Path deps on deleted crates (6)

- `launcher` (features/app/launcher) in Cargo.toml [workspace.dependencies]
- `productions` (features/productions) in Cargo.toml [workspace.dependencies]
- `setlists` (features/setlists) in Cargo.toml [workspace.dependencies]
- `studio-extensions` (features/distribution) in Cargo.toml [workspace.dependencies]
- `studio-island` (features/island) in Cargo.toml [workspace.dependencies]
- `studio-launcher` (extensions/launcher) in Cargo.toml [workspace.dependencies]

## Version clashes (studio's entry kept) (51)

- `architect-extensions`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · Ignition `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-extensions`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · frame `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'cbdc7c7d7149d40577688554c8b1ad652466f1dc'}`
- `architect-extensions`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · keyflow `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `architect-extensions`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · rhythm `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `architect-input`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · studio-adapters `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.12.6'}`
- `architect-mcp`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · frame `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'cbdc7c7d7149d40577688554c8b1ad652466f1dc'}`
- `architect-process-view`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · Ignition `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-telemetry`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.15.0', 'features': ['otel']}` · signal `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}`
- `architect-theme`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · keyflow `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `architect-ui`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · plugins `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.12.6'}`
- `architect-ui`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · studio-adapters `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.12.6'}`
- `architect-workspace`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · Ignition `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-workspace`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · frame `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'cbdc7c7d7149d40577688554c8b1ad652466f1dc'}`
- `architect-workspace`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · keyflow `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `architect`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · frame `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'cbdc7c7d7149d40577688554c8b1ad652466f1dc'}`
- `architect`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · session `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `architect`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · studio-adapters `{'git': 'https://github.com/FastTrackStudios/architect', 'tag': 'v0.12.6'}`
- `clap`: studio `{'version': '4', 'features': ['derive']}` · pitch `{'version': '4.5', 'features': ['derive']}`
- `clap`: studio `{'version': '4', 'features': ['derive']}` · rhythm `{'version': '4.5', 'features': ['derive']}`
- `crdt`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · frame `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'cbdc7c7d7149d40577688554c8b1ad652466f1dc'}`
- `crdt`: studio `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede'}` · session `{'git': 'https://github.com/FastTrackStudios/architect', 'rev': 'eaa969f150b4c558b4722c09e7cf619d9fa4bede', 'default-features': False}`
- `dioxus-native`: studio `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.7.1', 'default-features': False, 'features': ['prelude', 'system-fonts', 'svg', 'autofocus', 'data-uri', 'vello', 'accessibility', 'hot-reload', 'net', 'html', 'clipboard', 'file-dialog', 'vello-hybrid', 'woff', 'apple-font-embolden']}` · keyflow `{'git': 'https://github.com/DioxusLabs/dioxus', 'rev': '430cfb563f1b70602ef387f0c80f171d84527b41', 'features': ['prelude', 'system-fonts', 'svg', 'autofocus', 'data-uri', 'vello']}`
- `dioxus-ssr`: studio `{'git': 'https://github.com/DioxusLabs/dioxus', 'rev': '430cfb563f1b70602ef387f0c80f171d84527b41', 'default-features': False}` · keyflow `{'git': 'https://github.com/DioxusLabs/dioxus', 'rev': '430cfb563f1b70602ef387f0c80f171d84527b41'}`
- `editor-state`: studio `{'git': 'https://github.com/FastTrackStudios/editor', 'rev': '6817dc2c709fbb5a7858229592bbbc85eed49528', 'default-features': False}` · keyflow `{'git': 'https://www.github.com/FastTrackStudios/editor', 'rev': '6817dc2c709fbb5a7858229592bbbc85eed49528', 'default-features': False}`
- `editor-view`: studio `{'git': 'https://github.com/FastTrackStudios/editor', 'rev': '6817dc2c709fbb5a7858229592bbbc85eed49528', 'default-features': False}` · keyflow `{'git': 'https://www.github.com/FastTrackStudios/editor', 'rev': '6817dc2c709fbb5a7858229592bbbc85eed49528', 'default-features': False}`
- `facet`: studio `{'version': '0.50.0-rc.5', 'features': ['reflect', 'chrono', 'uuid']}` · frame `{'version': '0.50.0-rc.7', 'features': ['reflect', 'chrono', 'uuid']}`
- `facet`: studio `{'version': '0.50.0-rc.5', 'features': ['reflect', 'chrono', 'uuid']}` · keyflow `{'version': '0.50.0-rc.7', 'features': ['reflect', 'chrono', 'uuid']}`
- `facet`: studio `{'version': '0.50.0-rc.5', 'features': ['reflect', 'chrono', 'uuid']}` · plugins `{'version': '0.50.0-rc.5'}`
- `facet`: studio `{'version': '0.50.0-rc.5', 'features': ['reflect', 'chrono', 'uuid']}` · session `{'version': '0.50.0-rc.5'}`
- `loro`: studio `{'version': '1.12', 'default-features': False}` · frame `1.16`
- `nice-plug-core`: studio `{'git': 'https://github.com/FastTrackStudios/nice-plug.git', 'branch': 'fts-baseview-03'}` · frame `{'git': 'https://github.com/FastTrackStudios/nice-plug.git', 'tag': 'fts-upstream-2026-11'}`
- `rayon`: studio `1.12` · rhythm `1.11`
- `realfft`: studio `3.0` · pitch `3.5`
- `realfft`: studio `3.0` · stretch `3.5`
- `serde_json`: studio `1.0` · pitch `{'version': '1.0'}`
- `tempfile`: studio `3.8` · keyflow `3`
- `thiserror`: studio `2.0` · session `2`
- `tokio`: studio `{'version': '1.48', 'features': ['full']}` · keyflow `{'version': '1', 'default-features': False}`
- `tokio`: studio `{'version': '1.48', 'features': ['full']}` · session `{'version': '1', 'default-features': False}`
- `uuid`: studio `{'version': '1', 'features': ['v4', 'v5', 'v7', 'serde']}` · Ignition `{'version': '1', 'features': ['v4']}`
- `uuid`: studio `{'version': '1', 'features': ['v4', 'v5', 'v7', 'serde']}` · keyflow `{'version': '1', 'features': ['v4']}`
- `uuid`: studio `{'version': '1', 'features': ['v4', 'v5', 'v7', 'serde']}` · session `{'version': '1', 'features': ['v4']}`
- `vello`: studio `0.11` · plugins `{'version': '0.11', 'features': ['wgpu']}`
- `vello`: studio `0.11` · signal `{'version': '0.11', 'features': ['wgpu']}`
- `vello`: studio `0.11` · studio-adapters `{'version': '0.11', 'features': ['wgpu']}`
- `vox-core`: studio `{'version': '0.10.0-rc.5'}` · frame `{'version': '0.10.0-rc.7'}`
- `vox`: studio `{'version': '0.10.0-rc.5', 'default-features': False, 'features': ['runtime']}` · frame `{'version': '0.10.0-rc.7', 'default-features': False, 'features': ['runtime']}`
- `vox`: studio `{'version': '0.10.0-rc.5', 'default-features': False, 'features': ['runtime']}` · keyflow `{'version': '0.10.0-rc.7', 'default-features': False, 'features': ['runtime']}`
- `vox`: studio `{'version': '0.10.0-rc.5', 'default-features': False, 'features': ['runtime']}` · session `{'version': '0.10.0-rc.7', 'default-features': False, 'features': ['runtime']}`
- `wgpu`: studio `{'version': '30', 'features': ['wgsl']}` · Ignition `30`
- `wgpu`: studio `{'version': '30', 'features': ['wgsl']}` · frame `30`

## Internal deps with no crate in the monorepo (gone: fts-fx, …) (1)

- `ignition-mobile` in docs/imported/Ignition/workspace.Cargo.toml [workspace.dependencies]

## [patch] clashes (studio's entry kept) (47)

- `anyrender` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `anyrender` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `anyrender` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `anyrender` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `architect-action-derive` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-derive` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-devices` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-orgs` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-permissions` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect-rpc-derive` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `architect` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `bevy_pbr` in [patch.https://github.com/bevyengine/bevy]: Ignition `{'path': 'crates/studio-connect/views/lighting/vendor/bevy_pbr'}`
- `blitz-dom` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-dom` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-dom` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-dom` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-html` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-html` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-html` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-html` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-net` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-net` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-net` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-net` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-paint` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-paint` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-paint` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-paint` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-shell` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-shell` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-shell` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-shell` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-traits` in [patch.crates-io]: daw `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-traits` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/blitz', 'tag': 'fts-upstream-2026-11'}`
- `blitz-traits` in [patch.crates-io]: plugins `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `blitz-traits` in [patch.crates-io]: studio-adapters `{'git': 'https://www.github.com/FastTrackStudios/blitz', 'rev': 'db5b7ef11e4df19ef317c23f77152ec5e1887101'}`
- `crdt-derive` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `crdt` in [patch.https://github.com/FastTrackStudios/architect]: Ignition `{'git': 'https://www.github.com/FastTrackStudios/architect', 'tag': 'v0.15.0'}`
- `dioxus-native-dom` in [patch.https://github.com/DioxusLabs/dioxus]: frame `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.7.1'}`
- `facet-core` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `facet-core` in [patch.crates-io]: keyflow `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `facet-core` in [patch.crates-io]: rhythm `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `gdtf` in [patch.crates-io]: Ignition `{'path': 'crates/studio-engine/lighting/vendor/gdtf'}`
- `phon` in [patch.crates-io]: frame `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `phon` in [patch.crates-io]: keyflow `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `phon` in [patch.crates-io]: rhythm `{'git': 'https://github.com/FastTrackStudios/vendor', 'tag': 'v0.4.0'}`
- `tone3000` in [patch.crates-io]: signal `{'path': 'crates/studio-engine/signal/vendor/tone3000'}`

## Imported [patch] paths that resolve to nothing (1)

- `architect-workspace-hack` from session: vendor/no-workspace-hack

## Removed [patch] tables for internal repos (1)

- `[patch."https://github.com/FastTrackStudios/daw"]`

