//! [`TemplateTarget`] over an `.RPP` file — the offline backend.
//!
//! Wraps `dawfile_reaper::types::ReaperProject`, so a whole album's worth of
//! sessions can be organized in a batch without REAPER running. The live
//! counterpart is [`super::reaper`]; both are driven by
//! [`apply_buses`](super::apply_buses).
//!
//! # Track ids are indices
//!
//! [`RppTarget::TrackId`] is a position in `ReaperProject::tracks`. That is only
//! stable because this backend exclusively **appends** — inserting in the
//! middle would invalidate every id handed out so far, and REAPER's `AUXRECV`
//! records its source track by index too, so a mid-list insert would silently
//! repoint existing sends. If a reordering pass is ever added it has to fix up
//! both.

use color_palette::Color;
use daw_proto::FolderDepthChange;
use dawfile_reaper::types::track::{
    BusCompactSettings, FolderSettings, FolderState, MasterSendSettings, MuteSoloSettings,
    ReceiveSettings, Track, TrackSoloState,
};
use dawfile_reaper::types::ReaperProject;

use super::{find_group, TemplateTarget};

/// A parsed `.RPP` project the template can be applied to.
pub struct RppTarget<'a> {
    project: &'a mut ReaperProject,
}

impl<'a> RppTarget<'a> {
    /// Borrow `project` as a template target.
    pub const fn new(project: &'a mut ReaperProject) -> Self {
        Self { project }
    }

    /// The wrapped project.
    #[must_use]
    pub const fn project(&self) -> &ReaperProject {
        self.project
    }

    fn track(&mut self, id: usize) -> Option<&mut Track> {
        self.project.tracks.get_mut(id)
    }

    /// Tracks whose folder depth goes negative — each closes a folder that was
    /// never opened.
    ///
    /// REAPER tolerates this by clamping, so a project can carry the damage
    /// invisibly for years, but the nesting it describes is not a tree. Anything
    /// that reasons about folder membership (including
    /// [`gather_into_folder`](TemplateTarget::gather_into_folder), which will
    /// only move genuinely top-level tracks) is working from bad data until it
    /// is repaired.
    #[must_use]
    pub fn negative_depths(&self) -> Vec<(usize, String, i32)> {
        let mut depth: i32 = 0;
        let mut out = Vec::new();
        for (i, track) in self.project.tracks.iter().enumerate() {
            depth = depth.saturating_add(track.folder.as_ref().map_or(0, |f| f.indentation));
            if depth < 0 {
                out.push((i, track.name.clone(), depth));
            }
        }
        out
    }

    /// Folder nesting depth of each track, 0 at the top level.
    ///
    /// REAPER stores nesting as a per-track *change*, so a track's own depth is
    /// the running sum of every change before it — a track with no folder
    /// settings at all can still sit three folders deep.
    fn running_depths(&self) -> Vec<i32> {
        let mut depth: i32 = 0;
        self.project
            .tracks
            .iter()
            .map(|t| {
                let here = depth;
                depth = depth.saturating_add(t.folder.as_ref().map_or(0, |f| f.indentation));
                here
            })
            .collect()
    }
}

/// This backend edits an in-memory project; nothing here can fail, so the
/// error type is uninhabited and `?` on it is free.
#[derive(Debug)]
pub enum Never {}

impl std::fmt::Display for Never {
    // rustc requires the dereference for exhaustiveness — `&T` is always
    // considered inhabited regardless of `T` ("references are always
    // considered inhabited"), so `match self {}` doesn't type-check; only
    // `match *self {}` does, and that's exactly what this lint flags. No
    // rewrite avoids the deref here.
    #[allow(clippy::uninhabited_references)]
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {}
    }
}

impl std::error::Error for Never {}

