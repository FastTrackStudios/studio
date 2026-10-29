//! One peer's engine, kept in step with the shared doc.
//!
//! The bridge remembers the model the engine last showed. A local edit is
//! a difference between that and a fresh read of the engine, and goes into
//! the doc; a remote edit is a difference between that and the doc, and
//! goes into the engine. Either way the remembered model moves to the
//! engine's new state, so the engine never hears its own edit back and the
//! doc never records a remote edit twice.
//!
//! Undo is the doc's too ([`Bridge::undo`]): this peer's own steps,
//! undone in the doc — where another peer's edits since stay as they are —
//! and the engine made to match.

use daw_control::Project;

use crate::sync::align;
use crate::sync::diff::{Change, diff};
use crate::sync::doc::{ORIGIN_LOCAL, SessionDoc, Touched, Watch};
use crate::sync::engine::{self, Blobs, Direct, EngineIds, MediaRoot};
use crate::sync::model::SessionModel;

pub struct Bridge {
    doc: SessionDoc,
    project: Project,
    media: MediaRoot,
    /// Where FX state is kept (the project's store).
    blobs: std::sync::Arc<dyn Blobs>,
    /// Which engine FX and item each doc FX and item is, here.
    ids: EngineIds,
    /// The engine's own services, when it runs in this process.
    direct: Option<std::sync::Arc<dyn Direct>>,
    /// What the engine showed after the last reconcile, chart included.
    shown: SessionModel,
    /// This peer's steps, to undo and redo — from when the bridge began,
    /// so opening a song is not a step.
    undo: loro::UndoManager,
    /// The name the next step pushed onto either stack is given.
    naming: std::sync::Arc<std::sync::Mutex<String>>,
    /// What changes to the doc have touched since the engine last matched
    /// it — so a remote edit or an undo reads back only that.
    watch: Watch,
    /// Steps the engine finished since this peer's last recorded one that
    /// changed nothing the doc holds, by name, last on top. Each is undone
    /// first, as nothing: an undo never reaches past a step the person
    /// took to the one before it.
    quiet: Vec<String>,
}

/// An undo manager on `doc` whose steps carry the name `naming` holds
/// when each is pushed.
fn undo_manager(doc: &SessionDoc) -> (loro::UndoManager, std::sync::Arc<std::sync::Mutex<String>>) {
    let naming = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut undo = loro::UndoManager::new(doc.loro());
    undo.set_max_undo_steps(UNDO_STEPS);
    let named = std::sync::Arc::clone(&naming);
    undo.set_on_push(Some(Box::new(move |_, _, _| {
        let name = named.lock().map(|n| n.clone()).unwrap_or_default();
        loro::undo::UndoItemMeta {
            value: loro::LoroValue::from(name),
            cursors: Vec::new(),
        }
    })));
    (undo, naming)
}

/// How many steps back a song can be undone.
const UNDO_STEPS: usize = 200;

fn name_of(value: Option<loro::LoroValue>) -> String {
    match value {
        Some(loro::LoroValue::String(name)) => name.to_string(),
        _ => String::new(),
    }
}

