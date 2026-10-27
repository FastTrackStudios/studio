//! The library's data: what a song, an arrangement, a setlist, a part, an
//! album and a session are, and the few rules that read across them (how
//! long an arrangement plays, which sound a rig plays for an entry).
//!
//! Plain values, no I/O: a source (the fixture today, the synced docs
//! next) builds a [`Library`], and the screens read it.

use uuid::Uuid;

macro_rules! ids {
    ($($(#[$doc:meta])* $name:ident),* $(,)?) => {$(
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        pub struct $name(pub Uuid);
    )*};
}

ids! {
    /// A space a doc lives in: a person's own, or a band's.
    SpaceId,
    /// A song: the hub its arrangements, assets and presets hang off.
    SongId,
    /// One section of a song's material (Verse 1, Chorus, …).
    SectionId,
    /// One arrangement of a song.
    ArrangementId,
    /// A setlist.
    SetlistId,
    /// One entry of a setlist; parts key their sounds by it.
    EntryId,
    /// A rig attached to a setlist for one player.
    PartId,
    /// A rig (guitar, keys, …): its profiles, patches and song presets.
    RigId,
    /// An album.
    AlbumId,
    /// A session: a `.session`.
    SessionId,
    /// A project: Task's container for one undertaking.
    ProjectId,
}

/// A space: whose a doc is, and who sees it.
#[derive(Clone, PartialEq, Debug)]
pub struct Space {
    pub id: SpaceId,
    pub name: String,
    /// A band's space is shared with its members; a personal one is not.
    pub band: bool,
    /// Its members' names, for a band (the people a part can belong to).
    pub members: Vec<String>,
}

/// A musical key, as written on a chart (`A`, `Bb`, `F#m`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Key(pub String);

impl std::fmt::Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Beats per bar and the tempo they are counted at.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Time {
    pub bpm: f32,
    pub beats_per_bar: u8,
}

impl Time {
    /// How long `bars` bars play, in seconds.
    #[must_use]
    pub fn seconds(self, bars: u32) -> f32 {
        if self.bpm <= 0.0 {
            return 0.0;
        }
        bars as f32 * f32::from(self.beats_per_bar) * 60.0 / self.bpm
    }
}

/// One section of a song's material.
#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    pub id: SectionId,
    pub name: String,
    pub bars: u32,
}

/// Whether something an arrangement needs is on hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ready {
    /// There, and on this device.
    Here,
    /// There, but only in the cloud until it is downloaded.
    Cloud,
    /// Not made yet.
    Missing,
}

/// A step of an arrangement's form: a section, played `times` times.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Step {
    pub section: SectionId,
    pub times: u8,
}

/// One way to play a song: its own key, tempo and form over the song's
/// sections and assets, so a short live version reuses everything the full
/// one has.
#[derive(Clone, PartialEq, Debug)]
pub struct Arrangement {
    pub id: ArrangementId,
    pub name: String,
    pub key: Key,
    pub time: Time,
    pub form: Vec<Step>,
    pub chart: Ready,
    pub audio: Ready,
    /// The session it is produced in, if any.
    pub session: Option<SessionId>,
}

/// What kind of file a song keeps (by hash, once, for every arrangement).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssetKind {
    Stems,
    Mix,
    Click,
    Chart,
    Lyrics,
    Midi,
    Pdf,
}

/// One of a song's files.
#[derive(Clone, PartialEq, Debug)]
pub struct Asset {
    pub kind: AssetKind,
    pub name: String,
    pub bytes: u64,
    pub ready: Ready,
}

/// When a doc was last changed, and where.
#[derive(Clone, PartialEq, Debug)]
pub struct Edited {
    /// "2h", "Yesterday", "Oct 19": already said the way a list shows it.
    pub when: String,
    /// How long ago, in minutes: what "recently changed" sorts by.
    pub age: u32,
    pub by: String,
    pub device: String,
}

/// How a doc stands against the account's other devices.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sync {
    /// Up to date, and kept on this device for offline use.
    Offline,
    /// Up to date.
    Synced,
    /// Catching up: the fraction received.
    Syncing(f32),
    /// Known from the catalog; not on this device yet.
    Cloud,
}

/// A song: the catalog facts, its material, its arrangements, its files.
#[derive(Clone, PartialEq, Debug)]
pub struct Song {
    pub id: SongId,
    pub space: SpaceId,
    pub title: String,
    pub artist: String,
    pub writers: Vec<String>,
    pub ccli: Option<String>,
    pub tags: Vec<String>,
    /// The song's colour (a hex), so it is recognised before it is read.
    pub colour: &'static str,
    pub sections: Vec<Section>,
    pub arrangements: Vec<Arrangement>,
    pub assets: Vec<Asset>,
    pub edited: Edited,
    pub sync: Sync,
}

impl Song {
    #[must_use]
    pub fn arrangement(&self, id: ArrangementId) -> Option<&Arrangement> {
        self.arrangements.iter().find(|a| a.id == id)
    }

    #[must_use]
    pub fn section(&self, id: SectionId) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    /// The bars an arrangement plays: its form over this song's sections.
    /// A step naming a section the song no longer has counts nothing.
    #[must_use]
    pub fn bars(&self, arrangement: &Arrangement) -> u32 {
        arrangement
            .form
            .iter()
            .filter_map(|step| Some(self.section(step.section)?.bars * u32::from(step.times)))
            .sum()
    }

    /// How long an arrangement plays, in seconds.
    #[must_use]
    pub fn seconds(&self, arrangement: &Arrangement) -> f32 {
        arrangement.time.seconds(self.bars(arrangement))
    }
}

