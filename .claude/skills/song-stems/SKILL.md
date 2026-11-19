---
name: song-stems
description: Get a song's reference recording and split it into 11 stems (Vocals, Bass, Guitar, Piano, Other, Kick, Snare, Hi-Hat, Toms, Cymbals, Drums Other) with Logic Pro's Stem Splitter on voyager, collected into a "<Song> Reference" folder. Use when asked to stem-split or multitrack-separate a song, make reference stems, or as step 1 of `song-prep`.
---

# Reference recording and stems

Output: a folder `<Song> Reference/` holding `<basename> (Mix).wav` and 11 files
`<basename> (<Stem>).wav`, every one the exact length of the source, so they
all sit at 0 in a project.

Where it runs: **voyager** (the owner's Mac, Logic Pro 13). No other machine has
a proven separator. From another machine, launch a thread on voyager (fleet-t3)
or work over `ssh voyager` for the shell parts; Logic itself is driven with the
`cua-driver` MCP tools there (load the `cua-driver` skill).

## 1. The recording

```bash
cd ~/Downloads && yt-dlp -x --audio-format wav --no-playlist \
  -o '%(id)s - %(title)s.%(ext)s' '<url>'
```

- Quote the URL: zsh globs the `?`.
- **Say the video's title and length back to the owner and stop if it is not the
  expected song and version.** Links are sometimes a different recording.
- Downloads go to `~/Downloads`, not a scratch folder.
- The id in the name comes from the template; keep it, the stems and the project
  name files by that basename. A `/` in a title becomes `⧸`.

## 2. Logic: import, split

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

## 3. Collect

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

The folder path, the recording (title, length), the stem list with levels, which
stems are silent, and which Logic project was left open and unsaved.

## Not proven

Separation anywhere but Logic on voyager. If another separator is tried (for
example Demucs on a GPU machine), it must produce the same 11 names at the
source's exact length, and its drum kit pieces must be checked by ear before
`song-tempo-map` relies on them; write the result here.