impl Bridge {
    /// Start a session from this engine: the engine is the truth, the doc
    /// is written from it. `chart` is the song's chart text.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn host(
        project: Project,
        media: MediaRoot,
        doc: SessionDoc,
        chart: String,
        blobs: std::sync::Arc<dyn Blobs>,
        direct: Option<std::sync::Arc<dyn Direct>>,
    ) -> eyre::Result<Self> {
        let ids = EngineIds::default();
        let mut shown =
            engine::read(&project, &media, blobs.as_ref(), &ids, direct.as_deref()).await?;
        shown.chart = chart;
        doc.write(&shown, ORIGIN_LOCAL)?;
        let (undo, naming) = undo_manager(&doc);
        let watch = doc.watch();
        let bridge = Self {
            doc,
            project,
            media,
            blobs,
            ids,
            direct,
            shown,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
        };
        // What the engine showed is what the doc now holds.
        bridge.take_touched();
        Ok(bridge)
    }

    /// Join a session: `doc` already holds the host's state (imported
    /// before calling this), and the engine is made to match it —
    /// whatever it had open is replaced, never written into the doc.
    ///
    /// What the engine already holds as the doc does is kept, not made
    /// again: its FX and MIDI items under the engine's own guids are
    /// taken for the doc's ([`Self::adopt`]), so a song built from its
    /// file and joined to a live set holding the same song is built once.
    ///
    /// # Errors
    /// When the engine cannot be read or an engine call fails.
    pub async fn join(
        project: Project,
        media: MediaRoot,
        doc: SessionDoc,
        blobs: std::sync::Arc<dyn Blobs>,
        direct: Option<std::sync::Arc<dyn Direct>>,
    ) -> eyre::Result<Self> {
        use tracing::Instrument as _;
        // Each step its own span, under the song's build: where a join's
        // time goes.
        let ids = EngineIds::default();
        let current = engine::read(&project, &media, blobs.as_ref(), &ids, direct.as_deref())
            .instrument(tracing::info_span!("bridge.read_engine"))
            .await?;
        let (undo, naming) = tracing::info_span!("bridge.undo").in_scope(|| undo_manager(&doc));
        let watch = doc.watch();
        let mut bridge = Self {
            doc,
            project,
            media,
            blobs,
            ids,
            direct,
            shown: current,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
        };
        // The whole doc, whatever the watch has seen so far.
        tracing::info_span!("bridge.take_touched").in_scope(|| bridge.take_touched());
        let touched = Touched::everything();
        let target = tracing::info_span!("bridge.read_doc")
            .in_scope(|| bridge.doc.read_touched(&bridge.shown, &touched));
        tracing::info_span!("bridge.adopt").in_scope(|| bridge.adopt(&target));
        bridge
            .make_match(target, &touched)
            .instrument(tracing::info_span!(
                "bridge.make_match",
                bridge.changes = tracing::field::Empty
            ))
            .await?;
        Ok(bridge)
    }

    /// This bridge's song, kept in step with `doc` from now on — a song
    /// built from its file and then joined to a live set's doc of it. What
    /// the bridge knows of the engine (what it showed, which engine FX and
    /// item each entry is) holds, so the engine is not read again; it is
    /// made to match `doc` as [`Self::join`] makes it, and what it already
    /// holds as the doc does is kept, not made again. Undo starts here.
    ///
    /// # Errors
    /// When an engine call fails.
    pub async fn rejoin(self, doc: SessionDoc) -> eyre::Result<Self> {
        let (undo, naming) = undo_manager(&doc);
        let watch = doc.watch();
        let mut bridge = Self {
            doc,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
            ..self
        };
        bridge.take_touched();
        let touched = Touched::everything();
        let target = bridge.doc.read_touched(&bridge.shown, &touched);
        bridge.adopt(&target);
        bridge.make_match(target, &touched).await?;
        Ok(bridge)
    }

    /// Take what the engine shows as `target`'s where the two differ only
    /// in what each calls an FX or an item ([`align::renames`]): the
    /// engine's guids bound to the doc's ids, and what it showed renamed.
    fn adopt(&mut self, target: &SessionModel) {
        let renames = align::renames(&self.shown, target);
        self.ids.rename(&renames);
        renames.apply(&mut self.shown);
    }

    /// This bridge, kept on `project` from now on: the same song moved
    /// whole into another engine (a song built on a
    /// [`daw_standalone::sync::Stage`] and landed), under the same guids —
    /// so what the bridge knows of it (what the engine showed, which
    /// engine FX and item each doc entry is, its undo steps and what the
    /// doc has touched) holds there as it did here, and nothing is
    /// applied or read again.
    #[must_use]
    pub fn moved_to(self, project: Project, direct: Option<std::sync::Arc<dyn Direct>>) -> Self {
        Self {
            project,
            direct,
            ..self
        }
    }

    /// Join a session while this device is locked ([`crate::sync::Hold`]):
    /// as [`Self::join`], except the engine is left as it is — the doc's
    /// state waits for [`Self::merge_held`]. Until then the engine is what
    /// this bridge last showed, so an edit made here meanwhile is recorded
    /// as an edit (only what it changed), not as the whole engine.
    ///
    /// At the merge the doc wins for anything this device did not change
    /// after joining: what the engine held before it joined is replaced,
    /// as an unlocked join replaces it.
    ///
    /// # Errors
    /// When the engine cannot be read.
    pub async fn join_held(
        project: Project,
        media: MediaRoot,
        doc: SessionDoc,
        blobs: std::sync::Arc<dyn Blobs>,
        direct: Option<std::sync::Arc<dyn Direct>>,
    ) -> eyre::Result<Self> {
        let ids = EngineIds::default();
        let current =
            engine::read(&project, &media, blobs.as_ref(), &ids, direct.as_deref()).await?;
        let (undo, naming) = undo_manager(&doc);
        // Starts as "everything touched": the merge reads the doc whole.
        let watch = doc.watch();
        let target = doc.read();
        let mut bridge = Self {
            doc,
            project,
            media,
            blobs,
            ids,
            direct,
            shown: current,
            undo,
            naming,
            watch,
            quiet: Vec::new(),
        };
        // What it holds as the doc does is not an edit made here.
        bridge.adopt(&target);
        Ok(bridge)
    }

    /// The device was unlocked: merge what the lock held. This engine's
    /// own edits since the lock go into the doc first, as one step named
    /// `label` — only what they changed, so the others' edits already in
    /// the doc stay — then the engine is made to match the doc, which now
    /// holds both. Returns the steps the engine took, as
    /// [`Self::remote_changed`] does.
    ///
    /// # Errors
    /// When the engine cannot be read, the doc written, or an engine call
    /// fails.
    pub async fn merge_held(&mut self, label: &str) -> eyre::Result<Vec<Change>> {
        self.local_step(label).await?;
        self.remote_changed().await
    }

    /// The shared doc — for the sync driver to import into and export from.
    #[must_use]
    pub const fn doc(&self) -> &SessionDoc {
        &self.doc
    }

    /// What the engine showed at the last reconcile.
    #[must_use]
    pub const fn shown(&self) -> &SessionModel {
        &self.shown
    }

    /// The engine may have changed locally: read it, and record any
    /// difference in the doc. Returns whether anything was recorded.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn local_changed(&mut self) -> eyre::Result<bool> {
        let mut now = engine::read(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
        )
        .await?;
        now.chart.clone_from(&self.shown.chart);
        if now == self.shown {
            return Ok(false);
        }
        self.name_next("Edits");
        self.doc.write(&now, ORIGIN_LOCAL)?;
        self.shown = now;
        self.quiet.clear();
        Ok(true)
    }

    /// The engine finished an edit step named `label`: read it, and record
    /// what changed as one commit of that name — only what differs
    /// written. Returns whether anything was recorded.
    ///
    /// A step that changed nothing the doc holds (the doc holds the whole
    /// project, so: nothing a collaborator or an undo could see) records
    /// no commit — there is nothing to send anyone — but is kept as a
    /// quiet step: the next [`Self::undo`] undoes it, as nothing, rather
    /// than the step before it.
    ///
    /// # Errors
    /// When the engine cannot be read or the doc cannot be written.
    pub async fn local_step(&mut self, label: &str) -> eyre::Result<bool> {
        use tracing::Instrument as _;
        let mut now = engine::read(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
        )
        .instrument(tracing::info_span!("bridge.read_engine"))
        .await?;
        now.chart.clone_from(&self.shown.chart);
        let same = tracing::info_span!("bridge.compare").in_scope(|| now == self.shown);
        if same {
            if self.quiet.len() >= UNDO_STEPS {
                self.quiet.remove(0);
            }
            self.quiet.push(label.to_owned());
            return Ok(false);
        }
        self.name_next(label);
        tracing::info_span!("bridge.write_step")
            .in_scope(|| self.doc.write_step(&self.shown, &now, ORIGIN_LOCAL, label))?;
        self.shown = now;
        self.quiet.clear();
        Ok(true)
    }

    /// Undo this peer's last step: in the doc, then in the engine. Returns
    /// its name, or `None` with nothing to undo.
    ///
    /// # Errors
    /// When the doc cannot undo or the engine cannot be made to match.
    pub async fn undo(&mut self) -> eyre::Result<Option<String>> {
        // A step that changed nothing is undone as nothing.
        if let Some(name) = self.quiet.pop() {
            return Ok(Some(name));
        }
        if !self.undo.can_undo() {
            return Ok(None);
        }
        let name = name_of(self.undo.top_undo_value());
        // Its redo goes by the same name.
        self.name_next(&name);
        self.undo.undo()?;
        self.remote_changed().await?;
        Ok(Some(name))
    }

    /// Redo the step last undone: in the doc, then in the engine. Returns
    /// its name, or `None` with nothing to redo.
    ///
    /// # Errors
    /// When the doc cannot redo or the engine cannot be made to match.
    pub async fn redo(&mut self) -> eyre::Result<Option<String>> {
        if !self.undo.can_redo() {
            return Ok(None);
        }
        let name = name_of(self.undo.top_redo_value());
        self.name_next(&name);
        self.undo.redo()?;
        self.remote_changed().await?;
        Ok(Some(name))
    }

    /// The name of the step that would be undone next, if any.
    #[must_use]
    pub fn next_undo(&self) -> Option<String> {
        self.quiet.last().cloned().or_else(|| {
            self.undo
                .can_undo()
                .then(|| name_of(self.undo.top_undo_value()))
        })
    }

    /// The name of the step that would be redone next, if any.
    #[must_use]
    pub fn next_redo(&self) -> Option<String> {
        self.undo
            .can_redo()
            .then(|| name_of(self.undo.top_redo_value()))
    }

    fn name_next(&self, name: &str) {
        if let Ok(mut naming) = self.naming.lock() {
            name.clone_into(&mut naming);
        }
    }

    /// The chart was edited locally.
    ///
    /// # Errors
    /// When the doc cannot be written.
    pub fn chart_changed(&mut self, chart: &str) -> eyre::Result<()> {
        if self.shown.chart == chart {
            return Ok(());
        }
        self.shown.chart = chart.to_string();
        self.name_next("Edit chart");
        self.doc.write(&self.shown, ORIGIN_LOCAL)?;
        self.quiet.clear();
        Ok(())
    }

    /// The doc changed remotely: make the engine match it. Returns the
    /// steps taken — a caller redraws from them, and a `ChartChanged`
    /// among them is the chart editor's to show.
    ///
    /// # Errors
    /// When an engine call fails. The engine is re-read either way, so a
    /// half-applied change is reconciled on the next call.
    pub async fn remote_changed(&mut self) -> eyre::Result<Vec<Change>> {
        let touched = self.take_touched();
        if touched.is_empty() {
            return Ok(Vec::new());
        }
        let target = self.doc.read_touched(&self.shown, &touched);
        self.make_match(target, &touched).await
    }

    /// Make the engine match `target`, the doc as read for `touched`.
    async fn make_match(
        &mut self,
        target: SessionModel,
        touched: &Touched,
    ) -> eyre::Result<Vec<Change>> {
        use tracing::Instrument as _;
        let changes = tracing::info_span!("bridge.diff").in_scope(|| diff(&self.shown, &target));
        tracing::Span::current().record("bridge.changes", changes.len());
        if changes.is_empty() {
            return Ok(changes);
        }
        let apply = tracing::info_span!("bridge.apply", bridge.changes = changes.len());
        let applied = engine::apply(
            &self.project,
            &self.media,
            &changes,
            &target,
            self.blobs.as_ref(),
            &mut self.ids,
            self.direct.as_deref(),
        )
        .instrument(apply)
        .await;
        // The tracks read again only if a change could have touched them.
        let keep =
            (!touched.everything && !touched.parts.tracks).then(|| self.shown.tracks.clone());
        let mut now = engine::read_keeping(
            &self.project,
            &self.media,
            self.blobs.as_ref(),
            &self.ids,
            self.direct.as_deref(),
            keep,
        )
        .instrument(tracing::info_span!("bridge.read_back"))
        .await?;
        now.chart = target.chart;
        self.shown = now;
        applied?;
        Ok(changes)
    }

    /// What the doc's changes have touched since last asked, cleared.
    fn take_touched(&self) -> Touched {
        self.watch
            .touched
            .lock()
            .map_or_else(|_| Touched::everything(), |mut t| std::mem::take(&mut *t))
    }
}

#[cfg(test)]
mod tests;
