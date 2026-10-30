//! Recording the lanes again after a change, from what was recorded
//! before: [`Arrangement::rebuild_hiding`].
//!
//! A recording keeps what each row's commands were drawn from ([`Kept`]):
//! a fingerprint of the row as a whole, and the row cut into pieces of
//! timeline ([`CHUNK_SECS`] apart, never through a run of slices drawn as
//! one) each keyed by everything its items draw from. Recording again, a
//! row whose fingerprint is unchanged is taken whole from the last
//! recording; a row that changed takes the pieces whose keys match and
//! records only the rest. An edit that moves three slices of a kit
//! re-records the piece or two they sit in, not the song.
//!
//! What is taken is moved, not copied: the old recording is consumed.
//! What a full recording would draw, this draws — the keys cover every
//! input a piece is drawn from; the tests hold a rebuild to a full build
//! command for command.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::Arc;

use anyrender::recording::RenderCommand;
use anyrender::{PaintScene, Scene};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use daw_proto::{Item, Track};
use daw_ui::studio::project::Project;

use super::{
    Arrangement, DIVIDER, FADE_SHADE, FOLDER_BODY, Fades, Heights, Held, Index, ItemBox, LaneRole,
    LaneWave, Lettering, MixKey, Mixed, Mixes, Palette, Part, SliceRuns, TAKE_BODY, Title,
    body_alpha, chart_marks, command_index, folded_contents, gain_for, item_color, joined_wave,
    keys_of, kit_color, kit_roles, loudest_of, midi_preview, note_lane_take, record_panel,
    remembered_loudest, row_lanes, seamless_runs, source_of, spell, take_color, take_name,
    wave_gain,
};
use timeline::rows::RowLayout;

/// How far apart a row's pieces are cut, in seconds of timeline: an item
/// starting in another such step starts another piece.
///
/// Small enough that an edit re-records a handful of a kit's slices, big
/// enough that a song's rows are a few thousand pieces, not one an item.
pub(super) const CHUNK_SECS: f64 = 4.0;

/// What everything was recorded at, whatever the row: a change to any of
/// it records every row again.
#[derive(Clone, PartialEq)]
struct Basis {
    palette: Palette,
    tcp: crate::tcp::Tcp,
    layout: crate::layout::Layout,
    /// The song's length (bits): every row's stripe reaches it.
    length: u64,
    /// The items' body alpha (bits): a view option.
    alpha: u32,
    /// The song's keys, which a chord's title is spelled in.
    keys: u64,
    /// The tracks as the drum roles, hidden tracks and folders read them.
    tracks: u64,
    /// The takes kept out of sight.
    out_of_sight: u64,
}

/// What a recording keeps to record the next from.
#[derive(Default)]
pub(super) struct Kept {
    basis: Option<Basis>,
    /// The previews' generation it was recorded at: moved, some waveform
    /// or notes arrived and no row is taken whole.
    generation: u64,
    rows: Vec<KeptRow>,
    /// The shut folders' spans as mixed, for the next to keep.
    mixes: HashMap<MixKey, std::rc::Rc<Mixed>>,
    /// What was read for the rows, kept to read only what changed.
    reads: Reads,
    /// The panel's rows and heights as cut, so an unchanged panel is not
    /// cut again.
    panel: Option<PanelKept>,
}

/// What a recording read, kept for the next to read only what changed.
#[derive(Default)]
struct Reads {
    loudest: Loudest,
    /// The shut folders' children as read, by folder guid.
    children: FxHashMap<String, KeptChildren>,
    /// Each track's takes, longest first-seen, by the drawn version of its
    /// items they were read at: a kit row's take colours are its twenty
    /// tracks' takes, and only an edited track's are read again.
    takes: FxHashMap<String, (u64, Vec<(String, f64)>)>,
    /// Each track's version as the last recording drew it
    /// ([`track_hashes`]): what a waveform taken from it is stamped again
    /// with only if moved.
    versions: FxHashMap<String, u64>,
}

#[cfg(test)]
impl Kept {
    /// Each row's fingerprint, in order.
    pub(super) fn rows_whole(&self) -> Vec<u64> {
        self.rows.iter().map(|row| row.whole).collect()
    }

    /// Row `row`'s pieces' keys, in order.
    pub(super) fn chunk_keys(&self, row: usize) -> Vec<u64> {
        self.rows
            .get(row)
            .map(|kept| kept.chunks.iter().map(|chunk| chunk.key).collect())
            .unwrap_or_default()
    }
}

/// The panel as last cut: what it was cut from.
struct PanelKept {
    rows: Vec<(Track, u32)>,
    track_h: Vec<f64>,
    head_h: Vec<f64>,
    tcp: crate::tcp::Tcp,
    palette: Palette,
    skin: Option<Arc<frame::theme::runtime::Theme>>,
    /// The face the names are set in, by its data's id.
    font: u64,
}

/// One row as recorded.
struct KeptRow {
    guid: String,
    /// Everything the row is drawn from, as one number: unchanged, the
    /// row is taken whole.
    whole: u64,
    commands: Range<u32>,
    boxes: Range<usize>,
    titles: Range<usize>,
    chunks: Vec<KeptChunk>,
}

/// One piece of a row as recorded.
struct KeptChunk {
    key: u64,
    commands: Range<u32>,
    boxes: Range<usize>,
    titles: Range<usize>,
    /// The spans it mixed, carried to the next recording with it.
    mixes: Vec<MixKey>,
    /// Its items' lanes and takes, as [`note_lane_take`] saw them.
    lane_takes: Vec<(u32, Option<usize>)>,
    /// The waveforms its key was made from, kept alive so a later
    /// waveform cannot be allocated where one of them was and pass for it.
    #[expect(dead_code, reason = "held to keep the waveforms alive, never read")]
    held: Vec<Arc<crate::midi::Wave>>,
}

/// The old recording, its pieces to be taken in order.
struct Old {
    /// Whether it was recorded at this basis: if not, nothing is taken.
    same_basis: bool,
    same_generation: bool,
    rows: Vec<Option<KeptRow>>,
    by_guid: FxHashMap<String, usize>,
    /// The last old row taken: rows are taken in order, or not at all.
    taken: Option<usize>,
    commands: Taker<RenderCommand>,
    x: Vec<(f64, f64)>,
    waves: HashMap<u32, Vec<LaneWave>>,
    boxes: Taker<ItemBox>,
    titles: Taker<Title>,
    shapes: Shapes,
    panel: PanelOld,
}

/// The rows' shapes as recorded, each taken at most once.
#[derive(Default)]
struct Shapes {
    track_h: Vec<f64>,
    lane_tracks: Vec<Vec<Track>>,
    laned: Vec<Option<Track>>,
    stacks: Vec<crate::lane_stack::Stack>,
    head_h: Vec<f64>,
    takes: Vec<Arc<crate::take_colors::TakeColors>>,
    lane_takes: Vec<Vec<(u32, usize)>>,
    slot_h: Vec<f64>,
}

/// One row's shape: its heights, its lanes, its takes.
struct Shape {
    track_h: f64,
    lane_tracks: Vec<Track>,
    laned: Option<Track>,
    stack: crate::lane_stack::Stack,
    head_h: f64,
    takes: Arc<crate::take_colors::TakeColors>,
    lane_takes: Vec<(u32, usize)>,
    slot_h: f64,
}

impl Shapes {
    fn take(&mut self, row: usize) -> Option<Shape> {
        Some(Shape {
            track_h: *self.track_h.get(row)?,
            lane_tracks: std::mem::take(self.lane_tracks.get_mut(row)?),
            laned: self.laned.get_mut(row)?.take(),
            stack: std::mem::take(self.stacks.get_mut(row)?),
            head_h: *self.head_h.get(row)?,
            takes: std::mem::take(self.takes.get_mut(row)?),
            lane_takes: std::mem::take(self.lane_takes.get_mut(row)?),
            slot_h: *self.slot_h.get(row)?,
        })
    }

    fn push(&mut self, shape: Shape) {
        self.track_h.push(shape.track_h);
        self.lane_tracks.push(shape.lane_tracks);
        self.laned.push(shape.laned);
        self.stacks.push(shape.stack);
        self.head_h.push(shape.head_h);
        self.takes.push(shape.takes);
        self.lane_takes.push(shape.lane_takes);
        self.slot_h.push(shape.slot_h);
    }
}

/// A vector's items taken by ranges in increasing order, moved out.
struct Taker<T> {
    items: std::vec::IntoIter<T>,
    at: usize,
}

