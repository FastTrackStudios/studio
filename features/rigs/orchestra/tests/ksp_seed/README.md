# KSP Seed — reproducible Kontakt renders

Kontakt's KSP `random(a, b)` (decoded from Kontakt 8.7.1, macOS arm64,
`BScriptParser::EvalExpression`) is

```
v = ((rand() << 30) | (rand() << 15) | rand()) & 0x7fffffff   // 32-bit shifts
random(a, b) = v % (max − min + 1) + min
```

on the C library's `rand()`: on macOS, Park–Miller (`x = 16807·x mod 2^31−1`,
unseeded state 1). Kontakt seeds it with `srand(time(NULL))`, so two renders
of one project draw differently.

`ksp_seed.c` is a CLAP plugin. Placed in a render's FX chain before Kontakt
and run with `KSP_SEED=<n>`, it rebinds Kontakt's imports of `rand()` /
`srand()` to its own Park–Miller state, pinned to `n` at transport start, so
every KSP draw depends only on the seed. KONTRA replays the same draws with
`KONTRA_KSP_SEED=<n>` (local KONTRA branch `local/inspect-vars`).
`KSP_SEED_LOG=<file>` logs each call with its caller in the Kontakt image.
Unset, the plugin does nothing.

Build on voyager (CLAP headers from free-audio/clap 1.2.2, MIT):

```sh
clang -O2 -arch arm64 -arch x86_64 -bundle -I clap/include \
  -o ~/Library/Audio/Plug-Ins/CLAP/KspSeed.clap/Contents/MacOS/KspSeed ksp_seed.c
codesign -f -s - ~/Library/Audio/Plug-Ins/CLAP/KspSeed.clap
```

plus an `Info.plist` (`CFBundleExecutable KspSeed`, package type `BNDL`), and a
`[KspSeed.clap]` entry in the render config's `reaper-clap-macos-aarch64.ini`
(headless `-renderproject` does not rescan). `seed_renders.sh <sec dir> <mode>
<name:seed>...` renders seeded copies of `<mode>_r1.rpp`.

Verified: two `KSP_SEED=777` renders of the CSSS Violin I probe null to
−300 dB on every unit; the first draw is 16807 × 777.