/// What a setlist entry plays that is not a song.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceKind {
    Transition,
    Medley,
    Spoken,
    Pad,
    CountIn,
}

impl PieceKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Transition => "Transition",
            Self::Medley => "Medley",
            Self::Spoken => "Spoken",
            Self::Pad => "Pad",
            Self::CountIn => "Count-in",
        }
    }
}

/// What one entry of a setlist plays.
#[derive(Clone, PartialEq, Debug)]
pub enum Piece {
    /// An arrangement of a song.
    Song {
        song: SongId,
        arrangement: ArrangementId,
    },
    /// Anything else a show plays: a transition, a prayer over a pad.
    Other {
        kind: PieceKind,
        title: String,
        seconds: f32,
    },
}

/// One entry of a setlist.
#[derive(Clone, PartialEq, Debug)]
pub struct Entry {
    pub id: EntryId,
    pub piece: Piece,
    /// This show's key, when it differs from the arrangement's.
    pub key: Option<Key>,
    /// This show's tempo, when it differs.
    pub bpm: Option<f32>,
    pub notes: String,
}

/// A setlist: what one show plays, in order.
#[derive(Clone, PartialEq, Debug)]
pub struct Setlist {
    pub id: SetlistId,
    pub space: SpaceId,
    pub title: String,
    /// "Sun, Oct 26 · 7:30 PM", already said the way the list shows it.
    pub when: Option<String>,
    pub venue: Option<String>,
    pub entries: Vec<Entry>,
    pub edited: Edited,
    pub sync: Sync,
    /// On tonight: the home leads with it.
    pub tonight: bool,
}

/// What a rig is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RigKind {
    Guitar,
    Bass,
    Keys,
    Drums,
    Vocals,
}

impl RigKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Guitar => "Guitar",
            Self::Bass => "Bass",
            Self::Keys => "Keys",
            Self::Drums => "Drums",
            Self::Vocals => "Vocals",
        }
    }
}

/// The sound a rig wants for a song, wherever it is played.
#[derive(Clone, PartialEq, Debug)]
pub struct SongPreset {
    pub song: SongId,
    /// Only for one arrangement; `None` is every arrangement of the song.
    pub arrangement: Option<ArrangementId>,
    pub patch: String,
}

/// A rig: its profiles' default, and the song presets it keeps.
#[derive(Clone, PartialEq, Debug)]
pub struct Rig {
    pub id: RigId,
    pub kind: RigKind,
    pub name: String,
    pub owner: String,
    pub profile: String,
    /// What the profile plays when nothing more specific is set.
    pub default_patch: String,
    pub patches: u32,
    pub presets: Vec<SongPreset>,
    pub sync: Sync,
}

impl Rig {
    /// This rig's preset for an arrangement of a song: the arrangement's
    /// own, else the song's.
    #[must_use]
    pub fn preset(&self, song: SongId, arrangement: ArrangementId) -> Option<&SongPreset> {
        let for_song = |p: &&SongPreset| p.song == song;
        self.presets
            .iter()
            .filter(for_song)
            .find(|p| p.arrangement == Some(arrangement))
            .or_else(|| {
                self.presets
                    .iter()
                    .filter(for_song)
                    .find(|p| p.arrangement.is_none())
            })
    }
}

/// A rig attached to a setlist for one player: the sound per entry, where
/// this show differs from the rig's song presets.
#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub id: PartId,
    pub setlist: SetlistId,
    pub rig: RigId,
    pub player: String,
    pub sounds: Vec<(EntryId, String)>,
    pub sync: Sync,
}

/// Where a played sound came from, most specific first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum From {
    /// Set for this entry of this setlist.
    Part,
    /// The rig's preset for the song (or that arrangement).
    SongPreset,
    /// Nothing set: the profile's default.
    Profile,
}

/// The sound an entry plays on a part's rig, and where it came from.
#[derive(Clone, PartialEq, Debug)]
pub struct Sound {
    pub patch: String,
    pub from: From,
}

/// The sound `entry` plays on `rig` for `part`: the part's own for the
/// entry, then the rig's preset for the song, then the profile's default
/// (ADR 0002's resolution order).
#[must_use]
pub fn sound(part: &Part, rig: &Rig, entry: &Entry) -> Sound {
    if let Some((_, patch)) = part.sounds.iter().find(|(id, _)| *id == entry.id) {
        return Sound {
            patch: patch.clone(),
            from: From::Part,
        };
    }
    if let Piece::Song { song, arrangement } = entry.piece
        && let Some(preset) = rig.preset(song, arrangement)
    {
        return Sound {
            patch: preset.patch.clone(),
            from: From::SongPreset,
        };
    }
    Sound {
        patch: rig.default_patch.clone(),
        from: From::Profile,
    }
}

/// Where a session is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Writing,
    Tracking,
    Editing,
    Mixing,
    Mastering,
    Released,
}

impl Stage {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Writing => "Writing",
            Self::Tracking => "Tracking",
            Self::Editing => "Editing",
            Self::Mixing => "Mixing",
            Self::Mastering => "Mastering",
            Self::Released => "Released",
        }
    }
}

/// A session: a `.session` where an arrangement is produced.
#[derive(Clone, PartialEq, Debug)]
pub struct Session {
    pub id: SessionId,
    pub space: SpaceId,
    pub title: String,
    pub song: Option<SongId>,
    pub stage: Stage,
    pub tracks: u32,
    pub edited: Edited,
    pub sync: Sync,
    /// Its mixes, oldest first: the Review page.
    pub mixes: Vec<MixVersion>,
    /// What was said at moments of them.
    pub notes: Vec<Note>,
    /// Its production steps and their subtasks: the Status page.
    pub steps: Vec<Todo>,
}

