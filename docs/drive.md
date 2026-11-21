# Drive: design

A design note for **Drive**, the central system Studio and the other apps
use as their back end, written 2026-11-20. It lives here until Drive has
its own repository.

## What it is

One system for everything a person or an org keeps: tasks and projects,
calendars, contacts, notes, email, files and media, and the data of every
app built on it. Drive is a data platform, a service and an SDK; it is not
an app framework. Apps are separate products, each with its own repository
and UI, that embed `drive-client`; **no app deploys a server**.

The first apps: **Studio** (sets, songs, records), a **meal and cooking
tracker**, and an **agent wiki**.

It replaces Task (archived). It is built from the crates below directly,
not on Architect, whose ideas and code it borrows (see "Borrowed"). Where
Architect bundled the framework, the server, a UI shell and extensions,
Drive is only the data layer.

## Principles

1. **Local-first.** Every device holds the data it needs and works with no
   network. The data on the device is the truth; servers hold copies.
2. **Server-optional, server-friendly.** Devices sync with each other
   directly. A server makes things easier (always online, reachable from
   anywhere, backups, share links, search) but is never required, and
   never decides who wins an edit.
3. **One node, many roles.** The hosted cloud, a self-hosted server, a Mac
   mini at a church and a phone all run the same code. Roles are switched
   on, not built separately.
4. **Hosted by default, self-hostable everywhere.** A typical user signs
   up and everything works on our cloud. An org can add its own storage or
   servers, or run all of it itself, and everyone still works together.
5. **Anyone can be invited.** A share link opens in a browser with nothing
   installed and no account.
6. **A stable protocol before features.** Documents, blobs and the sync
   wire format are versioned and change rarely; apps and UI change often.
7. **Small, separable crates.** Each crate does one thing and can be used
   without the rest. No crate pulls in a UI shell, an extension model or a
   pinned UI framework. (The lesson of Task and Architect: coupling and
   churn became the debt.)

## Concepts

| Concept | What it is |
|---|---|
| **Account** | A person. Signs in through the central auth. Owns devices. |
| **Device key** | An iroh key pair per device or node. The account vouches for it. |
| **Org** | A group (a church, a band, a studio). Owns documents, nodes, storage. Accounts are members with roles. |
| **Document** | One Loro document: the unit of sync, sharing and permission. A task, a project, a set, a song, a record. |
| **Blob** | Bytes identified by their BLAKE3 hash: stems, mixes, PDFs, images, attachments. Documents hold hashes, never bytes. |
| **Node** | Anything running the system: a phone, a laptop, a VPS, a NAS. Has roles. |
| **Capability** | Signed permission: membership in an org, or a share link's scope and level. |
| **Link** | A reference from a document to another document, a blob, an email (by Message-ID) or an event. What ties the system together. |

## Documents

- **One Loro document per natural unit**, so sharing a thing shares exactly
  that document (and, read-only, what it references).
- **Typed.** Each document type has a schema (a Rust type over Loro's maps,
  movable lists, text and trees) with a **version number**. A document
  records its schema version; opening an older one runs that type's
  migration. Migrations only add or rename; data is never dropped.
- **Core types** the system owns: Task, Project, Note, Event, Contact,
  Folder, Thread (a link to an email conversation), Org, Membership.
- **App types** an app registers: Studio's Set, Song, Show, Series,
  Record, Track, Review. They use the same links, so a Set can link a Task
  ("book the drummer"), an Event (the service) and an email.
- **Tasks are one system.** Every app's to-dos are core Task documents, so
  there is one inbox and one list, whatever app made them.
- **Presence is not a document.** Who is here, playheads, "Mia is editing"
  ride Loro's ephemeral store, never stored.
- **Compaction.** Each document keeps a snapshot plus its recent updates;
  a node folds them into a new snapshot on a schedule.

## Storage

- **On devices and nodes: Turso** (the `turso` crate, local engine only;
  its sync is not used). Tables: document snapshots, update logs, and
  **projections**: plain tables an app queries (every set, open tasks,
  search). Projections are rebuilt from documents, never edited. Same file
  format as SQLite, so `rusqlite` is the fallback.
- **In the browser:** IndexedDB or OPFS for the same tables, until Turso's
  Rust crate runs in wasm.
