# Mobile rig: setting every light's address

All 22 lights go on **one DMX line, universe 1**, from the myDMX 5 adapter.
The showfile that matches is `Mobile Lighting 33ch.dvc`.

On each light you set two things:

1. **The mode**, which sets how many channels the light listens to.
2. **The address**, which is its first channel.

If either one is wrong, that light, and often the ones after it, will do the
wrong thing.

"Left" and "right" below are as seen **from the audience, looking at the
stage**.

## Before you start

- **Label each light.** Put tape on each light with its name and address,
  e.g. `TOWER 1 — 001`. Then a light that's been moved can be checked at a glance.
- **Cable order doesn't matter.** The DMX cable can run through the lights in
  whatever order is easiest. The address decides which light is which, not its
  place on the cable.
- **Terminate the line.** Put a DMX terminator in the last light's DMX OUT,
  if you have one.
- **Finding the settings.** Every one of these lights sets its address and mode
  from the buttons by its little display (MENU / UP / DOWN / ENTER).
  - The address usually shows as `A001` or `d001`.
  - The mode shows as e.g. `CH33`, `33CH` or `Ch07`.
  - Exact menu names vary, so check the light's manual if you can't find one.
- **Set them to DMX, not auto.** Make sure each light is in DMX mode, not
  auto, sound or master/slave.

## The list