/// One track of an album.
#[derive(Clone, PartialEq, Debug)]
pub struct Track {
    pub song: SongId,
    pub arrangement: ArrangementId,
    pub session: Option<SessionId>,
}

/// An album: the studio's ordered list.
#[derive(Clone, PartialEq, Debug)]
pub struct Album {
    pub id: AlbumId,
    pub space: SpaceId,
    pub title: String,
    pub artist: String,
    pub year: u16,
    pub tracks: Vec<Track>,
    pub edited: Edited,
    pub sync: Sync,
}

/// What a project is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProjectKind {
    /// A show, a run of services, a tour: what is played live.
    Live,
    /// A record: sessions that become an album or a single.
    Recording,
    /// Anything else a band takes on.
    Other,
}

impl ProjectKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Live => "Live",
            Self::Recording => "Recording",
            Self::Other => "Project",
        }
    }
}

/// Where a project stands (Task's status).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Planning,
    Active,
    OnHold,
    Done,
}

impl Status {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Planning => "Planning",
            Self::Active => "Active",
            Self::OnHold => "On hold",
            Self::Done => "Done",
        }
    }
}

/// One of a project's tasks, as Task keeps it.
#[derive(Clone, PartialEq, Debug)]
pub struct Todo {
    pub title: String,
    pub done: bool,
    pub who: Option<String>,
    /// "Thu", "Nov 12": already said the way a list shows it.
    pub due: Option<String>,
    /// Its subtasks (Task's `workflow.parent`): Edit Drums, Comp Vocals
    /// under Edit.
    pub children: Vec<Todo>,
}

impl Todo {
    /// Done: ticked, or (with subtasks) every subtask done, as Task
    /// cascades it.
    #[must_use]
    pub fn is_done(&self) -> bool {
        if self.children.is_empty() {
            self.done
        } else {
            self.children.iter().all(Todo::is_done)
        }
    }

    /// (done, total) over the leaves: what a progress bar counts.
    #[must_use]
    pub fn progress(&self) -> (usize, usize) {
        if self.children.is_empty() {
            return (usize::from(self.done), 1);
        }
        self.children
            .iter()
            .map(Todo::progress)
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    }

    /// Tick or untick it, and every subtask with it.
    pub fn set_done(&mut self, done: bool) {
        self.done = done;
        for c in &mut self.children {
            c.set_done(done);
        }
    }
}

/// The production steps of a song, in order, each with its subtasks per
/// part: Record Drums, Edit Vocals, Comp Vocals. What the Status page
/// shows; Task keeps them as tasks with subtasks.
#[must_use]
pub fn production_steps(parts: &[&str]) -> Vec<Todo> {
    let leaf = |title: String| Todo {
        title,
        done: false,
        who: None,
        due: None,
        children: Vec::new(),
    };
    let per_part = |verb: &str| {
        parts
            .iter()
            .map(|p| leaf(format!("{verb} {p}")))
            .collect::<Vec<_>>()
    };
    let step = |title: &str, children: Vec<Todo>| Todo {
        title: title.into(),
        done: false,
        who: None,
        due: None,
        children,
    };
    let mut edit = per_part("Edit");
    if parts.contains(&"Vocals") {
        edit.push(leaf("Comp Vocals".into()));
    }
    vec![
        step("Write", vec![leaf("Lyrics".into()), leaf("Chart".into())]),
        step(
            "Produce",
            vec![leaf("Arrangement".into()), leaf("Demo".into())],
        ),
        step("Record", per_part("Record")),
        step("Edit", edit),
        step(
            "Organize",
            vec![
                leaf("Name and colour tracks".into()),
                leaf("Route to buses".into()),
            ],
        ),
        step(
            "Mix",
            vec![
                leaf("Rough mix".into()),
                leaf("Mix notes in".into()),
                leaf("Final mix".into()),
            ],
        ),
        step(
            "Master",
            vec![leaf("Master".into()), leaf("Deliverables".into())],
        ),
    ]
}

/// One rendered version of a session's mix.
#[derive(Clone, PartialEq, Debug)]
pub struct MixVersion {
    pub n: u32,
    /// What changed: "vocals up 1 dB".
    pub note: String,
    pub when: String,
    pub by: String,
    pub seconds: f32,
    /// The waveform's outline, 0..=1 per bin.
    pub peaks: Vec<f32>,
}

/// A comment at a moment of a mix, on the version it was heard on (Task's
/// timecoded review comment).
#[derive(Clone, PartialEq, Debug)]
pub struct Note {
    pub at: f32,
    pub version: u32,
    pub author: String,
    pub when: String,
    pub text: String,
    pub resolved: bool,
    /// Left by a client, through a review link.
    pub client: bool,
}

/// A project: Task's container for one undertaking (a run of services, a
/// show, a record), with its people and tasks, and the setlists, sessions
/// and album attached to it.
#[derive(Clone, PartialEq, Debug)]
pub struct Project {
    pub id: ProjectId,
    pub space: SpaceId,
    pub title: String,
    pub kind: ProjectKind,
    pub status: Status,
    pub when: Option<String>,
    pub venue: Option<String>,
    pub people: Vec<String>,
    pub todos: Vec<Todo>,
    pub setlists: Vec<SetlistId>,
    pub sessions: Vec<SessionId>,
    pub album: Option<AlbumId>,
    pub edited: Edited,
    pub sync: Sync,
}

