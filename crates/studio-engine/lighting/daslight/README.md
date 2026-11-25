# ignition-daslight

Reads and writes Daslight 5 / myDMX 5 showfiles (`.dvc`): the patch, the
fixture profiles embedded in it, fixture groups, and scenes (written; read
back by name only).

```sh
cargo test -p ignition-daslight
# a venue's showfile, address list and TEST bank, from its fixtures.json
# (written to the venue's daslight/ folder)
cargo run -p ignition-daslight --example mobile_rig -- \
    features/lighting/daslight/tests/data/mobile-lighting.dvc \
    features/lighting/data/venues/room138-cbu
# with every Ignition look and effect cooked into the LOOKS, TOWERS, PARS,
# BARS, MOVERS, FX and STROBES banks, and the desk's own effects in NATIVE
# (run from features/lighting: the cook finds the profile under data/)
cd features/lighting
cargo run --release -p ignition-viz --example cook_dmx -- data/venues/room138-cbu /tmp/cooked.json
cargo run --release -p ignition-daslight --example mobile_rig -- \
    daslight/tests/data/mobile-lighting.dvc data/venues/room138-cbu /tmp/cooked.json
```

## The format

Plain XML rooted at `<DLMFILE TYPE="Daslight" VERSION="5">`. Bulk data is
zlib packed into attributes (`src/container.rs`):

| Where | Container |
|---|---|
| `PATCHS DATA` | base64 of `uint32be length + finished zlib` |
| `CONFIGURATION TOUCH_DOCK_MANAGER` | the same, hex |
| fixture `OUTMODE`, `FLAG` | hex of a sync-flushed zlib stream, no length |
| scene `FIXTUREDATA DATA` | base64 of the same |

## What the codes mean

Read from real showfiles, not from documentation.

`SSLCHANNELTYPE`: 0 other, 1 pan, 2 tilt (fine halves have
`SSLCHANNELLSB="1"`, coarse `SSLCHANNELMSB="1"`), 7 dimmer, 8 gobo, 15
strobe, 18 speed, 25/26/27 red/green/blue, 31 white, 37 macro, 43 warm white.

`FLAG`: one byte per channel. `0xc0` dimmer, `0x80` channels that fade
(colour, white, position), `0` channels that snap. `OUTMODE`: one zero byte
per channel.

`SSLFIXTTYPE`: 2 a par or bar, 3 a multi-cell LED fixture. The moving-head
code is unconfirmed (0 is written).

A profile's `SSLPROPERTIES > SSLBEAMS` is its cell grid, and a channel's own
`SSLBEAMS` lists the cells it drives. That is how a per-cell effect knows
which channel is which cell.

## Scenes and their racks

A `SCENE` holds its own `FIXTUREDATAS` (what the desk shows when it is
selected) and a `RACKS` list (`src/scene.rs`, `src/fx.rs`):

```text
SCENE DASUID NAME COLOR … LOOP_MODE="0" LOOP="0" AUTO_RELEASE_END="1"
  FIXTUREDATAS NB=          one FIXTUREDATA per fixture (DATA, above)
  RACKS
    RACK TYPE="9"           steps: a static scene is one step
      STEPS NB=
        STEP WAITTIME= FADETIME=   1/25 s ticks (25 is a second)
          FIXTUREDATAS NB=         that step's values
    RACK EXPAND_RACK="1" TYPE="2"  Color FX   (TYPE="4": Move FX)
      EFFECT TYPE="2" ID= DURATION="5000"
        PARAMS NB=
          PARAM TYPE="4" ID="1" > COLORS NB= > COLOR VAL="r/g/b/…"   palette
          PARAM TYPE="5" ID="1" > POINTS NB= > POINT X= Y=           Move FX shape
          PARAM TYPE= ID= VAL=                                       the rest
      BEAMS NB=
        BEAM FIXTURE=uid BEAMID=cell IDSELECTION=1..n              effect order
```

A `FIXTUREDATA DATA` is `uint16be` channel count, a `uint16be` per channel
(0..255, `0xFFFF` not in the scene), `uint32be` cell count, then per cell
13 half floats and a zero byte: the stage view's picture (red, green,
blue, warm white in slots 0-3, -1 unset; 8 and 11 are 1.0).

A scene that runs a Color or Move FX rack carries that rack and no steps
rack, as all seven effect scenes in a desk-programmed showfile (Gearbox)
do; its `FIXTUREDATAS` are the values under the effect (dimmers up,
strobes and modes off). The effects written, with the parameters copied
from those scenes (which parameter is which is only partly known):