impl<T> Taker<T> {
    fn new(items: Vec<T>) -> Self {
        Self {
            items: items.into_iter(),
            at: 0,
        }
    }

    /// Whether `range` is still to be taken: it starts no earlier than
    /// what was taken last ends.
    const fn can_take(&self, range: &Range<usize>) -> bool {
        range.start >= self.at
    }

    /// The items in `range`, those before it dropped. Nothing when it
    /// starts before what was taken last ends ([`Self::can_take`]).
    fn take(&mut self, range: Range<usize>) -> impl Iterator<Item = T> + '_ {
        let skip = range.start.checked_sub(self.at);
        if let Some(skip) = skip.filter(|skip| *skip > 0) {
            self.items.nth(skip - 1);
        }
        let len = if skip.is_some() { range.len() } else { 0 };
        self.at = self.at.max(range.end);
        self.items.by_ref().take(len)
    }
}

/// What is being recorded.
struct Out {
    lanes: Scene,
    index: Index,
    boxes: Vec<ItemBox>,
    titles: Vec<Title>,
    shapes: Shapes,
    rows: Vec<KeptRow>,
    y: f64,
}

/// What the rows are recorded from, read once for the whole song.
struct Song<'a> {
    project: &'a Project,
    previews: &'a crate::midi::Previews,
    palette: &'a Palette,
    tcp: crate::tcp::Tcp,
    layout: crate::layout::Layout,
    out_of_sight: &'a std::collections::HashSet<String>,
    keys: Vec<(f64, keyflow::key::Key)>,
    roles: HashMap<&'a str, LaneRole>,
    /// Each track's items as their detail is read, as a number
    /// ([`crate::wave_detail::Source::version`]).
    versions: FxHashMap<&'a str, u64>,
    /// Each track's items as drawn, as a number.
    drawn: FxHashMap<&'a str, u64>,
    hidden: std::collections::HashSet<&'a str>,
    /// The previews' generation as recording began
    /// ([`crate::midi::Previews::generation`]).
    generation: u64,
}

/// A row's own context, what its pieces draw in.
struct Row<'a> {
    row: usize,
    track: &'a Track,
    lanes_shown: bool,
    lane_track: Track,
    stack: crate::lane_stack::Stack,
    colors: Arc<crate::take_colors::TakeColors>,
    y: f64,
    body: f64,
    inset: f64,
    track_color: Color,
    /// A shut folder's loudest, its items' gain.
    loudest: Option<f64>,
}

impl Arrangement {
    /// Record `project`'s `rows`, taking from `old` what has not changed.
    #[expect(clippy::too_many_arguments, reason = "build_hiding's own")]
    pub(super) fn record(
        palette: &Palette,
        font: &crate::text::Font,
        (project, rows): (&Project, &[(Track, u32)]),
        layout: crate::layout::Layout,
        previews: &crate::midi::Previews,
        tcp: crate::tcp::Tcp,
        out_of_sight: &std::collections::HashSet<String>,
        old: Option<Self>,
    ) -> Self {
        let mut stages = crate::stages::Stages::start();
        let keys = keys_of(project);
        let roles = kit_roles(project);
        let (versions, drawn) = track_hashes(project);
        let song = Song {
            project,
            previews,
            palette,
            tcp,
            layout,
            out_of_sight,
            keys,
            roles,
            versions,
            drawn,
            hidden: project
                .tracks
                .iter()
                .filter(|t| !t.visible_in_tcp)
                .map(|t| t.guid.as_str())
                .collect(),
            generation: previews.generation(),
        };
        stages.mark("build.song_ms");
        let basis = basis_of(&song);
        let generation = song.generation;
        let (mut old, mut kept) = match old {
            Some(old) => Old::of(old, &basis, generation),
            None => (None, Kept::default()),
        };
        let mixes = Mixes {
            kept: std::cell::RefCell::new(std::mem::take(&mut kept.mixes)),
            made: std::cell::RefCell::default(),
        };
        let mut reads = std::mem::take(&mut kept.reads);
        let moved = reads.moved(&song.versions);
        let mut out = Out {
            lanes: Scene::new(),
            index: Index::default(),
            boxes: Vec::with_capacity(project.item_count),
            titles: Vec::with_capacity(project.item_count),
            shapes: Shapes::default(),
            rows: Vec::with_capacity(rows.len()),
            y: 0.0,
        };
        for (row, (track, depth)) in rows.iter().enumerate() {
            let whole = row_fingerprint(&song, track, *depth, (row, out.y), generation);
            if let Some(old) = old.as_mut()
                && old.take_row(&mut out, (&mixes, &moved), (row, &track.guid), whole)
            {
                continue;
            }
            record_row(
                &mut out,
                &song,
                (&mixes, old.as_mut(), &mut reads, &moved),
                (row, track, *depth),
                whole,
            );
        }
        debug_assert_eq!(
            out.index.x.len(),
            out.lanes.commands.len(),
            "one x extent per lane command"
        );
        stages.mark("build.rows_ms");

        let (panel, panel_kept) = cut_panel(
            (palette, font),
            (rows, &out.shapes),
            tcp,
            kept.panel.take(),
            old.as_mut().map(|old| &mut old.panel),
        );
        out.index.panel = panel.spans;
        out.index.panel_bar = panel.bar_spans;
        stages.mark("build.panel_ms");

        let Out {
            lanes,
            index,
            boxes,
            titles,
            shapes,
            rows: kept_rows,
            ..
        } = out;
        reads.loudest.forget_unheld();
        // A folder no longer shut keeps nothing read for it.
        reads
            .children
            .retain(|folder, _| project.folds.contains_key(folder));
        reads.versions = song
            .versions
            .iter()
            .map(|(track, version)| ((*track).to_owned(), *version))
            .collect();
        Self {
            tcp,
            lanes,
            panel: panel.panel,
            panel_bar: panel.bar,
            panel_zoom: panel.zoom,
            index,
            rows_at: RowLayout::from_heights(0, shapes.slot_h.iter().copied()),
            fine_behind: std::cell::Cell::new(false),
            lanes_wide: std::cell::Cell::new(false),
            track_h: shapes.track_h,
            lane_tracks: shapes.lane_tracks,
            laned: shapes.laned,
            stacks: shapes.stacks,
            head_h: shapes.head_h,
            takes: shapes.takes,
            lane_takes: shapes.lane_takes,
            rows: rows.len(),
            length_secs: project.length_secs,
            bpm: project.bpm,
            tempo: project.tempo.clone(),
            song_start: crate::ruler::song_start_of(project),
            item_count: project.item_count,
            titles,
            items: boxes,
            sections: project.sections.clone(),
            markers: project.markers.clone(),
            lettering: Lettering::from_env(),
            chart: chart_marks(project, &song.keys),
            kept: Kept {
                basis: Some(basis),
                generation,
                rows: kept_rows,
                mixes: mixes.made.into_inner(),
                reads,
                panel: panel_kept,
            },
        }
    }
}

impl Old {
    /// `old` taken apart to record from, and what it keeps.
    fn of(mut old: Arrangement, basis: &Basis, generation: u64) -> (Option<Self>, Kept) {
        let mut kept = std::mem::take(&mut old.kept);
        let same_basis = kept.basis.as_ref() == Some(basis);
        let rows: Vec<Option<KeptRow>> = std::mem::take(&mut kept.rows)
            .into_iter()
            .map(Some)
            .collect();
        let by_guid = rows
            .iter()
            .enumerate()
            .filter_map(|(i, row)| Some((row.as_ref()?.guid.clone(), i)))
            .collect();
        let slot_h = (0..old.rows)
            .map(|row| old.slot_of(row).map_or(0.0, |(_, h)| h))
            .collect();
        let shapes = Shapes {
            track_h: std::mem::take(&mut old.track_h),
            lane_tracks: std::mem::take(&mut old.lane_tracks),
            laned: std::mem::take(&mut old.laned),
            stacks: std::mem::take(&mut old.stacks),
            head_h: std::mem::take(&mut old.head_h),
            takes: std::mem::take(&mut old.takes),
            lane_takes: std::mem::take(&mut old.lane_takes),
            slot_h,
        };
        let mut index = std::mem::take(&mut old.index);
        let taken = Self {
            same_basis,
            same_generation: same_basis && kept.generation == generation,
            rows,
            by_guid,
            taken: None,
            commands: Taker::new(std::mem::take(&mut old.lanes.commands)),
            x: std::mem::take(&mut index.x),
            waves: std::mem::take(&mut index.waves),
            boxes: Taker::new(std::mem::take(&mut old.items)),
            titles: Taker::new(std::mem::take(&mut old.titles)),
            shapes,
            panel: PanelOld {
                panel: std::mem::take(&mut old.panel),
                bar: std::mem::take(&mut old.panel_bar),
                spans: std::mem::take(&mut index.panel),
                bar_spans: std::mem::take(&mut index.panel_bar),
                zoom: old.panel_zoom,
            },
        };
        (Some(taken), kept)
    }

