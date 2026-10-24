# ADR 0002: a rig is one doc; setlists, songs and parts point into it

- Status: accepted. Replaces ADR 0001's granularity (the measurements stand).
- Date: 2026-10-24
- Context: `docs/design/sync.md` (one model, Loro docs, the doc catalog).

## Question

Signal and Session are one app over one model now, synced as Loro docs
between every device and shared with other people. Where are the doc
boundaries, so that a change to a profile reaches every setlist, every
block and preset reference always resolves, and a setlist can be shared
with a band without sharing anyone's sounds?

## Decision

| doc | holds | lives in |
|---|---|---|
| `rig` | its blocks and modules, presets, patches and profiles | its owner's space |
| `song` | key, tempo, sections, chart and audio (by sha256) | the space it is shared in |
| `setlist` | entries in order (each with an id), per-set key, tempo, start | the space it is shared in |
| `part` | one person's sound per entry of one setlist: patch/profile references, overrides | that person's space |
| `session` | a project (`.session`), one doc per branch | its owner's space |

1. **A doc is the unit of atomic change, of sync and of sharing.** Loro
   orders and merges edits within a doc; between docs there is no
   transaction and no order. So what points at itself densely and changes
   together is one doc.
2. **A rig is one doc.** Presets name blocks by id, patches are presets with
   values, profiles select patches. As separate docs (ADR 0001), a patch
   could arrive before the rig change that made its block, or keep a block
   another device deleted. In one doc every reference resolves against the
   same state. The whole demo library is 60 KiB; one rig is a fraction.
3. **Nothing is copied across docs; everything is referenced by id.** A
   setlist entry names a song; a part names a patch or a profile of a rig
   (`rig/<uuid>` + patch id); a session track names a rig the same way.
   Updating a profile changes one doc, and everything that plays it
   resolves through the reference when it reads. Copies are what drift
   (ADR 0001's per-setlist layout lost an edit that way); with none there
   is nothing to reconcile.
4. **Shared and personal are separate docs.** The band shares the setlist
   and its songs; each player's sounds are their own `part` doc, keyed by
   setlist entry id, so a reorder of the shared set carries every part with
   it. A profile edit changes your sound in every set you play and nothing
   anybody else sees.
5. **Content never edited is content-addressed** (NAM captures, IRs, plugin
   states, charts, practice audio): a sha256 in the doc, the bytes stored
   once. A hash cannot drift.

## References across docs still dangle — so they degrade, never fail

Docs sync independently, so a part can name a patch a device has not
received yet, or one deleted elsewhere.

- **Deletes are tombstones.** A deleted patch, profile or song stays in its
  doc marked deleted: it still resolves (shown as deleted, restorable), and
  an offline device learns of the delete instead of keeping a ghost.
- **Resolution falls back:** the entry's override, then the part's patch,
  then the profile's default patch, then the library's default profile.
  The view says what it fell back to ("patch missing — playing All-Around").
- **Ids, never names or positions.** Stable ids for every entity and every
  list item that is referenced (`session-format`'s rule, all docs).

## Sessions use rigs as they are

A session track that carries a rig references the `rig` doc; opening the
session opens the rig doc beside it, so a preset chosen in Session is the
same preset the live rig plays, and an edit in either is one edit. A
session does not embed a rig.

## Sharing

For now the unit of sharing is a **space** (an org, or the personal
account's own): a gig or a band is an org, and inviting someone makes them
a member. Personal docs reference shared ones across spaces; doc ids are
global. Sharing one setlist without the rest of a space needs per-doc
grants, which the catalog leaves room for. When it comes, a share is a
closure: the setlist, the songs it lists and their blobs, so nobody gets a
set with holes in it.

## Consequences

- `features/signal/proto/src/doc` keeps its codec; its layouts become the
  docs above (a rig doc holds its patches and profiles; `library:settings`
  moves into the owner's space).
- The rigs' own perform files (`perform-store`'s `.styx`, the guitar rig's
  setlists) and `signal-storage`'s SQLite move onto these docs.
- The gig folder (`gig.json`, its charts and audio) becomes a setlist, its
  songs and their blobs, imported once.