/// What a comment is about: a doc, or a place inside one.
#[derive(Clone, PartialEq, Debug)]
pub enum Anchor {
    Song(SongId),
    Arrangement(SongId, ArrangementId),
    Setlist(SetlistId),
    Entry(SetlistId, EntryId),
    Album(AlbumId),
    Session(SessionId),
    Project(ProjectId),
}

/// One comment.
#[derive(Clone, PartialEq, Debug)]
pub struct Comment {
    pub author: String,
    pub when: String,
    pub text: String,
}

/// A conversation about one thing.
#[derive(Clone, PartialEq, Debug)]
pub struct Thread {
    pub anchor: Anchor,
    pub comments: Vec<Comment>,
    pub resolved: bool,
}

/// Everything the library shows: what one person can see, across their
/// own space and their bands'.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Library {
    /// The person reading it.
    pub me: String,
    pub spaces: Vec<Space>,
    pub songs: Vec<Song>,
    pub setlists: Vec<Setlist>,
    pub albums: Vec<Album>,
    pub sessions: Vec<Session>,
    pub projects: Vec<Project>,
    pub rigs: Vec<Rig>,
    pub parts: Vec<Part>,
    pub threads: Vec<Thread>,
}

impl Library {
    #[must_use]
    pub fn song(&self, id: SongId) -> Option<&Song> {
        self.songs.iter().find(|s| s.id == id)
    }
    #[must_use]
    pub fn setlist(&self, id: SetlistId) -> Option<&Setlist> {
        self.setlists.iter().find(|s| s.id == id)
    }
    #[must_use]
    pub fn album(&self, id: AlbumId) -> Option<&Album> {
        self.albums.iter().find(|a| a.id == id)
    }
    #[must_use]
    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|p| p.id == id)
    }
    #[must_use]
    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    /// The project a setlist is attached to, if any.
    #[must_use]
    pub fn project_of_setlist(&self, id: SetlistId) -> Option<&Project> {
        self.projects.iter().find(|p| p.setlists.contains(&id))
    }

    /// The project a session is attached to, if any.
    #[must_use]
    pub fn project_of_session(&self, id: SessionId) -> Option<&Project> {
        self.projects.iter().find(|p| p.sessions.contains(&id))
    }

    /// The project an album is attached to, if any.
    #[must_use]
    pub fn project_of_album(&self, id: AlbumId) -> Option<&Project> {
        self.projects.iter().find(|p| p.album == Some(id))
    }

    #[must_use]
    pub fn rig(&self, id: RigId) -> Option<&Rig> {
        self.rigs.iter().find(|r| r.id == id)
    }
    #[must_use]
    pub fn space(&self, id: SpaceId) -> Option<&Space> {
        self.spaces.iter().find(|s| s.id == id)
    }

    /// The parts attached to a setlist, mine first.
    #[must_use]
    pub fn parts_of(&self, setlist: SetlistId) -> Vec<&Part> {
        let mut parts: Vec<&Part> = self.parts.iter().filter(|p| p.setlist == setlist).collect();
        parts.sort_by_key(|p| p.player != self.me);
        parts
    }

    /// The setlists that play any arrangement of a song.
    #[must_use]
    pub fn setlists_with(&self, song: SongId) -> Vec<&Setlist> {
        self.setlists
            .iter()
            .filter(|s| {
                s.entries
                    .iter()
                    .any(|e| matches!(e.piece, Piece::Song { song: id, .. } if id == song))
            })
            .collect()
    }

    /// The albums a song is on.
    #[must_use]
    pub fn albums_with(&self, song: SongId) -> Vec<&Album> {
        self.albums
            .iter()
            .filter(|a| a.tracks.iter().any(|t| t.song == song))
            .collect()
    }

    /// The open threads about something, and about anything inside it.
    #[must_use]
    pub fn threads_on(&self, about: &Anchor) -> Vec<&Thread> {
        self.threads
            .iter()
            .filter(|t| !t.resolved && within(&t.anchor, about))
            .collect()
    }

    /// How long an entry plays, in seconds: its arrangement at this show's
    /// tempo, or what a non-song piece says.
    #[must_use]
    pub fn entry_seconds(&self, entry: &Entry) -> f32 {
        match &entry.piece {
            Piece::Other { seconds, .. } => *seconds,
            Piece::Song { song, arrangement } => {
                let Some(song) = self.song(*song) else {
                    return 0.0;
                };
                let Some(arr) = song.arrangement(*arrangement) else {
                    return 0.0;
                };
                let time = Time {
                    bpm: entry.bpm.unwrap_or(arr.time.bpm),
                    ..arr.time
                };
                time.seconds(song.bars(arr))
            }
        }
    }

    /// How long a setlist plays, in seconds.
    #[must_use]
    pub fn setlist_seconds(&self, setlist: &Setlist) -> f32 {
        setlist.entries.iter().map(|e| self.entry_seconds(e)).sum()
    }
}

/// Whether a thread about `anchor` belongs under `about`: the same thing,
/// or a place inside it.
fn within(anchor: &Anchor, about: &Anchor) -> bool {
    match (anchor, about) {
        (Anchor::Arrangement(s, _), Anchor::Song(t)) => s == t,
        (Anchor::Entry(s, _), Anchor::Setlist(t)) => s == t,
        _ => anchor == about,
    }
}