    /// The old row for track `guid`, if it comes after the last taken.
    fn row_of(&self, guid: &str) -> Option<usize> {
        let at = *self.by_guid.get(guid)?;
        self.taken.is_none_or(|last| at > last).then_some(at)
    }

    /// The old row for track `guid` as kept, taken (if it comes after the
    /// last taken): its pieces are taken from it one by one.
    fn take_kept_row(&mut self, guid: &str) -> Option<KeptRow> {
        let at = self.row_of(guid)?;
        self.taken = Some(at);
        self.rows.get_mut(at)?.take()
    }

    /// Take the row for `guid` whole, as row `row`, when it was recorded
    /// from just what it would be now (`whole`).
    fn take_row(
        &mut self,
        out: &mut Out,
        (mixes, moved): (&Mixes, &Moved),
        (row, guid): (usize, &str),
        whole: u64,
    ) -> bool {
        if !self.same_generation {
            return false;
        }
        let Some(at) = self.row_of(guid) else {
            return false;
        };
        if self.rows[at]
            .as_ref()
            .is_none_or(|kept| kept.whole != whole)
        {
            return false;
        }
        let Some(kept) = self.rows[at].take() else {
            return false;
        };
        let Some(shape) = self.shapes.take(at) else {
            return false;
        };
        self.taken = Some(at);
        let commands = command_index(&out.lanes);
        let boxes = out.boxes.len();
        let titles = out.titles.len();
        let delta = Delta::between(&kept, commands, boxes, titles);
        if !self.move_pieces(
            out,
            moved,
            (
                kept.commands.clone(),
                kept.boxes.clone(),
                kept.titles.clone(),
            ),
            row,
        ) {
            return false;
        }
        for chunk in &kept.chunks {
            for key in &chunk.mixes {
                mixes.carry(key);
            }
        }
        out.index.lanes.push(commands..command_index(&out.lanes));
        out.y += shape.slot_h;
        out.shapes.push(shape);
        out.rows.push(KeptRow {
            guid: kept.guid,
            whole,
            commands: commands..command_index(&out.lanes),
            boxes: boxes..out.boxes.len(),
            titles: titles..out.titles.len(),
            chunks: kept.chunks.into_iter().map(|c| delta.shift(c)).collect(),
        });
        true
    }

    /// Move the commands, boxes and titles in these old ranges onto `out`
    /// for row `row`: the commands' extents and waveforms with them (their
    /// tracks' versions stamped as they are now), the boxes and titles
    /// re-rowed. `false` if they were already passed.
    fn move_pieces(
        &mut self,
        out: &mut Out,
        moved: &Moved,
        (commands, boxes, titles): (Range<u32>, Range<usize>, Range<usize>),
        row: usize,
    ) -> bool {
        let wide = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
        let range = wide(commands.start)..wide(commands.end);
        if !(self.commands.can_take(&range)
            && self.boxes.can_take(&boxes)
            && self.titles.can_take(&titles))
        {
            return false;
        }
        let from = command_index(&out.lanes);
        out.lanes.commands.extend(self.commands.take(range.clone()));
        out.index
            .x
            .extend(self.x.get(range).unwrap_or(&[]).iter().copied());
        for (k, old) in (commands.start..commands.end).enumerate() {
            if let Some(mut waves) = self.waves.remove(&old) {
                if !moved.is_empty() {
                    for wave in &mut waves {
                        restamp(wave, moved);
                    }
                }
                let at = from.saturating_add(u32::try_from(k).unwrap_or(u32::MAX));
                out.index.waves.insert(at, waves);
            }
        }
        out.boxes
            .extend(self.boxes.take(boxes).map(|b| ItemBox { row, ..b }));
        out.titles
            .extend(self.titles.take(titles).map(|t| Title { row, ..t }));
        true
    }
}

/// The panel as recorded, to keep if nothing it was cut from changed.
struct PanelOld {
    panel: Scene,
    bar: Scene,
    spans: Vec<Range<u32>>,
    bar_spans: Vec<Range<u32>>,
    zoom: f64,
}

/// How far a row's pieces moved from where they were recorded.
struct Delta {
    commands: i64,
    boxes: i64,
    titles: i64,
}

impl Delta {
    fn between(kept: &KeptRow, commands: u32, boxes: usize, titles: usize) -> Self {
        let wide = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
        Self {
            commands: i64::from(commands) - i64::from(kept.commands.start),
            boxes: wide(boxes) - wide(kept.boxes.start),
            titles: wide(titles) - wide(kept.titles.start),
        }
    }

    fn shift(&self, chunk: KeptChunk) -> KeptChunk {
        let by = |r: &Range<usize>, d: i64| {
            let move_by =
                |n: usize| usize::try_from(i64::try_from(n).unwrap_or(0) + d).unwrap_or(0);
            move_by(r.start)..move_by(r.end)
        };
        let commands = |r: &Range<u32>| {
            let move_by = |n: u32| u32::try_from(i64::from(n) + self.commands).unwrap_or(0);
            move_by(r.start)..move_by(r.end)
        };
        KeptChunk {
            commands: commands(&chunk.commands),
            boxes: by(&chunk.boxes, self.boxes),
            titles: by(&chunk.titles, self.titles),
            ..chunk
        }
    }
}

impl Mixes {
    /// Keep the last recording's mix of `key` for the next, unused here: a
    /// piece taken whole uses what it mixed without mixing it again.
    fn carry(&self, key: &MixKey) {
        if let Some((key, mixed)) = self.kept.borrow_mut().remove_entry(key) {
            self.made.borrow_mut().insert(key, mixed);
        }
    }
}

/// The tracks whose items moved since the last recording, each with its
/// version now ([`track_hashes`]).
type Moved = Vec<(String, u64)>;

/// `wave`'s detail read as its tracks are now (`moved` the ones that
/// moved): what a fresh recording gives it. A version moved, what it kept
/// of the old is let go.
fn restamp(wave: &mut LaneWave, moved: &Moved) {
    let mut stale = false;
    for source in &mut wave.sources {
        let Some(track) = source.track.as_deref() else {
            continue;
        };
        if let Some((_, now)) = moved.iter().find(|(t, _)| t == track)
            && source.version != *now
        {
            source.version = *now;
            stale = true;
        }
    }
    if stale {
        *wave.fine_kept.borrow_mut() = None;
    }
}

impl Reads {
    /// The tracks whose version is not the one last drawn.
    fn moved(&self, versions: &FxHashMap<&str, u64>) -> Moved {
        versions
            .iter()
            .filter(|(track, now)| self.versions.get(**track) != Some(*now))
            .map(|(track, now)| ((*track).to_owned(), *now))
            .collect()
    }

    /// The takes recorded on `tracks`, numbered, each track's read again
    /// only if its items changed: a kit's row is twenty tracks' takes.
    fn takes_on<'t>(
        takes: &mut FxHashMap<String, (u64, Vec<(String, f64)>)>,
        song: &Song<'_>,
        tracks: impl IntoIterator<Item = &'t Track>,
    ) -> crate::take_colors::TakeColors {
        let mut all: Vec<(String, f64)> = Vec::new();
        for track in tracks {
            let drawn = song.drawn.get(track.guid.as_str()).copied().unwrap_or(0);
            let fresh = takes.get(&track.guid).is_some_and(|(at, _)| *at == drawn);
            if !fresh {
                takes.insert(
                    track.guid.clone(),
                    (drawn, longest_takes(song.project, &track.guid)),
                );
            }
            if let Some((_, read)) = takes.get(&track.guid) {
                all.extend(read.iter().cloned());
            }
        }
        crate::take_colors::TakeColors::of(
            all.iter().map(|(name, length)| (name.as_str(), *length)),
        )
    }
}

/// Each take name on `track`'s items, at its longest.
fn longest_takes(project: &Project, track: &str) -> Vec<(String, f64)> {
    let mut longest: FxHashMap<&str, f64> = FxHashMap::default();
    for item in project.items.get(track).into_iter().flatten() {
        if let Some(name) = project.names.get(&item.guid) {
            let at = longest.entry(name.as_str()).or_insert(0.0);
            *at = at.max(item.length.as_seconds());
        }
    }
    longest
        .into_iter()
        .map(|(name, length)| (name.to_owned(), length))
        .collect()
}