| ✓ | Light | Where it is | Mode | Address | Uses channels |
|---|---|---|---|---|---|
| ☐ | **Tower 1** (U'King ZQ01334) | back wall, far left | **33CH** | **001** | 1–33 |
| ☐ | **Tower 2** (U'King ZQ01334) | back wall, left of centre | **33CH** | **034** | 34–66 |
| ☐ | **Tower 3** (U'King ZQ01334) | back wall, right of centre | **33CH** | **067** | 67–99 |
| ☐ | **Tower 4** (U'King ZQ01334) | back wall, far right | **33CH** | **100** | 100–132 |
| ☐ | **Tree 1 Par 1** (U'King 36 LED par) | left tree, 1st from left | **7CH** | **141** | 141–147 |
| ☐ | **Tree 1 Par 2** | left tree, 2nd from left | **7CH** | **148** | 148–154 |
| ☐ | **Tree 1 Par 3** | left tree, 3rd from left | **7CH** | **155** | 155–161 |
| ☐ | **Tree 1 Par 4** | left tree, 4th from left | **7CH** | **162** | 162–168 |
| ☐ | **Tree 2 Par 1** | right tree, 1st from left | **7CH** | **169** | 169–175 |
| ☐ | **Tree 2 Par 2** | right tree, 2nd from left | **7CH** | **176** | 176–182 |
| ☐ | **Tree 2 Par 3** | right tree, 3rd from left | **7CH** | **183** | 183–189 |
| ☐ | **Tree 2 Par 4** | right tree, 4th from left | **7CH** | **190** | 190–196 |
| ☐ | **Mover 1** (Mini Gobo moving head) | on top of Tower 1 | **11CH** | **201** | 201–211 |
| ☐ | **Mover 2** | on top of Tower 2 | **11CH** | **212** | 212–222 |
| ☐ | **Mover 3** | on top of Tower 3 | **11CH** | **223** | 223–233 |
| ☐ | **Mover 4** | on top of Tower 4 | **11CH** | **234** | 234–244 |
| ☐ | **Solena 1** (Solena Max Bar 28) | floor at the back wall, left | **24CH** | **301** | 301–324 |
| ☐ | **Solena 2** (Solena Max Bar 28) | floor at the back wall, right | **24CH** | **325** | 325–348 |
| ☐ | **RockStrip 1** (Rockville RockStrip 252) | front, far left | **28CH** | **401** | 401–428 |
| ☐ | **RockStrip 2** | front, left of centre | **28CH** | **429** | 429–456 |
| ☐ | **RockStrip 3** | front, right of centre | **28CH** | **457** | 457–484 |
| ☐ | **RockStrip 4** | front, far right | **28CH** | **485** | 485–512 |

The jumps to 141, 201, 301 and 401 keep each group easy to remember.
RockStrip 4 ends exactly on channel 512, so the universe is full.

## Notes per light

- **ZQ01334 towers** (from U'King's manual, `ZQ01334.pdf`):
  - **Mode:** press MENU until the display shows `CHnd`, press ENTER, pick
    `33CH` with UP/DOWN, then press ENTER.
  - **Address:** press MENU until it shows `A001`, press ENTER, set
    `001` / `034` / `067` / `100` with UP/DOWN, then press ENTER.
  - **Other menu items:** `w255`, `r255`, `G255`, `b255` and `F000` are the
    light's own manual colour and strobe settings. Leave them alone, since
    myDMX takes over once it's addressed.
  - **33CH is the important one.** It's the only mode that gives each of the
    six lamps its own warm white and its own colour. Its 15-channel mode can't
    do that.
  - **A tower in the wrong mode shifts the next one.** 33 + 1 = 34, so if a
    tower isn't in 33CH, Tower 2 onward will be off too.
  - **Lamp numbering:** the manual only says "the first light" to "the
    sixth light", not which end lamp 1 is. Use the `TOWER n LAMPS` banks
    (below) to find out.
- **Tree pars:**
  - **Mode:** 7 channels is their DMX mode.
  - **Placement:** each tree is one row of four pars on its crossbar (1 × 4).
    Hang them Par 1 to Par 4 left to right, as seen from the audience.
    The order matters for chases that run across the trees.
  - **Who each par lights:**

    | Tree | Par 1 | Par 2 | Par 3 | Par 4 |
    |---|---|---|---|---|
    | Tree 1 | Bass | Drummer | Singer | Guitar 1 |
    | Tree 2 | Bass | Singer | Guitar 1 | Guitar 2 |
- **Mini Gobo movers** (the small 11-channel movers, the same model as at
  Riverside):
  - **Mode:** press MENU until the display shows `Chnd`, press ENTER, pick
    `11CH`, then press ENTER. In 9CH the fine pan/tilt channels drop out and
    every channel after them shifts.
  - **Address:** press MENU until it shows `Addr`, press ENTER, set
    `201` / `212` / `223` / `234`, then press ENTER.
  - **Sound control:** set `Soun` to `OFF`, so the movers only follow DMX.
  - **Startup:** they reset when powered up, so let them finish before testing.
- **Solena bars:** use the 24-channel mode, which gives 8 sections × RGB. Each
  section can be its own colour.
- **RockStrips:** use the 28-channel mode, which gives 8 sections you can
  colour separately. Rockville's chart isn't confirmed yet, so use the
  `ROCKSTRIP CHANNELS` bank (one button per channel) to check what each
  channel lights.

## Check it worked

1. Open `Mobile Lighting 33ch.dvc` in myDMX 5.
2. Go to the **TEST** bank. There is one button per light.
3. Press each button in turn. Only that light should come on.
   - If a **different** light comes on, its address is wrong.
   - If the right light comes on but looks **wrong** (part of it lit, or
     strobing or flickering), its mode is wrong.
   - If **nothing** comes on, check it's in DMX mode and that the cable reaches it.
4. What a correct light looks like on its TEST button:
   - **Tower:** all six warm whites on, and the six colour cells a rainbow
     (a different colour per lamp). One colour all the way up, or only the
     first lamps right, means it isn't in 33CH.
   - **Solena bar:** a rainbow across its 8 sections. One solid colour means
     it isn't in 24CH.
   - **Par:** full white.
   - **RockStrip:** a rainbow across its 8 sections.
   - **Mover:** centred, open white beam.
5. The last button, **ALL**, lights everything at once.

## Tower lamp banks

Each tower has its own bank, `TOWER 1 LAMPS` to `TOWER 4 LAMPS`:

- **Lamp buttons:** `L1 WARM`, `L1 RED`, `L1 GREEN`, `L1 BLUE` and so on up to
  `L6 BLUE`. Each lights only that lamp in only that colour.
- **Whole-tower buttons:** `ALL WARM`, `ALL RED`, `ALL GREEN` and `ALL BLUE`
  light every lamp in that colour.
- **`BAR RGB WHITE`:** the tower-wide colour channels (9–11) at full.

Press `L1 WARM` and note which end of the tower lights. That's lamp 1, and
it tells us which way up the lamps are numbered.

The full channel-by-channel layout of each light is in `ADDRESSES.md`.