/// A change a person makes in the library. The screens make every change
/// through one of these, so the source behind them (the fixture, the synced
/// docs) applies them the same way: [`Library::apply`] here, a doc
/// transaction there.
#[derive(Clone, PartialEq, Debug)]
pub enum Edit {
    /// Say something about a thing (a new thread, or the open one).
    Comment {
        about: Anchor,
        author: String,
        text: String,
    },
    /// Tick a project's task, or untick it.
    ToggleTodo { project: ProjectId, index: usize },
    /// Attach a rig to a setlist for a player: a new part.
    AttachRig {
        setlist: SetlistId,
        rig: RigId,
        player: String,
        part: PartId,
    },
    /// Move an entry to another place in its setlist.
    MoveEntry {
        setlist: SetlistId,
        entry: EntryId,
        to: usize,
    },
    /// Take an entry out of a setlist.
    RemoveEntry { setlist: SetlistId, entry: EntryId },
    /// Tick a session's production step, or one of its subtasks (`sub`),
    /// or untick it. A step ticked ticks its subtasks.
    ToggleStep {
        session: SessionId,
        step: usize,
        sub: Option<usize>,
    },
    /// Say something at a moment of a mix version.
    MixNote {
        session: SessionId,
        version: u32,
        at: f32,
        author: String,
        text: String,
    },
    /// Mark a mix note dealt with, or not.
    ResolveNote { session: SessionId, index: usize },
    /// Add an arrangement of a song to the end of a setlist.
    AddSong {
        setlist: SetlistId,
        entry: EntryId,
        song: SongId,
        arrangement: ArrangementId,
    },
}

/// Why an edit could not be made: what it names is not there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditError {
    NoSuchThing,
}

impl Library {
    /// Make an edit. An edit naming something that is not there changes
    /// nothing.
    pub fn apply(&mut self, edit: Edit) -> Result<(), EditError> {
        match edit {
            Edit::Comment {
                about,
                author,
                text,
            } => {
                let comment = Comment {
                    author,
                    when: "now".into(),
                    text,
                };
                match self
                    .threads
                    .iter_mut()
                    .find(|t| !t.resolved && t.anchor == about)
                {
                    Some(t) => t.comments.push(comment),
                    None => self.threads.push(Thread {
                        anchor: about,
                        comments: vec![comment],
                        resolved: false,
                    }),
                }
            }
            Edit::ToggleTodo { project, index } => {
                let todo = self
                    .projects
                    .iter_mut()
                    .find(|p| p.id == project)
                    .and_then(|p| p.todos.get_mut(index))
                    .ok_or(EditError::NoSuchThing)?;
                todo.done = !todo.done;
            }
            Edit::AttachRig {
                setlist,
                rig,
                player,
                part,
            } => {
                if self.setlist(setlist).is_none() || self.rig(rig).is_none() {
                    return Err(EditError::NoSuchThing);
                }
                // One part per rig per player per setlist.
                if !self
                    .parts
                    .iter()
                    .any(|p| p.setlist == setlist && p.rig == rig && p.player == player)
                {
                    self.parts.push(Part {
                        id: part,
                        setlist,
                        rig,
                        player,
                        sounds: Vec::new(),
                        sync: Sync::Synced,
                    });
                }
            }
            Edit::MoveEntry { setlist, entry, to } => {
                let set = self
                    .setlists
                    .iter_mut()
                    .find(|s| s.id == setlist)
                    .ok_or(EditError::NoSuchThing)?;
                let from = set
                    .entries
                    .iter()
                    .position(|e| e.id == entry)
                    .ok_or(EditError::NoSuchThing)?;
                let e = set.entries.remove(from);
                let to = to.min(set.entries.len());
                set.entries.insert(to, e);
            }
            Edit::RemoveEntry { setlist, entry } => {
                let set = self
                    .setlists
                    .iter_mut()
                    .find(|s| s.id == setlist)
                    .ok_or(EditError::NoSuchThing)?;
                let before = set.entries.len();
                set.entries.retain(|e| e.id != entry);
                if set.entries.len() == before {
                    return Err(EditError::NoSuchThing);
                }
                // What the parts kept for it goes with it.
                for p in self.parts.iter_mut().filter(|p| p.setlist == setlist) {
                    p.sounds.retain(|(id, _)| *id != entry);
                }
            }
            Edit::ToggleStep { session, step, sub } => {
                let s = self
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session)
                    .ok_or(EditError::NoSuchThing)?;
                let step = s.steps.get_mut(step).ok_or(EditError::NoSuchThing)?;
                match sub {
                    Some(i) => {
                        let t = step.children.get_mut(i).ok_or(EditError::NoSuchThing)?;
                        let done = !t.is_done();
                        t.set_done(done);
                    }
                    None => {
                        let done = !step.is_done();
                        step.set_done(done);
                    }
                }
            }
            Edit::MixNote {
                session,
                version,
                at,
                author,
                text,
            } => {
                let s = self
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session)
                    .ok_or(EditError::NoSuchThing)?;
                if !s.mixes.iter().any(|m| m.n == version) {
                    return Err(EditError::NoSuchThing);
                }
                s.notes.push(Note {
                    at,
                    version,
                    author,
                    when: "now".into(),
                    text,
                    resolved: false,
                    client: false,
                });
                s.notes.sort_by(|a, b| a.at.total_cmp(&b.at));
            }
            Edit::ResolveNote { session, index } => {
                let s = self
                    .sessions
                    .iter_mut()
                    .find(|s| s.id == session)
                    .ok_or(EditError::NoSuchThing)?;
                let n = s.notes.get_mut(index).ok_or(EditError::NoSuchThing)?;
                n.resolved = !n.resolved;
            }
            Edit::AddSong {
                setlist,
                entry,
                song,
                arrangement,
            } => {
                if self
                    .song(song)
                    .and_then(|s| s.arrangement(arrangement))
                    .is_none()
                {
                    return Err(EditError::NoSuchThing);
                }
                let set = self
                    .setlists
                    .iter_mut()
                    .find(|s| s.id == setlist)
                    .ok_or(EditError::NoSuchThing)?;
                set.entries.push(Entry {
                    id: entry,
                    piece: Piece::Song { song, arrangement },
                    key: None,
                    bpm: None,
                    notes: String::new(),
                });
            }
        }
        Ok(())
    }

    /// What matches a search, across kinds: (kind, title, detail, what
    /// it is). Case-blind, on titles, artists and venues.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<Found> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let hit = |s: &str| s.to_lowercase().contains(&q);
        let mut found = Vec::new();
        for p in self
            .projects
            .iter()
            .filter(|p| hit(&p.title) || p.venue.as_deref().is_some_and(hit))
        {
            found.push(Found::Project(p.id));
        }
        for s in self
            .setlists
            .iter()
            .filter(|s| hit(&s.title) || s.venue.as_deref().is_some_and(hit))
        {
            found.push(Found::Setlist(s.id));
        }
        for s in self
            .songs
            .iter()
            .filter(|s| hit(&s.title) || hit(&s.artist))
        {
            found.push(Found::Song(s.id));
        }
        for s in self.sessions.iter().filter(|s| hit(&s.title)) {
            found.push(Found::Session(s.id));
        }
        found
    }
}