/// Record row `row` — `track` at `depth` — from scratch where it must, and
/// from `old`'s pieces of it where they are keyed as now.
fn record_row(
    out: &mut Out,
    song: &Song<'_>,
    (mixes, old, reads, moved): (&Mixes, Option<&mut Old>, &mut Reads, &Moved),
    (row, track, depth): (usize, &Track, u32),
    whole: u64,
) {
    let project = song.project;
    // A shut folder's children, read again only where they changed; and its
    // gain, the loudest of them.
    let children = project.folds.get(&track.guid).map(|fold| {
        let kept = reads.children.entry(track.guid.clone()).or_default();
        kept.refresh(song, track, (song.generation, &mut reads.loudest));
        kept.table(fold)
    });
    let shown = Shown::of(song, &mut reads.takes, (track, depth));
    let from = (command_index(&out.lanes), out.boxes.len(), out.titles.len());
    let body = (shown.height - DIVIDER).max(0.5);
    record_row_head(out, song, (row, out.y, shown.height, body), &shown.stack);
    let ctx = Row {
        row,
        track,
        lanes_shown: shown.laned.is_some(),
        lane_track: shown.laned.clone().unwrap_or_else(|| track.clone()),
        stack: shown.stack,
        colors: shown.colors,
        y: out.y,
        body,
        // Items sit inside their lane, but a two-pixel inset on a
        // three-pixel row leaves nothing to see. Scaled down as the row
        // shrinks, so a collapsed session still shows its items as
        // bands rather than as empty lanes.
        inset: (body * 0.05).clamp(0.0, 2.0),
        track_color: crate::tcp::track_color(song.palette, track),
        loudest: children
            .as_ref()
            .map(|children| children.loudest(&track.guid)),
    };
    let read = RowRead::of(song, shown.items);
    let (chunks, lane_take) = record_pieces(
        out,
        song,
        (&ctx, &read, children.as_ref()),
        (mixes, old, moved),
    );
    out.index.lanes.push(from.0..command_index(&out.lanes));
    let slot = shown.height + crate::automation::lanes_height(&track.guid);
    out.shapes.push(Shape {
        track_h: shown.height,
        lane_tracks: crate::lanes::shown_on(&project.tracks, track)
            .into_iter()
            .cloned()
            .collect(),
        laned: shown.laned,
        stack: ctx.stack,
        head_h: shown.head,
        takes: ctx.colors,
        lane_takes: lane_take
            .into_iter()
            .filter_map(|(lane, take)| Some((lane, take?)))
            .collect(),
        slot_h: slot,
    });
    out.rows.push(KeptRow {
        guid: track.guid.clone(),
        whole,
        commands: from.0..command_index(&out.lanes),
        boxes: from.1..out.boxes.len(),
        titles: from.2..out.titles.len(),
        chunks,
    });
    out.y += slot;
}

/// What a row shows: which lanes, its items, how tall, its takes.
struct Shown<'p> {
    /// The lanes it shows them all of ([`row_lanes`]); `None` showing what
    /// plays.
    laned: Option<Track>,
    /// Its items as drawn: on a row showing its lanes, every lane's but
    /// the takes kept out of sight (never off the lane that plays or the
    /// comp); else what plays.
    items: Vec<&'p Item>,
    /// The track's own height, or the user's default, floored at something
    /// far below REAPER's minimum — see `layout`. The divider lives INSIDE
    /// the row, so a 24 track is 24 tall.
    head: f64,
    /// A strip each for the lanes holding something and the one that
    /// plays, the one that plays at the track's height and each take a lane
    /// tall under it; empty showing what plays.
    stack: crate::lane_stack::Stack,
    height: f64,
    /// Its takes, each its colour: on its lanes, a kit's comp strip.
    colors: Arc<crate::take_colors::TakeColors>,
}

impl<'p> Shown<'p> {
    fn of(
        song: &Song<'p>,
        takes: &mut FxHashMap<String, (u64, Vec<(String, f64)>)>,
        (track, depth): (&Track, u32),
    ) -> Self {
        let project = song.project;
        let laned = row_lanes(project, track);
        let head = song.layout.height_of(track.height);
        let Some(lanes) = laned.as_ref() else {
            let colors = (track.is_folder && depth == 0)
                .then(|| Arc::new(Reads::takes_on(takes, song, tracks_of_row(project, track))))
                .unwrap_or_default();
            return Self {
                laned,
                items: project.sounding(track),
                head,
                stack: crate::lane_stack::Stack::default(),
                height: head,
                colors,
            };
        };
        let heard = [
            crate::lanes::playing(lanes),
            crate::arrange_edit::comp_lane(lanes),
        ];
        let items: Vec<&Item> = project
            .lane(&track.guid)
            .iter()
            .filter(|i| {
                !song.out_of_sight.contains(&i.guid)
                    || heard.contains(&Some(i.fixed_lane.unwrap_or(0)))
            })
            .collect();
        let stack = crate::lane_stack::Stack::new(
            lanes,
            items.iter().map(|i| i.fixed_lane.unwrap_or(0)),
            head,
        );
        Self {
            height: stack.height(),
            colors: Arc::new(Reads::takes_on(takes, song, tracks_of_row(project, track))),
            laned,
            items,
            head,
            stack,
        }
    }
}

/// `track` and every track under it.
fn tracks_of_row<'p>(project: &'p Project, track: &'p Track) -> impl Iterator<Item = &'p Track> {
    std::iter::once(track).chain(daw_ui::studio::folded::under(&project.tracks, &track.guid))
}

/// A row's items as read for drawing: each one's peaks and notes, and
/// the runs of slices drawn as one.
struct RowRead<'p> {
    items: Vec<&'p Item>,
    waves: Vec<Option<Arc<crate::midi::Wave>>>,
    notes: Vec<Option<Vec<crate::midi::Note>>>,
    runs: SliceRuns,
}

impl<'p> RowRead<'p> {
    fn of(song: &Song<'p>, items: Vec<&'p Item>) -> Self {
        let project = song.project;
        let waves: Vec<Option<Arc<crate::midi::Wave>>> =
            items.iter().map(|i| song.previews.wave(&i.guid)).collect();
        let notes: Vec<Option<Vec<crate::midi::Note>>> =
            items.iter().map(|i| song.previews.get(&i.guid)).collect();
        // A take cut at every hit draws as one item a run: slices with
        // their peaks read, no notes, of no shut folder.
        let runs = seamless_runs(
            &items,
            |a, b| project.title(a).is_some() && project.title(a) == project.title(b),
            |at| {
                items
                    .get(at)
                    .is_some_and(|i| daw_ui::studio::folded::parse_guid(&i.guid).is_none())
                    && notes.get(at).is_none_or(Option::is_none)
                    && waves.get(at).is_some_and(Option::is_some)
            },
        );
        Self {
            items,
            waves,
            notes,
            runs,
        }
    }
}