impl TemplateTarget for RppTarget<'_> {
    /// Nest a "DI" capture under its sibling primary track, for any group
    /// that opts in via [`monarchy::Group::nest_secondary_mics`] — electric
    /// guitar turns this on for its DI feed.
    ///
    /// Only reshapes the exact adjacency the house convention already
    /// tracks in: a "DI" track immediately following, at the same folder
    /// depth, a non-DI sibling. No track changes position, so no `AUXRECV`
    /// index needs rewriting — only folder nesting, mute, and collapse
    /// flags change.
    ///
    /// Idempotent: once nested, "DI" sits inside its sibling rather than
    /// beside it, so a second pass no longer sees them as siblings.
    fn nest_secondary_mics(&mut self) {
        let config = crate::default_config();
        let entries = super::contextual_paths(self);

        for pair in entries.windows(2) {
            let [main, di] = pair else { continue };
            if main.context != di.context {
                continue; // not siblings
            }
            let is_di = |name: &str| name.trim().eq_ignore_ascii_case("di");
            if !is_di(&di.name) || is_di(&main.name) {
                continue;
            }
            let Some(leaf) = di.path.last() else {
                continue;
            };
            if !find_group(&config, leaf).is_some_and(|g| g.nest_secondary_mics) {
                continue;
            }

            let main_is_plain = self.project.tracks.get(main.track).is_some_and(|t| {
                t.folder
                    .as_ref()
                    .is_none_or(|f| f.folder_state == FolderState::Regular && f.indentation == 0)
            });
            let di_indentation = self
                .project
                .tracks
                .get(di.track)
                .and_then(|t| t.folder.as_ref())
                .map_or(0, |f| f.indentation);
            if !main_is_plain || di_indentation > 0 {
                // Not the plain "two siblings" shape this expects — leave
                // whatever unusual structure is already there alone.
                continue;
            }

            if let Some(t) = self.project.tracks.get_mut(main.track) {
                t.folder = Some(FolderSettings {
                    folder_state: FolderState::FolderParent,
                    indentation: 1,
                });
                t.bus_compact = Some(BusCompactSettings {
                    arrange_collapse: 2,
                    mixer_collapse: 2,
                    wiring_collapse: 0,
                    wiring_x: 0,
                    wiring_y: 0,
                });
            }
            if let Some(t) = self.project.tracks.get_mut(di.track) {
                t.folder = Some(FolderSettings {
                    folder_state: FolderState::LastInFolder,
                    // One more level to close than before: the DI folder we
                    // just opened on `main`, on top of whatever it already closed.
                    indentation: di_indentation.saturating_sub(1),
                });
                t.mutesolo = Some(MuteSoloSettings {
                    mute: true,
                    solo: TrackSoloState::NoSolo,
                    solo_defeat: false,
                });
            }
        }
    }

    type TrackId = usize;
    type Error = Never;

    fn find_track(&self, name: &str) -> Option<usize> {
        self.project
            .tracks
            .iter()
            .position(|t| t.name.trim().eq_ignore_ascii_case(name.trim()))
    }

    fn append_track(&mut self, name: &str) -> Result<usize, Never> {
        self.project.tracks.push(Track {
            name: name.to_string(),
            ..Track::default()
        });
        Ok(self.project.tracks.len().saturating_sub(1))
    }

    fn set_folder_depth(&mut self, id: &usize, depth: FolderDepthChange) -> Result<(), Never> {
        // REAPER's ISBUS is (folder_state, indentation): a folder parent is
        // (1, 1), the last track inside n folders is (2, -n), and an ordinary
        // track is (0, 0).
        let settings = match depth {
            FolderDepthChange::Normal => FolderSettings {
                folder_state: FolderState::Regular,
                indentation: 0,
            },
            FolderDepthChange::FolderStart => FolderSettings {
                folder_state: FolderState::FolderParent,
                indentation: 1,
            },
            FolderDepthChange::ClosesLevels(n) => FolderSettings {
                folder_state: FolderState::LastInFolder,
                indentation: i32::from(n),
            },
        };
        if let Some(track) = self.track(*id) {
            track.folder = Some(settings);
        }
        Ok(())
    }

    fn set_color(&mut self, id: &usize, hex: &str) -> Result<(), Never> {
        // An unparseable color leaves the track's own color alone rather than
        // failing the whole apply — the routing matters, the tint does not.
        let Ok(color) = Color::from_hex_str(hex) else {
            return Ok(());
        };
        let native = color.to_reaper_native();
        if let Some(track) = self.track(*id) {
            track.peak_color = Some(native);
        }
        Ok(())
    }

    fn set_channel_count(&mut self, id: &usize, channels: u32) -> Result<(), Never> {
        if let Some(track) = self.track(*id) {
            track.channel_count = channels;
        }
        Ok(())
    }

    fn has_send(&self, source: &usize, dest: &usize) -> bool {
        self.project.tracks.get(*dest).is_some_and(|t| {
            i32::try_from(*source).map_or(false, |src_i32| {
                t.receives.iter().any(|r| r.source_track_index == src_i32)
            })
        })
    }

    fn add_send(&mut self, source: &usize, dest: &usize) -> Result<(), Never> {
        // REAPER stores a send as an AUXRECV on the *destination* track, which
        // names its source by index — there is no send record on the source.
        let receive = ReceiveSettings {
            source_track_index: i32::try_from(*source).unwrap_or(-1),
            mode: 0, // post-fader
            volume: 1.0,
            pan: 0.0,
            mute: false,
            mono_sum: false,
            invert_polarity: false,
            source_audio_channels: 0,
            dest_audio_channels: 0,
            pan_law: -1.0,
            midi_channels: -1, // no MIDI
            automation_mode: -1,
        };
        if let Some(track) = self.track(*dest) {
            track.receives.push(receive);
        }
        Ok(())
    }

    fn set_parent_send(&mut self, id: &usize, enabled: bool) -> Result<(), Never> {
        if let Some(track) = self.track(*id) {
            track.master_send = Some(MasterSendSettings {
                enabled,
                unknown_field_2: 0,
            });
        }
        Ok(())
    }

    fn folder_depths(&self) -> Vec<(usize, String, i32)> {
        self.project
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    i,
                    t.name.clone(),
                    t.folder.as_ref().map_or(0, |f| f.indentation),
                )
            })
            .collect()
    }

    fn gather_into_folder(
        &mut self,
        folder: &str,
        tracks: &[usize],
    ) -> Result<Option<crate::apply::Gathered<usize>>, Never> {
        // Only top-level tracks that carry no folder structure can be pulled
        // out. Three ways a track fails that, each of which breaks the project
        // differently:
        //
        // - a folder parent — moving it orphans its children;
        // - the track that closes a folder — moving it leaves that folder open
        //   over everything after it;
        // - anything nested inside a folder — it belongs to that folder, and
        //   yanking it out silently changes what the folder sums.
        let depths = self.running_depths();
        let mut movable: Vec<usize> = tracks
            .iter()
            .copied()
            .filter(|i| {
                depths.get(*i) == Some(&0)
                    && self.project.tracks.get(*i).is_some_and(|t| {
                        t.folder.as_ref().is_none_or(|f| {
                            f.folder_state == FolderState::Regular && f.indentation == 0
                        })
                    })
            })
            .collect();
        movable.sort_unstable();
        movable.dedup();
        if movable.is_empty() {
            return Ok(None);
        }

        let moving: std::collections::HashSet<usize> = movable.iter().copied().collect();
        let total = self.project.tracks.len();

        // New order: everything staying, then the folder, then the moved
        // tracks in their original relative order.
        let staying: Vec<usize> = (0..total).filter(|i| !moving.contains(i)).collect();

        // old index → new index, for the AUXRECV rewrite below. The folder
        // track is inserted between the two runs, so the moved tracks shift by
        // one extra place.
        let mut remap = vec![usize::MAX; total];
        for (new, old) in staying.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = new;
            }
        }
        let folder_index = staying.len();
        for (offset, old) in movable.iter().enumerate() {
            if let Some(slot) = remap.get_mut(*old) {
                *slot = folder_index.saturating_add(1).saturating_add(offset);
            }
        }

        let mut reordered: Vec<Track> = Vec::with_capacity(total.saturating_add(1));
        let mut taken: Vec<Option<Track>> = std::mem::take(&mut self.project.tracks)
            .into_iter()
            .map(Some)
            .collect();
        for old in &staying {
            reordered.push(
                taken
                    .get_mut(*old)
                    .and_then(Option::take)
                    .unwrap_or_default(),
            );
        }
        reordered.push(Track {
            name: folder.to_string(),
            folder: Some(FolderSettings {
                folder_state: FolderState::FolderParent,
                indentation: 1,
            }),
            ..Track::default()
        });
        for old in &movable {
            let mut track = taken
                .get_mut(*old)
                .and_then(Option::take)
                .unwrap_or_default();
            // These carried no folder structure (checked above); inside the
            // folder they are plain members.
            track.folder = Some(FolderSettings {
                folder_state: FolderState::Regular,
                indentation: 0,
            });
            reordered.push(track);
        }
        // The last member closes the folder we just opened.
        if let Some(last) = reordered.last_mut() {
            last.folder = Some(FolderSettings {
                folder_state: FolderState::LastInFolder,
                indentation: -1,
            });
        }

        // A receive names its source by index, so every one of them has to
        // follow the tracks that moved.
        for track in &mut reordered {
            for receive in &mut track.receives {
                let old = receive.source_track_index;
                if old >= 0 {
                    if let Ok(old_idx) = usize::try_from(old) {
                        if let Some(&mapped) = remap.get(old_idx) {
                            if let Ok(new_idx) = i32::try_from(mapped) {
                                receive.source_track_index = new_idx;
                            }
                        }
                    }
                }
            }
        }

        self.project.tracks = reordered;
        Ok(Some(crate::apply::Gathered {
            folder: folder_index,
            moved: movable
                .iter()
                .map(|old| remap.get(*old).copied().unwrap_or(usize::MAX))
                .collect(),
            skipped: tracks
                .iter()
                .copied()
                .filter(|i| !movable.contains(i))
                .map(|old| remap.get(old).copied().unwrap_or(usize::MAX))
                .collect(),
        }))
    }
}
