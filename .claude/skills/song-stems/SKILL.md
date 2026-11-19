---
name: song-stems
description: Split a song's reference recording (an audio file you already have) into 11 stems (Vocals, Bass, Guitar, Piano, Other, Kick, Snare, Hi-Hat, Toms, Cymbals, Drums Other) — open models on a Linux GPU machine, or Logic Pro's Stem Splitter on macOS — collected into a "<Song> Reference" folder. Use when asked to stem-split or multitrack-separate a song, make reference stems, or as step 1 of `song-prep`.
---

# Stems from a reference recording

Output: a folder `<Song> Reference/` holding `<basename> (Mix).wav` and 11 files
`<basename> (<Stem>).wav`, every one the exact length of the source, so they
all sit at 0 in a project.

Two separators are proven. Use the one for the machine you are on; both give
the same 11 names, each the source's exact length and rate.

| Machine | Separator | Time for a 4-minute song |
|---|---|---|
| Linux with an NVIDIA GPU (THEBATTLESHIP) | `split_stems_linux.sh`: two open models, in two passes | about 1 minute |
| macOS (voyager) | Logic Pro 13 Stem Splitter, driven with `cua-driver` | about 2 minutes |

## 1. The recording

The process starts from an audio file. How it was obtained is up to whoever
brings it, and is not part of this skill.

- A WAV at the recording's own sample rate; convert anything else with ffmpeg
  first (`ffmpeg -i in.ext out.wav`).
- **Say the file's name, length and what it is (artist, version) back to the
  owner and stop if it is not the expected song and version.** Every later step
  is built on this one recording.
- Its name without the extension is the **basename**: the stems, the mix copy
  and the project are all named from it, so settle the name before splitting.

## 2. Linux: two passes on the GPU

```bash
scripts=<studio>/.claude/skills/song-prep/scripts
$scripts/split_stems_linux.sh "<source.wav>" "<Song> Reference"
```

- Pass 1, **BS-Roformer-SW**: vocals, drums, bass, guitar, piano, other.
  Pass 2, **MDX23C DrumSep** (aufr33/jarredou) on the drums stem: kick, snare,
  toms, hi-hat, ride, crash. Cymbals = ride + crash; Drums Other = the drums
  stem less the pieces. It also writes `(Drums)`, the whole kit, and `(Mix)`.
- Setup, once per machine: a venv at `/run/media/Development/tools/stem-sep`
  (`uv venv --python 3.11 .venv && uv pip install --python .venv/bin/python
  "audio-separator[gpu]"`, version 0.48.0 as tested); the models download on
  first use into `models/`. Another place: set `STEM_SEP`.
- The script prints `GPU: <name>`. `GPU: none` means the driver's libraries
  were not found and it will take about 30 times longer; on NixOS it adds
  `/run/opengl-driver/lib` itself. A `libcurand` warning is harmless.
- The models work at 44.1 kHz and turn loud input down; the script resamples
  back to the source's rate and restores the level (it prints the gain). Do not
  call `audio-separator` directly for a reference folder, or the stems come out
  at the wrong rate and level.

Against Logic on the same five recordings (`compare_stems.py <A> <B>`):
kick, snare, bass, guitar, vocals agree hit for hit (0.92–1.00); hi-hat and toms
mostly (0.5–0.8); **cymbals and Drums Other differ** (0.15–0.5), so check those
by ear before a chart or light cue leans on them. Its stems sum back to the mix
far more closely than Logic's (−24 to −34 dB against −10 to −14 dB). Where
Logic gave a silent kick and snare on a brush track (Rockin' Around the
Christmas Tree), this one gives quiet ones (−50 and −35 dB): listen before
trusting either.

## 3. macOS: Logic, import and split


Use an empty project. If the open project has content, **do not `File > New`**
(it would close the owner's unsaved project): import onto a new track at the end
of the open project and select only the new region. Never save or close a Logic
project.

1. `list_windows({app:"Logic Pro"})` for the pid and the `… - Tracks` window.
2. `invoke_menu(["File","Import","Audio File…"])`. The Open panel only shows
   while Logic is frontmost, and typed keys never reach it. What works:
   click the sidebar **Downloads** cell (AX), click the file's `AXRow`, then a
   pixel click on **Open** with `delivery_mode:"foreground"` (the button is not
   addressable by AX; a background pixel click is ignored).
3. With the region selected: tracks-area **Functions** menu (`AXMenuButton`
   "Functions") → **Stem Splitter…**. Name both by role and name in one
   `run_steps` batch; tokens go stale when the menu closes.
4. The dialog is an `AXSheet` on the main window. Defaults are right: all stems,
   Drums "Individual Kit Pieces" (6). Check the screenshot, press **Apply**.

`osascript` has no assistive access from the terminal, so it is not an option.

### Collect

Logic writes the stems while it splits, into
`<project>.logicx/Media/Audio Files/<basename>_<Stem>.wav`; no export is needed.

```bash
scripts=<studio>/.claude/skills/song-prep/scripts
$scripts/collect_stems.sh "$HOME/Music/Logic/Untitled N.logicx" \
  "$HOME/Downloads/<basename>.wav" "$HOME/Downloads/<Song> Reference"
cp -n "$HOME/Downloads/<basename>.wav" "$HOME/Downloads/<Song> Reference/<basename> (Mix).wav"
```

It waits until all 11 files stop growing (about 2 minutes for a 4-minute song),
copies and renames them, and prints each stem's length and mean volume. Run it
in the background and wait for it.

## Checks

- Every stem's length equals the source's. Stems exported by hand from Logic
  come out 1–6 s short and of differing lengths: re-collect instead of padding.
  `scripts/stemalign.py <folder> <mix>` reports each stem's offset to the mix.
- A stem near −90 dB means the instrument is not on the record (no kick or snare
  on a 1950s brush track, no guitar on a synth-pop one). That is normal; note it,
  because the tempo map must not lean on a silent stem.

## Report

The folder path, the recording (file, length), the stem list with levels, which
stems are silent, which separator was used, and (Logic) which project was left open and unsaved.