/// A row's items (`read`) recorded onto `out` piece by piece ([`chunks_of`]):
/// each piece keyed as now, taken from `old`'s same row where it was keyed
/// alike, else recorded. The pieces as kept, and the lanes and takes they
/// hold ([`note_lane_take`]).
fn record_pieces(
    out: &mut Out,
    song: &Song<'_>,
    (ctx, read, children): (&Row<'_>, &RowRead<'_>, Option<&ChildTable<'_>>),
    (mixes, mut old, moved): (&Mixes, Option<&mut Old>, &Moved),
) -> (Vec<KeptChunk>, Vec<(u32, Option<usize>)>) {
    let context = row_context(ctx);
    let mut was = old
        .as_deref_mut()
        .filter(|old| old.same_basis)
        .and_then(|old| old.take_kept_row(&ctx.track.guid));
    let keyed: FxHashMap<u64, usize> = was
        .iter()
        .flat_map(|kept| kept.chunks.iter().enumerate().map(|(i, c)| (c.key, i)))
        .collect();
    let mut lane_take: Vec<(u32, Option<usize>)> = Vec::new();
    let mut chunks = Vec::new();
    let mut last: Option<usize> = None;
    for range in chunks_of(&read.items, &read.runs) {
        let (key, held) = chunk_key(context, song, (read, range.clone()), children);
        let reused = keyed
            .get(&key)
            .copied()
            .filter(|&i| last.is_none_or(|last| i > last));
        let taken = match (reused, old.as_deref_mut(), was.as_mut()) {
            (Some(i), Some(old), Some(was)) => {
                take_chunk(out, (moved, mixes), old, &mut was.chunks[i], ctx.row).inspect(|taken| {
                    last = Some(i);
                    if children.is_some() {
                        restamp_folded(
                            out,
                            (song.project, &ctx.colors),
                            &read.items[range.clone()],
                            (taken.boxes.clone(), taken.titles.clone()),
                        );
                    }
                })
            }
            _ => None,
        };
        let chunk =
            taken.unwrap_or_else(|| record_chunk(out, song, (ctx, read, range), (mixes, children)));
        for (lane, take) in &chunk.lane_takes {
            note_lane_take(&mut lane_take, *lane, *take);
        }
        chunks.push(KeptChunk { key, held, ..chunk });
    }
    (chunks, lane_take)
}

/// The items of `read` in `range` recorded onto `out`, as a piece.
fn record_chunk(
    out: &mut Out,
    song: &Song<'_>,
    (ctx, read, range): (&Row<'_>, &RowRead<'_>, Range<usize>),
    (mixes, children): (&Mixes, Option<&ChildTable<'_>>),
) -> KeptChunk {
    let commands = command_index(&out.lanes);
    let (boxes, titles) = (out.boxes.len(), out.titles.len());
    let mut used = Vec::new();
    let mut lane_takes = Vec::new();
    for at in range {
        record_item(
            out,
            song,
            ctx,
            (read, at),
            (mixes, children, &mut used, &mut lane_takes),
        );
    }
    KeptChunk {
        key: 0,
        commands: commands..command_index(&out.lanes),
        boxes: boxes..out.boxes.len(),
        titles: titles..out.titles.len(),
        mixes: used,
        lane_takes,
        held: Vec::new(),
    }
}

/// Take `chunk` from the old recording onto `out`, as row `row`'s.
fn take_chunk(
    out: &mut Out,
    (moved, mixes): (&Moved, &Mixes),
    old: &mut Old,
    chunk: &mut KeptChunk,
    row: usize,
) -> Option<KeptChunk> {
    let commands = command_index(&out.lanes);
    let (boxes, titles) = (out.boxes.len(), out.titles.len());
    if !old.move_pieces(
        out,
        moved,
        (
            chunk.commands.clone(),
            chunk.boxes.clone(),
            chunk.titles.clone(),
        ),
        row,
    ) {
        return None;
    }
    for key in &chunk.mixes {
        mixes.carry(key);
    }
    Some(KeptChunk {
        key: chunk.key,
        commands: commands..command_index(&out.lanes),
        boxes: boxes..out.boxes.len(),
        titles: titles..out.titles.len(),
        mixes: std::mem::take(&mut chunk.mixes),
        lane_takes: std::mem::take(&mut chunk.lane_takes),
        held: Vec::new(),
    })
}

/// The colour `item` is ringed in: its take's (`numbered`), or else its
/// name's ([`take_color`]), or else its own (`color`).
fn outline(project: &Project, item: &Item, (numbered, color): (Option<usize>, Color)) -> Color {
    numbered.map_or_else(
        || project.title(item).map_or(color, take_color),
        |i| crate::take_colors::swatch(i).1,
    )
}

/// A piece of a shut folder's row just taken (its `boxes` and `titles` on
/// `out`) named for its `items` as they are now: a span added before them
/// renumbers them, and their guids and names with them.
fn restamp_folded(
    out: &mut Out,
    (project, colors): (&Project, &crate::take_colors::TakeColors),
    items: &[&Item],
    (boxes, titles): (Range<usize>, Range<usize>),
) {
    for (taken, item) in out.boxes[boxes].iter_mut().zip(items) {
        if taken.guid != item.guid {
            taken.guid.clone_from(&item.guid);
            let numbered = take_name(project, item).and_then(|t| colors.index_of(t));
            taken.outline = outline(project, item, (numbered, taken.color));
        }
    }
    let named = items.iter().filter_map(|item| project.title(item));
    for (title, name) in out.titles[titles].iter_mut().zip(named) {
        if title.name != name {
            name.clone_into(&mut title.name);
        }
    }
}

/// A row's background, the divider under it and the lines between its
/// lanes' strips.
fn record_row_head(
    out: &mut Out,
    song: &Song<'_>,
    (row, y, h, body): (usize, f64, f64, f64),
    stack: &crate::lane_stack::Stack,
) {
    let palette = song.palette;
    let reach = song.project.length_secs.max(1.0);
    let stripe = if row % 2 == 0 {
        palette.row_a
    } else {
        palette.row_b
    };
    // Recorded absurdly wide so a replay at any zoom still covers the
    // viewport; the backend clips.
    let mut fill = |color: Color, rect: Rect| {
        out.lanes
            .fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
        out.index.x.push((f64::MIN, f64::MAX));
    };
    fill(stripe, Rect::new(0.0, y, reach, y + body));
    fill(palette.divider, Rect::new(0.0, y + body, reach, y + h));
    for at in stack.seams() {
        let line = at.mul_add(body, y);
        fill(
            palette.divider.multiply_alpha(0.6),
            Rect::new(0.0, line - 0.25, reach, line + 0.25),
        );
    }
}

/// Item `at` of `items`, recorded onto `out`: its body, its waveform (at
/// replay), its notes, its fades, its box and its title.
fn record_item(
    out: &mut Out,
    song: &Song<'_>,
    ctx: &Row<'_>,
    (read, at): (&RowRead<'_>, usize),
    (mixes, children, used, lane_takes): (
        &Mixes,
        Option<&ChildTable<'_>>,
        &mut Vec<MixKey>,
        &mut Vec<(u32, Option<usize>)>,
    ),
) {
    let project = song.project;
    let RowRead {
        items,
        waves,
        notes,
        runs,
    } = read;
    let Some(&item) = items.get(at) else { return };
    let part = runs.parts.get(at).copied().unwrap_or(Part::Alone);
    let inside = part == Part::Inside;
    // The run it draws for, when it heads one: its slices.
    let run: Vec<usize> = match part {
        Part::Head { run } => runs.members[run].clone(),
        _ => vec![at],
    };
    let band = if ctx.lanes_shown {
        ctx.stack.band(item.fixed_lane.unwrap_or(0))
    } else {
        (0.0, 1.0)
    };
    let plays = !ctx.lanes_shown || daw_ui::studio::project::on_playing_lane(&ctx.lane_track, item);
    let (y, body) = (
        band.0.mul_add(ctx.body, ctx.y),
        (band.1 - band.0) * ctx.body,
    );
    let x0 = item.position.as_seconds();
    let x1 = x0 + item.length.as_seconds().max(0.001);
    let drawn_x1 = run.last().and_then(|&k| items.get(k)).map_or(x1, |last| {
        last.position.as_seconds() + last.length.as_seconds().max(0.001)
    });
    // How far into the source this item starts. Zero for everything that
    // has not been slipped, which is why adding it moves no picture that
    // was right before.
    let slip = item.start_offset.as_seconds();
    let take = take_name(project, item);
    let numbered = take.and_then(|t| ctx.colors.index_of(t));
    if ctx.lanes_shown {
        note_lane_take(lane_takes, item.fixed_lane.unwrap_or(0), numbered);
    }
    // On a row showing its lanes, each take's body in its own colour —
    // "the blue take" — and its waveform as ever.
    let ink = item_color(item, ctx.track_color, song.tcp.mixing);
    let take_body = numbered
        .filter(|_| ctx.lanes_shown)
        .map(|i| crate::take_colors::swatch(i).1);
    let color = take_body.unwrap_or(ink);
    // A lane that does not play is there to be chosen from, not heard:
    // drawn faint.
    let faint = |c: Color| if plays { c } else { c.multiply_alpha(0.45) };
    let (color, ink) = (faint(color), faint(ink));
    // The body, dimmed, and the waveform over it in the full colour; a
    // shut folder's body fainter still, a take's stronger.
    let folded_guid = daw_ui::studio::folded::parse_guid(&item.guid);
    let alpha = if take_body.is_some() {
        TAKE_BODY
    } else if folded_guid.is_some() {
        body_alpha() * FOLDER_BODY
    } else {
        body_alpha()
    };
    if !inside {
        out.lanes.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color.multiply_alpha(alpha),
            None,
            &Rect::new(x0, y + ctx.inset, drawn_x1, y + body - ctx.inset),
        );
        out.index.x.push((x0, drawn_x1));
    }
    let top = y + ctx.inset;
    let bottom = y + body - ctx.inset;
    // What the item CONTAINS: notes if it holds MIDI, the envelope
    // otherwise — each once it has been read, plain until then. A shut
    // folder's item holds what is under it.
    let folded = children.and_then(|children| {
        folded_contents(
            project,
            (&song.roles, &song.versions, &song.hidden, mixes),
            children,
            item,
        )
    });
    let folded = folded.map(|(held, key)| {
        used.push(key);
        held
    });
    let is_folder = folded.is_some();
    // An item that plays is read as its track plays: one read of the
    // track for all its items, kept through an edit. A run's: the
    // track's over all of it, or each slice's.
    let sources = if daw_ui::studio::project::on_playing_lane(&ctx.lane_track, item) && !item.muted
    {
        let version = song
            .versions
            .get(item.track_guid.as_str())
            .copied()
            .unwrap_or(0);
        let scale = if item.volume > 1e-6 {
            item.volume.recip()
        } else {
            1.0
        };
        vec![
            crate::wave_detail::Source::track(&item.track_guid, x0, drawn_x1 - x0, version)
                .scaled(scale),
        ]
    } else {
        run.iter()
            .filter_map(|&k| items.get(k))
            .map(|i| source_of(i))
            .collect()
    };
    let held = folded.unwrap_or_else(|| Held {
        notes: notes.get(at).cloned().flatten(),
        wave: waves.get(at).cloned().flatten(),
        layers: Vec::new(),
        sources,
    });
    let (item_notes, wave) = (held.notes, held.wave);
    // Audio: the take's own peaks, once read — plain until then.
    let drawn_wave = if run.len() > 1 {
        let slices: Vec<(f64, &crate::midi::Wave)> = run
            .iter()
            .filter_map(|&k| {
                Some((
                    items.get(k)?.position.as_seconds(),
                    &**waves.get(k)?.as_ref()?,
                ))
            })
            .collect();
        Some(Arc::new(joined_wave(&slices, x0, drawn_x1)))
    } else {
        wave.clone()
    };
    if let Some(whole) = drawn_wave.filter(|w| !inside && w.step > 0.0 && !w.points.is_empty()) {
        // One gain for every layer: the loudest anywhere reaches the edge,
        // the layers keep their loudness to each other. An item at its true
        // level through its item gain, as REAPER draws it; a shut folder's
        // row a summary, as loud as the lane allows.
        let gain = if held.layers.is_empty() && !is_folder {
            item.volume.max(0.0)
        } else if let Some(loudest) = ctx.loudest.filter(|_| folded_guid.is_some()) {
            // The folder's loudest — but never past this piece's own:
            // right after an edit the new slices' peaks are still being
            // read, and the loudest of what has arrived can be a quiet one.
            gain_for(loudest.max(loudest_of(&whole)))
        } else {
            wave_gain(&whole)
        };
        let lane = |wave, color, sources, role| LaneWave {
            runs: std::cell::RefCell::default(),
            fine_kept: std::cell::RefCell::default(),
            sources,
            role,
            gain,
            wave,
            x0,
            x1: drawn_x1,
            top,
            bottom,
            color,
        };
        let drawn: Vec<LaneWave> = if held.layers.is_empty() {
            vec![lane(whole, ink, held.sources, None)]
        } else {
            held.layers
                .into_iter()
                .map(|(role, wave, sources)| {
                    lane(wave, kit_color(role).unwrap_or(ink), sources, Some(role))
                })
                .collect()
        };
        // Over the body just recorded, drawn at replay.
        let body = command_index(&out.lanes).saturating_sub(1);
        out.index.waves.insert(body, drawn);
    }
    // On a folder of recorded audio the notes under it are a hint — the
    // drum MIDI, a sample's triggers — so faint, in a strip along the
    // bottom, under the audio's own shape.
    let hint = is_folder && wave.is_some();
    let (roll_top, roll_ink) = if hint {
        (
            (bottom - top).mul_add(-0.22, bottom),
            color.multiply_alpha(0.35),
        )
    } else {
        (top, color)
    };
    if let Some(roll) = item_notes
        .as_deref()
        .and_then(|notes| midi_preview(notes, x0, x1, roll_top, bottom))
    {
        out.lanes
            .fill(Fill::NonZero, Affine::IDENTITY, roll_ink, None, &roll);
        out.index.x.push((x0, x1));
    }
    // The fades, as the part of the item they take away; a run's: its
    // first slice's fade in, its last's out.
    let fades = Fades::of(item);
    let drawn_fades = run
        .last()
        .and_then(|&k| items.get(k))
        .map_or(fades, |last| {
            let out = Fades::of(last);
            Fades {
                fade_out: out.fade_out,
                out_shape: out.out_shape,
                ..fades
            }
        });
    if !inside && let Some(path) = drawn_fades.path(x0, drawn_x1, top, bottom, 1.0) {
        out.lanes
            .fill(Fill::NonZero, Affine::IDENTITY, FADE_SHADE, None, &path);
        out.index.x.push((x0, drawn_x1));
    }
    out.boxes.push(ItemBox {
        row: ctx.row,
        guid: item.guid.clone(),
        wave,
        color,
        slip,
        x0,
        x1,
        fades,
        // A shut folder's row stands for many items: no one item volume
        // to show or turn.
        volume: (!is_folder).then_some(item.volume),
        band,
        outline: outline(project, item, (numbered, color)),
        drawn_to: (!inside).then_some(drawn_x1),
    });
    if !inside && let Some(name) = project.title(item) {
        out.titles.push(Title {
            row: ctx.row,
            x0,
            x1: drawn_x1,
            name: name.to_owned(),
            spelled: spell(&ctx.track.name, name, x0, &song.keys),
        });
    }
}

/// Each child item's loudest peak, by guid, with the waveform it was read
/// from: a shut folder's gain is read from all of them, and reading every
/// waveform's every point each recording was a quarter of what an edit
/// cost.
#[derive(Default)]
pub(super) struct Loudest {
    by_guid: FxHashMap<String, (Arc<crate::midi::Wave>, f64)>,
}

impl Loudest {
    /// `guid`'s loudest peak in `wave`: read once a waveform.
    fn of(&mut self, guid: &str, wave: &Arc<crate::midi::Wave>) -> f64 {
        if let Some((kept, loudest)) = self.by_guid.get(guid)
            && Arc::ptr_eq(kept, wave)
        {
            return *loudest;
        }
        let loudest = loudest_of(wave);
        self.by_guid
            .insert(guid.to_owned(), (Arc::clone(wave), loudest));
        loudest
    }

    /// Let go of the waveforms nothing else holds any more: their items
    /// are gone, or read again.
    fn forget_unheld(&mut self) {
        self.by_guid
            .retain(|_, (wave, _)| Arc::strong_count(wave) > 1);
    }
}

/// A shut folder's children as its row reads them: each child by guid,
/// and the loudest peak of what its fold is of.
pub(super) struct ChildTable<'a> {
    by_guid: &'a FxHashMap<String, ChildNow>,
    /// The loudest peak under the folder, as read.
    loudest: f64,
}

/// One child of a shut folder, as read: where it is, its peaks and notes.
pub(super) struct ChildNow {
    pub(super) track: Arc<str>,
    pub(super) at: f64,
    pub(super) length: f64,
    pub(super) wave: Option<Arc<crate::midi::Wave>>,
    pub(super) notes: Option<Vec<crate::midi::Note>>,
    /// Its take's name: the folder item's take is its children's.
    name: Option<String>,
    /// Its loudest peak.
    loudest: f64,
    /// Everything a mix takes from it — which it is, where, its track,
    /// its peaks and notes — as one number.
    pub(super) hash: u64,
    /// Which it is, as a number.
    pub(super) id: u64,
}

/// A shut folder's children as last read, kept from one recording to the
/// next: a track's read again only when its items changed, and every one
/// once something arrived in the previews — the kit's 26 000 slices are
/// not looked up again for an edit to one mic.
#[derive(Default)]
pub(super) struct KeptChildren {
    /// The previews' generation they were read at.
    generation: Option<u64>,
    /// Each track under the folder: its items' version as read
    /// ([`track_hashes`]' `drawn`), and their guids.
    tracks: FxHashMap<String, (u64, Vec<String>)>,
    by_guid: FxHashMap<String, ChildNow>,
}

impl KeptChildren {
    /// Brought up to `song` under `folder`, at the previews' `generation`
    /// (each loudest peak from `loudest`).
    fn refresh(
        &mut self,
        song: &Song<'_>,
        folder: &Track,
        (generation, loudest): (u64, &mut Loudest),
    ) {
        let project = song.project;
        if self.generation != Some(generation) {
            self.read_again(song, loudest);
            self.generation = Some(generation);
        }
        let under = daw_ui::studio::folded::under(&project.tracks, &folder.guid);
        let now: FxHashSet<&str> = under.iter().map(|t| t.guid.as_str()).collect();
        // Tracks gone from under it.
        let gone: Vec<String> = self
            .tracks
            .keys()
            .filter(|t| !now.contains(t.as_str()))
            .cloned()
            .collect();
        for track in gone {
            self.forget(&track);
        }
        for track in under {
            let drawn = song.drawn.get(track.guid.as_str()).copied().unwrap_or(0);
            if self
                .tracks
                .get(&track.guid)
                .is_some_and(|(at, _)| *at == drawn)
            {
                continue;
            }
            self.forget(&track.guid);
            let items = project
                .items
                .get(&track.guid)
                .map_or(&[][..], Vec::as_slice);
            let shared: Arc<str> = Arc::from(track.guid.as_str());
            for item in items {
                self.by_guid
                    .insert(item.guid.clone(), child_now(song, item, &shared, loudest));
            }
            self.tracks.insert(
                track.guid.clone(),
                (drawn, items.iter().map(|item| item.guid.clone()).collect()),
            );
        }
    }

    /// Every child's peaks and notes read again: something arrived.
    fn read_again(&mut self, song: &Song<'_>, loudest: &mut Loudest) {
        for (guid, child) in &mut self.by_guid {
            let wave = song.previews.wave(guid);
            let notes = song.previews.get(guid);
            let same_wave = match (&wave, &child.wave) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if same_wave && notes == child.notes {
                continue;
            }
            child.loudest = wave.as_ref().map_or(0.0, |w| loudest.of(guid, w));
            child.wave = wave;
            child.notes = notes;
            child.hash = child.hashed();
        }
    }

    /// `track`'s children let go.
    fn forget(&mut self, track: &str) {
        if let Some((_, guids)) = self.tracks.remove(track) {
            for guid in guids {
                self.by_guid.remove(&guid);
            }
        }
    }

    /// The table `fold`'s row reads.
    fn table(&self, fold: &daw_ui::studio::folded::Fold) -> ChildTable<'_> {
        // The loudest of what the fold is of: a take on a lane that does
        // not play, under a row that shows what plays, is not.
        let loudest = fold
            .spans
            .iter()
            .flat_map(|span| span.from.iter())
            .filter_map(|guid| self.by_guid.get(guid))
            .filter(|child| child.wave.is_some())
            .fold(0.0_f64, |a, child| a.max(child.loudest));
        ChildTable {
            by_guid: &self.by_guid,
            loudest,
        }
    }
}