| Effect | Rack, `EFFECT ID` | Palette | Own parameters `(TYPE, ID, VAL)` |
|---|---|---|---|
| Rainbow | 2, 130 | the desk's 9-colour wheel | (1,10,0.028) (0,11,0) (1,12,1) |
| Knight Rider | 2, 127 | black, red | (0,10,9) (2,11,1) (2,12,1) (2,13,0) (0,14,50) |
| Sparkle | 2, 121 | the wheel and a deep red | (0,10,10) (1,11,1) |
| Perlin | 2, 128 | the wheel | (0,10,4) (0,11,75) (0,12,2) (0,13,1) (0,14,48) |
| Move, slow | 4, 224 | 4 points | (1,2,0.085) speed? (2,3,0) |
| Move, fast | 4, 221 | 4 points | (1,2,0.114) speed? (2,3,0) |

Every Color FX also carries `(2,2,0)` and `(6,3,0)`. A palette `COLOR` is
`r/g/b` (0..1) then `/0/0/0/0/0/1/1/1/1/0/0/0/0/0/1`. A mover is two
`BEAM`s (`BEAMID` 0 and 1), as the desk writes it; Move FX points are on
the pan/tilt grid, 0..1. Not modelled: how `LOOP_MODE`/`LOOP` spell "play
N times" (every scene read is 0/0, "always loop").

## Cooked scenes

`cook` turns DMX recorded off another controller (`ignition-viz`'s
`cook_dmx` example: every look and library effect, 24 s at 10 fps) into
Steps scenes: find where the recording repeats after a 2 s warm-up
(`find_cycle`; a period that falls between frames is fitted to the
nearest whole frame), fold runs of identical frames into one step, keep
at most 80 steps (a longer cycle keeps every n-th frame), and slice each
step per fixture by address. A cycle whose median change is small fades
between steps over one frame's time; chases and strobes snap. Fine pan
and tilt bytes are left out of that test (`coarse`): they wrap every few
frames of a sweep. An effect that ends held is a one-shot, cooked from
its first frame to where it lands; one that never changes is skipped.
`mobile_rig` writes what each became to the venue's `daslight/COOKED.md`.

## Library files and EasyView

The desk's `ScanLibrary/*.ssl2` and EasyView's demo `.ev2` projects are
XORed with one fixed keystream (`DasCryptKey` in `libQtDasCore`), the same
for every file. A file that contains `xml version`, or that starts with
`<`, is read as plain XML, so this crate writes them plain and never
encrypts.

- `.ssl2`: `<?xml …?><DLMFILE VERSION="3" TYPE="">` around one
  `SSLLIBRARY`, the same element a showfile embeds. EasyView looks a
  patched fixture's profile up in `ScanLibrary/<SSLNAME>` and refuses the
  fixture when the file is missing.
- `.ev2` (`src/easyview.rs`): `EV2FILE VERSION="3"`, the profiles embedded
  under `FIXTURES`, then `OBJECTS`: a `Scene` object of six planes and a
  `Fixtures` object of per-type groups of `FIXTURE` elements, each linked to
  the desk by `DASUID`. Centimetres, Y up, +Z to the back wall. A plane that
  has a `COLOR` must carry the whole material (`TRANSPARENCY`, `EMISSIVE`,
  `SPECULAR`, `TEXTUREFILE`, `TEXTURESCALE`): without `TEXTURESCALE`
  EasyView 2.0.3 crashes on load. A fixture's rest pose hangs straight down;
  `ROTATION` is a tilt about X, a turn about Y, then a roll about Z.

Scenery (`easyview::project_in`): any number of `panel`s (a coloured
plane), `block`s (a box of six panels) and `Model`s (a mesh from
EasyView's own `Library/` folder, `FILE3D="./Library/…"`, `.x` or `.dae`,
stretched to a box). `ROTATION` is applied about X, then Y, then Z, each
about the room's axes. EasyView loads a mesh centred on its bounding box,
not at its own origin. The example's `support/scenery.rs` builds Room
138 from the venue's `environment.json`, `room.json` and `props.json`.
A profile whose `SSL3DOBJECTFILE2` is `:/FIXTURES/LEDS/LED` (the desk's
generic LED Bar) is drawn cell by cell; `:/FIXTURES/TRADS/CHANGER` is one
lens.

The desk's 2D view: a fixture's `BEAM`s are stored unturned (its grid
offsets from `POSX`/`POSY`) and the desk turns them by `ANGLE` itself, as
in a desk-saved show. The example lays the rig out as seen from the
audience (`front_view` in `mobile_rig.rs`) so the desk's pixel effects
sweep the stage as Ignition's do.

The example writes both into the venue's `daslight/` folder; copy
`ScanLibrary/*` into `/Applications/myDMX 5/ScanLibrary/` on the desk.
