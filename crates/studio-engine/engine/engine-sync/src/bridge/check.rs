//! The engine checked against the doc on demand ([`Bridge::check`]):
//! periodically at idle, and when asked (`ENGINE_CHECK`). After every
//! apply the bridge checks what it read back as it goes
//! ([`Bridge::last_apply_drift`]).

use crate::drift::{self, Digest, Drift};
use crate::engine;

use super::Bridge;

/// What the last [`Bridge::check`] saw, so the next one with nothing new
/// since costs nothing: the doc's version and digest, and the engine's
/// revision of the song (every write to it moves it on).
#[derive(Debug, Default)]
pub struct Checked {
    doc: Option<(loro::VersionVector, Digest)>,
    engine_at: Option<u64>,
    matched: bool,
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// The engine holds what the doc says. `read`: the engine was read for
    /// it (`false`: neither had changed since the last check that matched).
    Matched { read: bool },
    /// Where they differ.
    Drifted(Vec<Drift>),
    /// The doc has changes the engine has not been given yet (a remote
    /// edit, an undo): the two are apart on purpose until the bridge
    /// applies them. Not checked.
    Pending,
}

impl Bridge {
    /// Whether the engine still holds what the doc says: the engine read
    /// (only what its journals say changed since the bridge last read it,
    /// unless `whole`), both digested and compared, and, where they
    /// differ, what differs named. Reads only; it changes neither.
    ///
    /// Steps read off the engine must be in the doc before this is asked
    /// (the engine's step writer drained), or a step not yet written reads
    /// as a difference.
    ///
    /// The outcome is recorded on the current span when it declares the
    /// `engine_check.*` fields ([`drift::span`]), and reported
    /// ([`drift::report`]) under `when`.
    ///
    /// # Errors
    /// When the engine cannot be read.
    pub async fn check(
        &self,
        seen: &mut Checked,
        whole: bool,
        when: &'static str,
    ) -> eyre::Result<Check> {
        use tracing::Instrument as _;
        let current = tracing::Span::current();
        let pending = self
            .watch
            .touched
            .lock()
            .map_or(true, |touched| !touched.is_empty());
        if pending {
            current.record("engine_check.skipped", "pending");
            return Ok(Check::Pending);
        }
        let version = self.doc.version();
        let at = self.revision();
        let unchanged = seen.matched
            && !whole
            && at.is_some()
            && seen.engine_at == at
            && seen.doc.as_ref().is_some_and(|(v, _)| *v == version);
        if unchanged {
            current.record("engine_check.skipped", "unchanged");
            current.record("engine_check.ok", true);
            return Ok(Check::Matched { read: false });
        }
        let since = if whole {
            None
        } else {
            self.cursor.map(|cursor| engine::Since {
                before: &self.shown,
                cursor,
            })
        };
        current.record(
            "engine_check.read",
            if since.is_some() { "journal" } else { "whole" },
        );
        let now = engine::read_keeping(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
            None,
            since,
        )
        .instrument(tracing::info_span!("engine_check.read_engine"))
        .await?;
        let engine_digest =
            tracing::info_span!("engine_check.digest").in_scope(|| drift::digest(&now));
        let mut doc_model = None;
        let doc_digest = match &seen.doc {
            Some((v, d)) if *v == version => d.clone(),
            _ => tracing::info_span!("engine_check.read_doc").in_scope(|| {
                let model = self.doc.read();
                let d = drift::digest(&model);
                doc_model = Some(model);
                d
            }),
        };
        let found = if engine_digest == doc_digest {
            Vec::new()
        } else {
            let doc_model = doc_model.unwrap_or_else(|| self.doc.read());
            tracing::info_span!("engine_check.drift").in_scope(|| drift::drift(&now, &doc_model))
        };
        drift::report(when, &found);
        seen.matched = found.is_empty();
        seen.engine_at = at;
        seen.doc = Some((version, doc_digest));
        Ok(if found.is_empty() {
            Check::Matched { read: true }
        } else {
            Check::Drifted(found)
        })
    }

    /// The engine's revision of this song, when it runs in this process:
    /// any write to the project moves it on (the item journals cover only
    /// items and takes). `None`: it cannot be told, and every check reads.
    fn revision(&self) -> Option<u64> {
        self.direct
            .as_ref()
            .and_then(|d| d.standalone())
            .and_then(|sa| sa.read_project(self.project.guid(), |p| p.revision))
    }

    /// Where the engine differed from the doc after the last apply, as
    /// read back then: empty when it took everything it was given.
    #[must_use]
    pub fn last_apply_drift(&self) -> &[Drift] {
        &self.applied_drift
    }
}