/// `item`, a child of a shut folder on `track`, as read now.
fn child_now(song: &Song<'_>, item: &Item, track: &Arc<str>, loudest: &mut Loudest) -> ChildNow {
    let wave = song.previews.wave(&item.guid);
    let mut id = hasher();
    item.guid.hash(&mut id);
    let mut child = ChildNow {
        track: Arc::clone(track),
        at: item.position.as_seconds(),
        length: item.length.as_seconds(),
        loudest: wave.as_ref().map_or(0.0, |w| loudest.of(&item.guid, w)),
        wave,
        notes: song.previews.get(&item.guid),
        name: song.project.names.get(&item.guid).cloned(),
        hash: 0,
        id: id.finish(),
    };
    child.hash = child.hashed();
    child
}

impl ChildNow {
    /// Everything a mix takes from it, as one number.
    fn hashed(&self) -> u64 {
        let mut h = hasher();
        self.id.hash(&mut h);
        hash_f64(&mut h, self.at);
        hash_f64(&mut h, self.length);
        self.track.hash(&mut h);
        hash_wave(&mut h, self.wave.as_ref());
        hash_notes(&mut h, self.notes.as_deref());
        self.name.hash(&mut h);
        h.finish()
    }
}

impl ChildTable<'_> {
    /// Child `guid`, if it is in the song.
    pub(super) fn get(&self, guid: &str) -> Option<&ChildNow> {
        self.by_guid.get(guid)
    }

    /// The folder's loudest point anywhere under it, as remembered
    /// ([`remembered_loudest`]): one display gain for all its items.
    fn loudest(&self, folder: &str) -> f64 {
        remembered_loudest(folder, self.loudest)
    }
}

