//! Building facts: the adapters on the input side of the seam.
//!
//! Two of them, which is what makes the seam real rather than
//! hypothetical: the golden fixture's own tree, and a live track list
//! with the template's ext-state read back beside it. The engine never
//! learns which one it is talking to.

use std::collections::HashMap;

use daw_proto::Track;

use super::facts::{path_of, Fact, Segment};
use super::language::{classify_language, Language};
use crate::golden_session::kind::TrackExt;
use crate::golden_session::rpp::Flat;
use crate::golden_session::Kind;
use crate::track_schema::{self, TrackDimension};

/// One half of a stereo pair, which the engine always rails.
///
/// A pair — OH, Rooms, a piano — is ONE stereo track
/// (`flow.scenes.reaper-model`); a session that has split it into an L
/// and an R has two rows for one signal, and the pair's folder is what
/// carries the width.
#[must_use]
pub fn is_pair_half(name: &str) -> bool {
    matches!(
        name.trim().to_uppercase().as_str(),
        "L" | "R" | "LEFT" | "RIGHT"
    )
}

/// Facts from the golden session's own tree.
///
/// The authoritative source: the fixture tree carries every kind and
/// every template group directly, with no parse in between, which is
/// what makes the fixture test a test of the ENGINE rather than of a
/// project-file reader.
#[must_use]
pub fn from_flat(flats: &[Flat]) -> Vec<Fact> {
    let mut walk = Walk::default();
    flats
        .iter()
        .enumerate()
        .map(|(index, flat)| {
            let template: Option<Vec<String>> = flat
                .template
                .map(|path| path.iter().map(|s| (*s).to_owned()).collect());
            let mut fact = walk.step(
                index,
                &Node {
                    guid: &flat.guid,
                    name: &flat.name,
                    depth: flat.depth,
                    is_folder: flat.is_folder,
                    kind: Some(flat.kind),
                    template: template.as_deref(),
                },
            );
            fact.mixer_only = flat.mixer_only;
            fact
        })
        .collect()
}

/// Facts from a live track list, with the template's ext-state beside
/// it.
///
/// `ext` is what [`crate::golden_session::read_kinds`] read back from
/// the project file, keyed by GUID. A track it says nothing about is a
/// track the user added: it has no kind and no template group, which is
/// exactly when a selector's `name` is the right escape hatch.
///
/// `sends` is a source track's cue send, by GUID of the destination
/// track it sends to — how a live caller (a REAPER routing read, a
/// standalone in-memory model, or a hand-built test) tells this adapter
/// what `flow.scenes.performer-identity` calls "the send". An empty map
/// is a session apply has not wired cue sends into yet (#55/#61): every
/// track then falls back to its Performer dimension alone.
#[must_use]
pub fn from_tracks<S: std::hash::BuildHasher>(
    rows: &[(Track, u32)],
    ext: &HashMap<String, TrackExt, S>,
    sends: &HashMap<String, String, S>,
) -> Vec<Fact> {
    let mut walk = Walk::default();
    rows.iter()
        .enumerate()
        .map(|(index, (track, depth))| {
            let known = ext.get(&track.guid);
            let template: Option<Vec<String>> = known
                .and_then(|e| e.group.as_deref())
                .map(|g| g.split('/').map(str::to_owned).collect());
            let mut fact = walk.step(
                index,
                &Node {
                    guid: &track.guid,
                    name: &track.name,
                    depth: *depth,
                    is_folder: track.is_folder,
                    kind: known.and_then(|e| e.kind),
                    template: template.as_deref(),
                },
            );
            // A folder's own name is a structural label, not a
            // performer assignment — "Toms" (the piece) reads as the
            // performer "Tom" under the same name parser that reads a
            // source track's own name, which is exactly the false
            // identity this guards against. Only a track that is
            // actually a signal — a leaf — can belong to someone.
            if !track.is_folder {
                fact.performer = performer_of(&track.guid, &track.name, ext, sends);
            }
            fact.mixer_only = !track.visible_in_tcp && track.visible_in_mixer;
            fact
        })
        .collect()
}