/// One search result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Found {
    Project(ProjectId),
    Setlist(SetlistId),
    Song(SongId),
    Session(SessionId),
}

/// How well `query` matches `text`, fuzzily: every character of the
/// query in order (case-blind), scored higher for matches at word starts
/// and in runs, lower for gaps. `None` when it does not match at all.
/// What the open finder and the command palette rank by.
#[must_use]
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (ti, c) in t.iter().enumerate() {
        if qi < q.len() && *c == q[qi] {
            let word_start = ti == 0 || !t[ti - 1].is_alphanumeric();
            score += 10;
            if word_start {
                score += 8;
            }
            match last {
                Some(l) if l + 1 == ti => score += 6,
                Some(l) => score -= ((ti - l - 1) as i32).min(5),
                None => score -= (ti as i32).min(5),
            }
            last = Some(ti);
            qi += 1;
        }
    }
    (qi == q.len()).then_some(score)
}

/// `3:48`, `1:02:10`: seconds said as a clock.
#[must_use]
pub fn clock(seconds: f32) -> String {
    let total = seconds.max(0.0).round() as u64;
    let (h, m, s) = (total / 3600, total / 60 % 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `41 min`, `1 h 12 min`: a setlist's length, the way a band says it.
#[must_use]
pub fn minutes(seconds: f32) -> String {
    let m = (seconds.max(0.0) / 60.0).round() as u64;
    if m >= 60 {
        format!("{} h {} min", m / 60, m % 60)
    } else {
        format!("{m} min")
    }
}

/// One thing the player plays: a version of a session's mix. A song is
/// heard through the session that produces it, an album through its
/// tracks' sessions, a setlist through its songs'.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Listen {
    pub session: SessionId,
    pub version: u32,
}

impl Library {
    /// The session's newest mix.
    #[must_use]
    pub fn latest_mix(&self, session: SessionId) -> Option<Listen> {
        let s = self.session(session)?;
        s.mixes.last().map(|m| Listen {
            session,
            version: m.n,
        })
    }

    /// The session and mix a listen is of.
    #[must_use]
    pub fn mix(&self, listen: Listen) -> Option<(&Session, &MixVersion)> {
        let s = self.session(listen.session)?;
        Some((s, s.mixes.iter().find(|m| m.n == listen.version)?))
    }

    /// A song's newest mix: of the session furthest along that produces it.
    #[must_use]
    pub fn song_mix(&self, song: SongId) -> Option<Listen> {
        self.sessions
            .iter()
            .filter(|s| s.song == Some(song) && !s.mixes.is_empty())
            .max_by_key(|s| s.mixes.len())
            .and_then(|s| self.latest_mix(s.id))
    }

    /// An album's tracks that can be heard, in order.
    #[must_use]
    pub fn album_queue(&self, album: AlbumId) -> Vec<Listen> {
        self.album(album).map_or_else(Vec::new, |a| {
            a.tracks
                .iter()
                .filter_map(|t| t.session.and_then(|s| self.latest_mix(s)))
                .collect()
        })
    }

    /// A setlist's songs that can be heard, in the show's order.
    #[must_use]
    pub fn setlist_queue(&self, setlist: SetlistId) -> Vec<Listen> {
        self.setlist(setlist).map_or_else(Vec::new, |s| {
            s.entries
                .iter()
                .filter_map(|e| match e.piece {
                    Piece::Song { song, .. } => self.song_mix(song),
                    Piece::Other { .. } => None,
                })
                .collect()
        })
    }

    /// The newest mixes across the library, newest first: what the home
    /// offers to play.
    #[must_use]
    pub fn new_mixes(&self) -> Vec<Listen> {
        let mut heard: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|s| !s.mixes.is_empty())
            .collect();
        heard.sort_by_key(|s| s.edited.age);
        heard
            .into_iter()
            .filter_map(|s| self.latest_mix(s.id))
            .collect()
    }

    /// What a listen is called and by whom: the song's title and artist
    /// (the session's title when it has no song), and its colour.
    #[must_use]
    pub fn listen_label(&self, listen: Listen) -> Option<(String, String, &'static str)> {
        let s = self.session(listen.session)?;
        let song = s.song.and_then(|id| self.song(id));
        Some(match song {
            Some(song) => (s.title.clone(), song.artist.clone(), song.colour),
            None => (s.title.clone(), String::new(), "#52525b"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn song_with_two_arrangements() -> (Song, ArrangementId, ArrangementId) {
        let (verse, chorus) = (SectionId(id()), SectionId(id()));
        let time = Time {
            bpm: 120.0,
            beats_per_bar: 4,
        };
        let full = ArrangementId(id());
        let short = ArrangementId(id());
        let arrangement = |id, form: Vec<Step>| Arrangement {
            id,
            name: String::new(),
            key: Key("A".into()),
            time,
            form,
            chart: Ready::Here,
            audio: Ready::Here,
            session: None,
        };
        let song = Song {
            id: SongId(id()),
            space: SpaceId(id()),
            title: "Song".into(),
            artist: String::new(),
            writers: vec![],
            ccli: None,
            tags: vec![],
            colour: "#fff",
            sections: vec![
                Section {
                    id: verse,
                    name: "Verse".into(),
                    bars: 8,
                },
                Section {
                    id: chorus,
                    name: "Chorus".into(),
                    bars: 8,
                },
            ],
            arrangements: vec![
                arrangement(
                    full,
                    vec![
                        Step {
                            section: verse,
                            times: 2,
                        },
                        Step {
                            section: chorus,
                            times: 2,
                        },
                    ],
                ),
                arrangement(
                    short,
                    vec![Step {
                        section: chorus,
                        times: 1,
                    }],
                ),
            ],
            assets: vec![],
            edited: Edited {
                when: String::new(),
                age: 0,
                by: String::new(),
                device: String::new(),
            },
            sync: Sync::Synced,
        };
        (song, full, short)
    }

    #[test]
    fn an_arrangement_plays_its_form_over_the_songs_sections() {
        let (song, full, short) = song_with_two_arrangements();
        // 32 bars of 4/4 at 120: 64 seconds. The short one shares the
        // chorus and plays it once: 16 seconds.
        assert_eq!(song.bars(song.arrangement(full).unwrap()), 32);
        assert!((song.seconds(song.arrangement(full).unwrap()) - 64.0).abs() < 1e-3);
        assert!((song.seconds(song.arrangement(short).unwrap()) - 16.0).abs() < 1e-3);
    }

    #[test]
    fn a_step_naming_a_section_the_song_lost_counts_nothing() {
        let (mut song, full, _) = song_with_two_arrangements();
        song.sections.remove(0);
        assert_eq!(song.bars(song.arrangement(full).unwrap()), 16);
    }

    #[test]
    fn the_sound_falls_back_from_the_part_to_the_song_preset_to_the_profile() {
        let (song, full, short) = song_with_two_arrangements();
        let setlist = SetlistId(id());
        let entry = |arrangement| Entry {
            id: EntryId(id()),
            piece: Piece::Song {
                song: song.id,
                arrangement,
            },
            key: None,
            bpm: None,
            notes: String::new(),
        };
        let (set_here, preset_here, nothing) = (
            entry(full),
            entry(short),
            Entry {
                piece: Piece::Other {
                    kind: PieceKind::Pad,
                    title: "Prayer".into(),
                    seconds: 60.0,
                },
                ..entry(full)
            },
        );
        let rig = Rig {
            id: RigId(id()),
            kind: RigKind::Guitar,
            name: "Guitar".into(),
            owner: "me".into(),
            profile: "Worship".into(),
            default_patch: "All-Around".into(),
            patches: 3,
            presets: vec![
                SongPreset {
                    song: song.id,
                    arrangement: None,
                    patch: "Crunch".into(),
                },
                SongPreset {
                    song: song.id,
                    arrangement: Some(short),
                    patch: "Ambient".into(),
                },
            ],
            sync: Sync::Synced,
        };
        let part = Part {
            id: PartId(id()),
            setlist,
            rig: rig.id,
            player: "me".into(),
            sounds: vec![(set_here.id, "Lead".into())],
            sync: Sync::Synced,
        };

        assert_eq!(
            sound(&part, &rig, &set_here),
            Sound {
                patch: "Lead".into(),
                from: From::Part
            }
        );
        // The arrangement's own preset beats the song's.
        assert_eq!(
            sound(&part, &rig, &preset_here),
            Sound {
                patch: "Ambient".into(),
                from: From::SongPreset
            }
        );
        assert_eq!(
            sound(&part, &rig, &nothing),
            Sound {
                patch: "All-Around".into(),
                from: From::Profile
            }
        );
    }

    #[test]
    fn a_show_at_another_tempo_plays_its_arrangement_at_that_tempo() {
        let (song, full, _) = song_with_two_arrangements();
        let entry = Entry {
            id: EntryId(id()),
            piece: Piece::Song {
                song: song.id,
                arrangement: full,
            },
            key: None,
            bpm: Some(60.0),
            notes: String::new(),
        };
        let library = Library {
            songs: vec![song],
            ..Library::default()
        };
        assert!((library.entry_seconds(&entry) - 128.0).abs() < 1e-3);
    }

    #[test]
    fn lengths_read_the_way_a_band_says_them() {
        assert_eq!(clock(228.0), "3:48");
        assert_eq!(clock(3730.0), "1:02:10");
        assert_eq!(minutes(2460.0), "41 min");
        assert_eq!(minutes(4320.0), "1 h 12 min");
    }
    #[test]
    fn edits_reorder_attach_and_remove_and_refuse_what_is_not_there() {
        let mut lib = crate::fixture::library();
        let set = lib.setlists[0].id;
        let first = lib.setlists[0].entries[0].id;
        let n = lib.setlists[0].entries.len();
        lib.apply(Edit::MoveEntry {
            setlist: set,
            entry: first,
            to: 99,
        })
        .unwrap();
        assert_eq!(
            lib.setlists[0].entries.last().unwrap().id,
            first,
            "moved past the end lands last"
        );
        lib.apply(Edit::RemoveEntry {
            setlist: set,
            entry: first,
        })
        .unwrap();
        assert_eq!(lib.setlists[0].entries.len(), n - 1);
        assert_eq!(
            lib.apply(Edit::RemoveEntry {
                setlist: set,
                entry: first
            }),
            Err(EditError::NoSuchThing)
        );

        let rig = lib.rigs[0].id;
        let parts = lib.parts.len();
        let attach = Edit::AttachRig {
            setlist: lib.setlists[2].id,
            rig,
            player: "Cody".into(),
            part: PartId(id()),
        };
        lib.apply(attach.clone()).unwrap();
        lib.apply(attach).unwrap();
        assert_eq!(
            lib.parts.len(),
            parts + 1,
            "one part per rig per player per set"
        );
    }

    #[test]
    fn a_comment_joins_the_open_thread_or_starts_one() {
        let mut lib = crate::fixture::library();
        let about = Anchor::Setlist(lib.setlists[2].id);
        let threads = lib.threads.len();
        for text in ["first", "second"] {
            lib.apply(Edit::Comment {
                about: about.clone(),
                author: "Cody".into(),
                text: text.into(),
            })
            .unwrap();
        }
        assert_eq!(lib.threads.len(), threads + 1);
        assert_eq!(lib.threads_on(&about)[0].comments.len(), 2);
    }

    #[test]
    fn search_finds_across_kinds_whatever_the_case() {
        let lib = crate::fixture::library();
        let found = lib.search("paper");
        assert!(found.iter().any(|f| matches!(f, Found::Song(_))));
        assert!(found.iter().any(|f| matches!(f, Found::Session(_))));
        assert!(found.iter().any(|f| matches!(f, Found::Project(_))));
        assert!(lib.search("  ").is_empty());
    }
    #[test]
    fn fuzzy_matches_in_order_and_prefers_word_starts_and_runs() {
        assert!(fuzzy("snw", "Sunday Night Worship").is_some());
        assert!(
            fuzzy("wns", "Sunday Night Worship").is_none(),
            "order matters"
        );
        assert!(fuzzy("", "anything") == Some(0));
        // A run at a word start beats letters scattered through a word.
        let tight = fuzzy("pap", "Paper Boats").unwrap();
        let loose = fuzzy("pap", "Pineapple Party").unwrap();
        assert!(tight > loose, "{tight} vs {loose}");
        assert!(fuzzy("PAPER", "paper boats").is_some(), "case-blind");
    }
    #[test]
    fn a_step_is_done_when_its_subtasks_are_and_ticking_it_ticks_them() {
        let mut lib = crate::fixture::library();
        let id = lib.sessions[0].id;
        let edit = lib.sessions[0]
            .steps
            .iter()
            .position(|s| s.title == "Edit")
            .unwrap();
        let subs = lib.sessions[0].steps[edit].children.len();
        for i in 0..subs {
            if !lib.sessions[0].steps[edit].children[i].is_done() {
                lib.apply(Edit::ToggleStep {
                    session: id,
                    step: edit,
                    sub: Some(i),
                })
                .unwrap();
            }
        }
        assert!(
            lib.sessions[0].steps[edit].is_done(),
            "the last subtask done finishes the step"
        );
        lib.apply(Edit::ToggleStep {
            session: id,
            step: edit,
            sub: None,
        })
        .unwrap();
        assert_eq!(
            lib.sessions[0].steps[edit].progress().0,
            0,
            "unticking the step unticks its subtasks"
        );
    }

    #[test]
    fn a_mix_note_lands_in_time_order_on_a_version_that_exists() {
        let mut lib = crate::fixture::library();
        let id = lib.sessions[0].id;
        let v = lib.sessions[0].mixes.last().unwrap().n;
        lib.apply(Edit::MixNote {
            session: id,
            version: v,
            at: 1.0,
            author: "Cody".into(),
            text: "early".into(),
        })
        .unwrap();
        assert_eq!(lib.sessions[0].notes[0].text, "early");
        assert_eq!(
            lib.apply(Edit::MixNote {
                session: id,
                version: 999,
                at: 1.0,
                author: "Cody".into(),
                text: "x".into()
            }),
            Err(EditError::NoSuchThing)
        );
    }

    #[test]
    fn the_player_hears_albums_setlists_and_songs_through_their_sessions() {
        let lib = crate::fixture::library();
        let album = lib.albums[0].id;
        let queue = lib.album_queue(album);
        assert_eq!(queue.len(), 4, "the four tracks with mixes");
        for listen in &queue {
            let (session, mix) = lib.mix(*listen).expect("a real mix");
            assert_eq!(mix.n, session.mixes.last().unwrap().n, "the newest version");
        }
        let newest = lib.new_mixes();
        assert_eq!(newest.len(), 4);
        let ages: Vec<u32> = newest
            .iter()
            .map(|l| lib.session(l.session).unwrap().edited.age)
            .collect();
        assert!(ages.windows(2).all(|w| w[0] <= w[1]), "newest first");
        // Paper Boats has two sessions; the one with mixes is the one heard.
        let boats = lib
            .songs
            .iter()
            .find(|s| s.title == "Paper Boats")
            .unwrap()
            .id;
        let heard = lib.song_mix(boats).unwrap();
        assert_eq!(lib.session(heard.session).unwrap().title, "Paper Boats");
    }
}