/// A fast hasher, seeded alike every time: keys compare across recordings.
fn hasher() -> FxHasher {
    FxHasher::default()
}

fn hash_f64(h: &mut impl Hasher, value: f64) {
    value.to_bits().hash(h);
}

fn hash_color(h: &mut impl Hasher, color: Color) {
    for c in color.components {
        c.to_bits().hash(h);
    }
}

/// Everything about `item` a lane draws from (its take's name included).
fn hash_item(h: &mut impl Hasher, project: &Project, item: &Item) {
    hash_where(h, item);
    hash_how(h, project, item);
}

/// Where `item` is and what of its source it plays: what its detail is
/// read again for when it changes ([`crate::wave_detail::Source::version`]).
fn hash_where(h: &mut impl Hasher, item: &Item) {
    for n in [
        item.position.as_seconds(),
        item.length.as_seconds(),
        item.start_offset.as_seconds(),
        item.volume,
        item.fade_in_length.as_seconds(),
        item.fade_out_length.as_seconds(),
    ] {
        h.write_u64(n.to_bits());
    }
    item.fixed_lane.hash(h);
    item.muted.hash(h);
}

/// The rest of what `item` is drawn from: which it is, its fades' shapes,
/// its colour, its name.
fn hash_how(h: &mut impl Hasher, project: &Project, item: &Item) {
    item.guid.hash(h);
    item.index.hash(h);
    item.fade_in_shape.hash(h);
    item.fade_out_shape.hash(h);
    item.color.hash(h);
    item.label.hash(h);
    project.names.get(&item.guid).hash(h);
}

fn hash_notes(h: &mut impl Hasher, notes: Option<&[crate::midi::Note]>) {
    notes.map(<[_]>::len).hash(h);
    for note in notes.unwrap_or_default() {
        (
            note.at.to_bits(),
            note.len.to_bits(),
            note.pitch,
            note.velocity,
        )
            .hash(h);
    }
}

/// A waveform by identity: which one, not what is in it — a read always
/// makes a new one.
fn hash_wave(h: &mut impl Hasher, wave: Option<&Arc<crate::midi::Wave>>) {
    wave.map_or(0, |w| Arc::as_ptr(w) as usize).hash(h);
}

/// Each track's items as numbers, in one pass: as their detail is read
/// (`versions`, which a waveform's detail is held for), and as a lane draws
/// them (`drawn`: unchanged, a row drawn from them can be taken whole).
fn track_hashes(project: &Project) -> (FxHashMap<&str, u64>, FxHashMap<&str, u64>) {
    let mut versions = FxHashMap::default();
    let mut drawn = FxHashMap::default();
    versions.reserve(project.items.len());
    drawn.reserve(project.items.len());
    for (track, items) in &project.items {
        let (mut read, mut seen) = (hasher(), hasher());
        for item in items {
            hash_where(&mut read, item);
            hash_where(&mut seen, item);
            hash_how(&mut seen, project, item);
        }
        versions.insert(track.as_str(), read.finish());
        drawn.insert(track.as_str(), seen.finish());
    }
    (versions, drawn)
}

/// What every row is recorded at.
fn basis_of(song: &Song<'_>) -> Basis {
    let mut tracks = hasher();
    for track in &song.project.tracks {
        track.guid.hash(&mut tracks);
        track.parent_guid.hash(&mut tracks);
        track.name.hash(&mut tracks);
        track.is_folder.hash(&mut tracks);
        track.visible_in_tcp.hash(&mut tracks);
        track.lane_count.hash(&mut tracks);
        track.lane_play_mask.hash(&mut tracks);
        track.lane_names.hash(&mut tracks);
        format!("{:?}", track.lane_display).hash(&mut tracks);
    }
    let mut keys = hasher();
    format!("{:?}", song.keys).hash(&mut keys);
    // Order-free: a set's.
    let out_of_sight = song
        .out_of_sight
        .iter()
        .map(|guid| {
            let mut h = hasher();
            guid.hash(&mut h);
            h.finish()
        })
        .fold(0_u64, u64::wrapping_add);
    Basis {
        palette: song.palette.clone(),
        tcp: song.tcp,
        layout: song.layout,
        length: song.project.length_secs.to_bits(),
        alpha: body_alpha().to_bits(),
        keys: keys.finish(),
        tracks: tracks.finish(),
        out_of_sight,
    }
}