/// Which performer a track belongs to (`flow.scenes.performer-identity`,
/// decision #31): its own Performer dimension when its name carries one,
/// else the performer whose headphone bus it cue-sends to, else
/// unassigned.
///
/// r[impl flow.scenes.performer-order]
fn performer_of<S: std::hash::BuildHasher>(
    guid: &str,
    name: &str,
    ext: &HashMap<String, TrackExt, S>,
    sends: &HashMap<String, String, S>,
) -> Option<String> {
    if let Some(named) = track_schema::dimension_value(name, &[], TrackDimension::Performer) {
        return Some(named);
    }
    let dest_guid = sends.get(guid)?;
    let dest = ext.get(dest_guid)?;
    if dest.kind != Some(Kind::Headphones) {
        return None;
    }
    performer_from_headphone_bus_name(&dest.name)
}

/// The performer a headphone bus's name names.
///
/// The template's own convention (`groups/headphones.rs`): `HP
/// <performer>`. A bus for a group (`HP Choir`) names the group rather
/// than one performer, and is returned the same way — identity is a
/// name, whichever it turns out to be.
#[must_use]
fn performer_from_headphone_bus_name(name: &str) -> Option<String> {
    let trimmed = name.trim();
    let prefix = trimmed.get(..2)?;
    if !prefix.eq_ignore_ascii_case("hp") {
        return None;
    }
    let rest = trimmed
        .get(2..)?
        .trim_start_matches([' ', '-', '_', ':'])
        .trim();
    (!rest.is_empty()).then(|| rest.to_owned())
}

/// One track as the walk sees it, before its path is worked out.
struct Node<'a> {
    guid: &'a str,
    name: &'a str,
    depth: u32,
    is_folder: bool,
    kind: Option<Kind>,
    template: Option<&'a [String]>,
}

/// The tree walk both adapters share: it remembers the path of each
/// open folder so a track's own path is its parent's plus its own step,
/// and separately the language of each open Language folder so a
/// source under one inherits it without repeating the name.
#[derive(Default)]
struct Walk {
    /// `(depth, path)` for every folder currently open.
    open: Vec<(u32, Vec<Segment>)>,
    /// `(depth, language)` for every open folder that IS a language —
    /// `flow.vocals.language`'s folder-inheritance case.
    language_open: Vec<(u32, Language)>,
}

impl Walk {
    fn step(&mut self, index: usize, node: &Node<'_>) -> Fact {
        let Node {
            guid,
            name,
            depth,
            is_folder,
            kind,
            template,
        } = *node;
        self.open.retain(|(at, _)| *at < depth);
        self.language_open.retain(|(at, _)| *at < depth);
        let parent: Vec<Segment> = self
            .open
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        let mut own = Segment::named(name);
        own.kind = kind;
        if let Some(last) = template.and_then(<[String]>::last) {
            own.template = Some(last.clone());
        }
        let path = path_of(&parent, Some(&own), template);
        if is_folder {
            self.open.push((depth, path.clone()));
        }
        // The track's own name says a language first; failing that, the
        // nearest enclosing Language folder does.
        // r[impl flow.vocals.language]
        let own_language = classify_language(name);
        let language = own_language.or_else(|| self.language_open.last().map(|(_, l)| *l));
        if is_folder {
            if let Some(own_language) = own_language {
                self.language_open.push((depth, own_language));
            }
        }
        Fact {
            guid: guid.to_owned(),
            name: name.to_owned(),
            index: u32::try_from(index).unwrap_or(u32::MAX),
            depth,
            is_folder,
            kind,
            // A leaf does not extend the path it sits in: its own step
            // is its kind and its name, which the selector matches
            // directly rather than through the path.
            path: if is_folder { path } else { parent },
            performer: None,
            layer: None,
            channel: None,
            multi_mic: None,
            arrangement: None,
            language,
            pair_half: is_pair_half(name),
            mixer_only: false,
        }
    }
}