- **Blobs: OpenDAL** behind every store: local disk, S3, R2, a NAS, and
  the rest of its 40+ services. A storage node is an OpenDAL operator
  plus placement rules.
- **Identity of bytes (Spacedrive's method):** a sampled BLAKE3 hash finds
  likely duplicates fast; the full BLAKE3 hash, checked before anything is
  trusted or deleted, is the blob's identity.
- **Transfer:** `iroh-blobs` between nodes (resumable, verified as it
  streams); short-lived signed URLs straight to S3 or a CDN for browsers
  and big files, checked against the hash on arrival.
- **Existing buckets stay put.** A storage node scans a bucket, hashes its
  objects and records hash → key, without moving anything.

## Sync and connectivity

- **Loro updates over iroh** (QUIC, direct when possible, relayed when
  not). The same exchange runs over a websocket for browsers and guests.
- **Discovery:**
  - same room: mDNS on the local network (`iroh-mdns-address-lookup`);
  - anywhere: the account directory on the hosted cloud ("where can
    Cody's devices be reached");
  - first contact: an invite ticket (QR code or link).
  The directory is never the only way to find a peer.
- **Relays:** our own `iroh-relay` on the cloud (n0's public relays as a
  fallback only).
- **Always-on peers** carry edits between devices that are never online
  together, and are the backup.

## Realtime

Multiplayer in the manner of Google Docs, Figma and Canva, on the same
documents that sync offline.

- **Edits stream live.** An open document pushes each Loro update the
  moment it is made, over a reliable ordered stream.
- **Presence** (cursors, selections, viewports, who is here, drag
  previews) rides Loro's ephemeral store over unreliable QUIC datagrams:
  never saved, only the latest matters.
- **Text:** Loro rich text with marks (formatting, links, comment anchors
  that survive concurrent edits); stable cursors; undo per person;
  history and named versions from Loro's time travel.
- **Canvases:** layers and groups as a Loro movable tree; each object's
  properties a map, last writer wins per property (Figma's model). While
  dragging, positions go out as presence; the final position is written
  on release. Each page or board is its own document.
- **Live timing is not a CRDT.** A session has a leader (Studio: the engine
  running the set) that streams position and tempo with clock sync to
  every device; Ableton Link carries tempo on the local network. Playing
  audio together live over the internet is out of scope.
- **Topology:** a few peers in a room connect directly; many peers, or
  any browser, meet at the nearest always-on node, which fans out.
- `drive-sync` adds live subscriptions, a presence channel per document,
  leader sessions, and the choice of hub.

## Nodes and roles

One binary, `drive-node`, headless, plus the same code embedded
in every app.

| Role | Does |
|---|---|
| Client | Edits; keeps what is opened or pinned (embedded in apps) |
| Always-on peer | Keeps full copies of the documents its rules name |
| Relay | Connects devices that cannot reach each other |
| Storage | Keeps blobs, through OpenDAL |
| Index | Search and cross-document lists |
| Web | Serves the web app and share pages (Dioxus fullstack) |
| Mail | Connects mail accounts (see Mail) |

**Registering a node:** run it, join with a ticket or by signing in,
pick roles and an owner (a person or an org). **Placement rules** say
what a node keeps: "every document in org X", "all media of record Y",
"only what I opened plus the next two weeks' sets".

## Identity, permissions, sharing

- **Accounts** through the central auth (OIDC). Self-hosters may later run
  their own issuer that orgs choose to trust; one directory to start.
- **Keys, not sessions, carry authority.** The auth signs "this device key
  belongs to this account". Orgs sign memberships ("this account is an
  editor of this org"). Every node checks signatures itself, so
  permissions hold offline. Every update is signed by its device key and
  rejected by peers when the key lacks the role.
- **Removing someone** takes effect on every node that hears of it; what
  they already synced, they keep; keys for new data rotate.
- **Share links** carry a capability: a document (and what it references,
  read-only), a level (view, comment, edit) and an expiry. The cloud serves
  the web app limited to that scope; a guest gets a temporary key tied to
  the link; their edits are accepted only within it. "Save to my account"
  turns a guest into a user and keeps their work.
- **End-to-end encryption** per org, optional: keys in the URL fragment for
  links; no server search or server-rendered share pages for that org.

## Tiers

| Tier | Runs where |
|---|---|
| Hosted | Our cloud is the always-on peer, storage, relay, index, web and mail roles, for many orgs (isolated, with quotas and billing). |
| Bring your own storage | An org registers its buckets and servers; placement rules send its data there. Our cloud keeps accounts, discovery and links. |
| Self-hosted | The org runs `drive-node` with every role, registered with our directory, so its people, hosted users and guests still collaborate. |

Multi-tenancy, quotas and billing are the only parts self-hosters do not
need.

## Mail, calendars, contacts

- **The system is a mail client.** Each account connects its mailboxes:
  IMAP (`async-imap`, `imap-proto`) or JMAP (`jmap-client`); reading with
  `mail-parser`; sending with `mail-builder` and `mail-send` (SMTP, DKIM);
  `mail-auth` for checks. All MIT or Apache-2.0, from Stalwart Labs.
- **The mail server is the source of truth for mail.** Nodes with the
  mail role keep a cache in Turso; documents hold links by Message-ID
  (an email filed to a project, turned into a task, about a set). Sending
  and filing are actions on the account, not document edits.
- **Calendars and contacts** are core documents; `calcard` reads and
  writes iCal and vCard. Each org publishes a **read-only calendar feed**
  (its sets, services, rehearsals) that any phone's calendar subscribes to,
  with no app installed.
- **Stalwart** is an optional companion for orgs that want their own
  domain's mail, calendars and contacts: self-hosters run its free edition
  beside `drive-node`; hosting it for many orgs needs its enterprise licence
  (multi-tenancy is enterprise-only). Avoid its AGPL `sieve-rs` and the
  third-party `stalwart` / `stalwart-lite` crates.

## The spine, the apps, the overview

Drive is meant to run a whole life and a local business without
becoming a super app. Three layers:

1. **The spine**, owned by Drive: Event, Task, Project, Person (contacts),
   Org, Note, File, Thread (email), Booking. One calendar, one task list,
   one set of projects, whatever app writes to them.
2. **Focused apps**, separate products (Studio, Meals, the agent wiki, a
   booking app): each owns its domain types and links them to the spine
   (a Set makes an Event and Tasks; a booking makes an Event and a
   Person).
3. **The overview**: the Drive app and agents. They read the spine across
   every app and space, show any app's document as a card, and open the
   focused app for real work.

**The rule against a super app:** the Drive app edits only the spine
(calendar, tasks, projects, people, inbox and mail, files, search,
activity, accounts and orgs). Domain work stays in its app.

**Cards.** Each app declares, as data, how its types show elsewhere:
title, subtitle, date, icon, status, colour, and where a tap goes (a deep
link, or its hosted web build). The overview renders them with no app
code.

**Projects** link anything from any app: a record in Studio, its sessions
(Events), client emails (Threads), invoices (Tasks), the band (People).

## Spaces and tenancy

- A **personal space** plus any number of **orgs** (a church, a studio
  business, a band). The overview merges them, with an org filter.
- **Roles per org:** owner, admin, member, guest, and **client**: a
  business's customers, who see only their own projects, sessions and
  bookings.
- Each org has its own placement rules, storage (hosted or its own),
  billing and quotas.
- **Permissions** are org × app × type × level ("an intern edits sessions,
  never sees invoices").

## Agents

- An **agent principal** is an account with a scoped token, acting for a
  person or an org (read every calendar; write tasks in one project).
- Same API as the Drive app, over HTTP and MCP: query the spine, read
  cards and documents, edit within grants.
- **Actions:** apps declare actions (`studio.plan_set`, `meals.plan_week`)
  with a parameter schema; simple ones are document edits Drive runs
  itself, others run where the app (or its agent client) runs.
- **Provenance:** every update is signed, so the activity feed says which
  agent did what, for whom, and any of it can be reviewed or undone.

## Bookings

Availability comes from the one calendar across spaces; a public booking
page is served by the web role to people with nothing installed (the share
link mechanism); a confirmed booking becomes an Event and a Person, in the
calendar, the client's email and its project.

## Apps on Drive

- **Namespaced types.** Each app registers its document types
  (`studio.Set`, `meals.Recipe`, `wiki.Page`) with their schemas and
  versions. Drive owns the shared ones: Task, Project, Event, Contact,
  Note, File, Folder.
- **One account across apps.** Signing in to one signs in to all; their
  data links freely (a meal plan makes Events, every app's to-dos are
  Tasks in one list, a wiki page links a song or a recipe).
- **Per-app permissions.** An app gets its own types plus the shared types
  the user grants it ("Meals wants your calendar"), so apps by other
  people can come later.
- **Declarative projections.** An app declares, as data, which fields of
  its types become which columns of its query tables; any node indexes
  them without the app's code. Wasm modules only if that ever falls short.
- **Hosted web builds.** An app uploads its static web build to Drive,
  which serves it on the app's own address for share links and the web.
- **Sync by app.** A device syncs the types of the apps installed on it,
  what links to them, and what is pinned.
- **Agents are clients.** Drive's API (HTTP and MCP) with scoped tokens,
  capabilities for programs, so agents read and write across apps within
  what they are granted.
- **A Drive app** (optional) browses everything: files, tasks, mail, every
  app's documents.

## What apps see

Every app uses one client crate, `drive-client`, embedded:

- `open::<Set>(id)` → a typed, live view of a document; edits are
  methods on it.
- `query(projection)` → rows from the local tables, live.
- `blobs` → fetch by hash (from wherever it is), add a file.
- `share(doc, level, expiry)` → a link.
- `identity` → the account, its orgs, its devices.
- A thin Dioxus layer turns these into signals and hooks; the client crate
  itself knows nothing of Dioxus.

## Crates

Small and separable, one workspace, each prefixed `drive-` (`drive-core`,
`drive-doc`, …):

| Crate | Holds | Depends on |
|---|---|---|
| `core` | ids, keys, signatures, capabilities, links | ed25519 / iroh keys |
| `doc` | typed Loro documents, schemas, versions, migrations | `loro`, `core` |
| `store` | Turso tables: snapshots, update logs, projections | `turso`, `doc` |
| `blob` | BLAKE3 (sampled + full), OpenDAL stores, placement | `opendal`, `blake3`, `iroh-blobs` |
| `sync` | document exchange over any transport | `doc`, `store` |
| `net` | iroh endpoint, discovery, relay config, websocket fallback | `iroh` |
| `auth` | account sign-in, device attestation, memberships | `core` |
| `mail` | mail accounts, cache, links | Stalwart's mail crates |
| `client` | what apps embed (the API above) | the crates above |
| `dioxus` | signals and hooks over `client` | `client`, `dioxus` |
| `node` | the headless binary and its roles (`drive-node`) | `client`, `axum` |

Rules: no crate depends on `node` or `dioxus`; `doc`, `core` and `client`
build for wasm; every crate has its own tests.

## Borrowed

- **Architect:** its Loro hub and peer-to-peer sync, presence and
  compaction; its blob store's design (pinning, garbage collection,
  fetching from several remotes); its share links; its permission checker.
  Read and lift code; no dependency. Possibly extract its sync layer into
  `sync` instead of writing it again.
- **Spacedrive (Apache-2.0 since 2026-03-24):** OpenDAL for storage, the
  sampled-then-full BLAKE3 method, iroh set-up, Specta for typed clients.
- **Stalwart Labs' crates** for mail and calendars.

## Build order

1. `core`, `doc`, `store`, `sync`, `net`: two devices sync one Studio Set
   over iroh on the local network. No server.
2. `drive-node` as an always-on peer plus relay, on the VPS.
3. Share links and the web role: a guest opens a set in a browser.
4. `blob`: OpenDAL stores, BLAKE3, iroh-blobs, bucket indexing.
5. `auth`: accounts, device attestation, memberships, node registry,
   placement rules.
6. Core types: Task, Project, Event, Contact; the one inbox.
7. `mail`: mail accounts, cache, links; the calendar feed.
8. Hosted cloud: tenants, quotas, billing.

## Open questions

1. Drive's repository, and a public name (beside Google Drive and
   Spacedrive it may need a qualifier).
2. The central auth: the existing fts-auth, or Kanidm or Rauthy.
3. Encryption: optional per org (as above), or on by default.
4. Whether to extract Architect's sync layer or write `sync` fresh.
5. Schema migrations: per-type functions (as above), or a declarative
   schema with generated migrations.
6. How much of Task's data model (tasks, projects, goals, the inbox) to
   carry over as core types.