/// Everything row `row` (`track` at `depth`, its top at `y`) is drawn
/// from, as one number: the track, where it sits, which lanes it shows,
/// its items and — a folder — every item under it, and the previews'
/// `generation`.
fn row_fingerprint(
    song: &Song<'_>,
    track: &Track,
    depth: u32,
    (row, y): (usize, f64),
    generation: u64,
) -> u64 {
    let project = song.project;
    let mut h = hasher();
    generation.hash(&mut h);
    (row % 2).hash(&mut h);
    depth.hash(&mut h);
    hash_f64(&mut h, y);
    hash_f64(&mut h, crate::automation::lanes_height(&track.guid));
    track.guid.hash(&mut h);
    track.name.hash(&mut h);
    track.color.hash(&mut h);
    track.height.hash(&mut h);
    track.is_folder.hash(&mut h);
    track.lane_count.hash(&mut h);
    track.lane_play_mask.hash(&mut h);
    track.lane_names.hash(&mut h);
    crate::lanes::expanded(&track.guid).hash(&mut h);
    song.drawn.get(track.guid.as_str()).hash(&mut h);
    if track.is_folder {
        for under in daw_ui::studio::folded::under(&project.tracks, &track.guid) {
            crate::lanes::expanded(&under.guid).hash(&mut h);
            song.drawn.get(under.guid.as_str()).hash(&mut h);
        }
    }
    if let Some(fold) = project.folds.get(&track.guid) {
        fold.items.len().hash(&mut h);
        for (item, span) in fold.items.iter().zip(&fold.spans) {
            hash_f64(&mut h, item.position.as_seconds());
            hash_f64(&mut h, item.length.as_seconds());
            item.fixed_lane.hash(&mut h);
            span.whole.hash(&mut h);
            span.from.len().hash(&mut h);
        }
        crate::arrangement::remembered_now(&track.guid).hash(&mut h);
    }
    h.finish()
}

/// A row's own context, as one number: what every piece of it is drawn in.
fn row_context(ctx: &Row<'_>) -> u64 {
    let mut h = hasher();
    hash_f64(&mut h, ctx.y);
    hash_f64(&mut h, ctx.body);
    hash_f64(&mut h, ctx.inset);
    ctx.lanes_shown.hash(&mut h);
    ctx.stack.lanes().hash(&mut h);
    hash_f64(&mut h, ctx.stack.height());
    for lane in ctx.stack.lanes() {
        let (a, b) = ctx.stack.band(*lane);
        hash_f64(&mut h, a);
        hash_f64(&mut h, b);
    }
    ctx.lane_track.lane_count.hash(&mut h);
    ctx.lane_track.lane_play_mask.hash(&mut h);
    ctx.lane_track.lane_names.hash(&mut h);
    hash_color(&mut h, ctx.track_color);
    ctx.track.name.hash(&mut h);
    // The takes an item's take is numbered among: with its name (or its
    // children's), what numbers it.
    format!("{:?}", ctx.colors).hash(&mut h);
    ctx.loudest.map(f64::to_bits).hash(&mut h);
    h.finish()
}

/// Where `items` are cut into the pieces kept from one recording to the
/// next: where an item starts in another [`CHUNK_SECS`] step of the
/// timeline (or another lane, a folder's being lane by lane) than the one
/// before it — never inside a run of slices drawn as one.
pub(super) fn chunks_of(items: &[&Item], runs: &SliceRuns) -> Vec<Range<usize>> {
    // How far each item's run reaches, by index.
    let mut reach: Vec<usize> = (0..items.len()).collect();
    for members in &runs.members {
        let (Some(&first), Some(&last)) = (members.iter().min(), members.iter().max()) else {
            continue;
        };
        for slot in reach.iter_mut().take(last + 1).skip(first) {
            *slot = (*slot).max(last);
        }
    }
    #[expect(clippy::cast_possible_truncation, reason = "a step of the timeline")]
    let step = |item: &Item| (item.position.as_seconds() / CHUNK_SECS).floor() as i64;
    let mut out = Vec::new();
    let mut start = 0_usize;
    let mut held_to = 0_usize;
    for at in 1..items.len() {
        held_to = held_to.max(reach[at - 1]);
        let moved_on = step(items[at]) != step(items[at - 1])
            || items[at].fixed_lane != items[at - 1].fixed_lane;
        if moved_on && at > held_to {
            out.push(start..at);
            start = at;
        }
    }
    if !items.is_empty() {
        out.push(start..items.len());
    }
    out
}

/// A piece's key: the row's `context` and everything each of its items is
/// drawn from — the item, its place in a run, its name, its peaks and
/// notes, and a shut folder's every child under it. With the waveforms it
/// was made from, to keep alive.
fn chunk_key(
    context: u64,
    song: &Song<'_>,
    (read, range): (&RowRead<'_>, Range<usize>),
    children: Option<&ChildTable<'_>>,
) -> (u64, Vec<Arc<crate::midi::Wave>>) {
    let project = song.project;
    let RowRead {
        items,
        waves,
        notes,
        runs,
    } = read;
    let mut h = hasher();
    let mut held = Vec::new();
    context.hash(&mut h);
    range.len().hash(&mut h);
    for at in range.clone() {
        let Some(&item) = items.get(at) else { continue };
        if daw_ui::studio::folded::parse_guid(&item.guid).is_some() {
            // A shut folder's item by where it is, not which of the fold's
            // spans it is: a span added before it renumbers it, and draws
            // nothing different. Its name and guid are stamped again when
            // the piece is taken ([`restamp_folded`]).
            hash_where(&mut h, item);
            item.color.hash(&mut h);
        } else {
            hash_item(&mut h, project, item);
        }
        match runs.parts.get(at).copied().unwrap_or(Part::Alone) {
            Part::Alone => 0_usize.hash(&mut h),
            Part::Inside => 1_usize.hash(&mut h),
            Part::Head { run } => {
                2_usize.hash(&mut h);
                for member in &runs.members[run] {
                    member.saturating_sub(range.start).hash(&mut h);
                }
            }
        }
        let wave = waves.get(at).and_then(Option::as_ref);
        hash_wave(&mut h, wave);
        held.extend(wave.cloned());
        hash_notes(&mut h, notes.get(at).and_then(Option::as_deref));
        let span = daw_ui::studio::folded::parse_guid(&item.guid)
            .and_then(|(folder, index)| project.folds.get(folder)?.spans.get(index));
        if let (Some(span), Some(children)) = (span, children) {
            hash_f64(&mut h, span.start);
            hash_f64(&mut h, span.end);
            span.whole.hash(&mut h);
            for child in span.from.iter().filter_map(|guid| children.get(guid)) {
                h.write_u64(child.hash);
                held.extend(child.wave.clone());
            }
        }
    }
    (h.finish(), held)
}

/// The panel cut for `rows` at `shapes`' heights — or the last one, kept
/// (`kept`, `old`), when nothing it was cut from has changed.
fn cut_panel(
    (palette, font): (&Palette, &crate::text::Font),
    (rows, shapes): (&[(Track, u32)], &Shapes),
    tcp: crate::tcp::Tcp,
    kept: Option<PanelKept>,
    old: Option<&mut PanelOld>,
) -> (super::Panels, Option<PanelKept>) {
    let skin = crate::skin::loaded();
    let now = PanelKept {
        rows: rows.to_vec(),
        track_h: shapes.track_h.clone(),
        head_h: shapes.head_h.clone(),
        tcp,
        palette: palette.clone(),
        skin,
        font: font.data().data.id(),
    };
    if let (Some(kept), Some(old)) = (kept, old)
        && kept.same_as(&now)
    {
        let panels = super::Panels {
            panel: std::mem::take(&mut old.panel),
            bar: std::mem::take(&mut old.bar),
            spans: std::mem::take(&mut old.spans),
            bar_spans: std::mem::take(&mut old.bar_spans),
            zoom: old.zoom,
        };
        return (panels, Some(now));
    }
    let heights = Heights {
        track: &shapes.track_h,
        head: &shapes.head_h,
    };
    (
        record_panel(palette, font, rows, heights, 1.0, tcp),
        Some(now),
    )
}

impl PanelKept {
    fn same_as(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.track_h == other.track_h
            && self.head_h == other.head_h
            && self.tcp == other.tcp
            && self.font == other.font
            && self.palette == other.palette
            && match (&self.skin, &other.skin) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
