//! UI-owned editor session with a coalescing theme compilation worker.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;

use figcraft_engine::{EngineError, Session};
use frame::theme::host::ThemeHost;
use frame_doc::doc::Document;
use frame_doc::doc::NodeId;
use serde_json::Value;

use crate::save::{SaveError, SaveSnapshot};
use crate::theme::{self, CompileError};

/// Completion of a requested preview. Poll this from the UI thread.
#[derive(Debug)]
pub struct PreviewEvent {
    pub request: u64,
    pub result: Result<(), CompileError>,
}

struct Job {
    request: u64,
    document: Arc<Document>,
    mode: Option<String>,
    host_version: u64,
}

#[derive(Default)]
struct Queue {
    latest: u64,
    base_version: u64,
    pending: Option<Job>,
    closed: bool,
    active: bool,
    saving: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

/// Owned by a view on its UI thread; compilation runs on one worker thread.
/// The caller supplies the document and host; no file is loaded or saved here.
pub struct LiveEditor {
    session: Session,
    host: Arc<ThemeHost>,
    queue: Shared,
    events: mpsc::Receiver<PreviewEvent>,
    sender: mpsc::Sender<PreviewEvent>,
    mode: Option<String>,
    save_snapshot: Option<Arc<SaveSnapshot>>,
    save_receiver: Option<mpsc::Receiver<SaveDone>>,
}

/// Opening a snapshot failed before the editor was created.
#[derive(Debug)]
pub enum OpenError {
    NoTheme,
    Document(frame_doc::Error),
    /// The host changed while the snapshot was being prepared.
    Superseded,
    /// The opening worker exited without returning a result.
    WorkerStopped,
    SaveSnapshot(SaveError),
}

struct Prepared {
    session: Session,
    version: u64,
    mode: Option<String>,
    save_snapshot: Arc<SaveSnapshot>,
}

struct SaveDone {
    result: Result<(), SaveError>,
    snapshot: Option<SaveSnapshot>,
    version: Option<u64>,
}

/// A background opening request. Poll on the UI thread without blocking.
pub struct OpeningEditor {
    host: Arc<ThemeHost>,
    receiver: Option<mpsc::Receiver<Result<Prepared, OpenError>>>,
}

impl OpeningEditor {
    /// Return the completed editor or error once, or `None` while opening.
    pub fn try_finish(&mut self) -> Option<Result<LiveEditor, OpenError>> {
        let result = match self.receiver.as_ref()?.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => Err(OpenError::WorkerStopped),
        };
        self.receiver = None;
        Some(result.and_then(|prepared| {
            if self.host.version() != prepared.version {
                return Err(OpenError::Superseded);
            }
            Ok(LiveEditor::with_session(
                prepared.session,
                self.host.clone(),
                prepared.version,
                prepared.mode,
                Some(prepared.save_snapshot),
            ))
        }))
    }
}

impl LiveEditor {
    /// Open an existing document on the same host used by the playing view.
    #[must_use]
    pub fn new(document: Document, host: Arc<ThemeHost>) -> Self {
        let version = host.version();
        Self::with_snapshot(document, host, version, None)
    }

    /// Open exactly the displayed merged theme, including its active mode.
    /// This converts synchronously; UI code should use [`Self::open_async`].
    /// The document is a flattened in-memory snapshot: edits do not map back
    /// to the original layers and this controller does not save them.
    ///
    /// # Errors
    /// The host has no theme, or its scene cannot become a document.
    pub fn from_host(host: Arc<ThemeHost>) -> Result<Self, OpenError> {
        let prepared = prepare(&host)?;
        if host.version() != prepared.version {
            return Err(OpenError::Superseded);
        }
        Ok(Self::with_session(
            prepared.session,
            host,
            prepared.version,
            prepared.mode,
            Some(prepared.save_snapshot),
        ))
    }

    /// Start snapshot conversion on a worker; call [`OpeningEditor::try_finish`]
    /// from the UI thread. This does not load files or save the resulting edits.
    #[must_use]
    pub fn open_async(host: Arc<ThemeHost>) -> OpeningEditor {
        let (sender, receiver) = mpsc::channel();
        let worker_host = host.clone();
        thread::spawn(move || {
            let _ = sender.send(prepare(&worker_host));
        });
        OpeningEditor {
            host,
            receiver: Some(receiver),
        }
    }

    fn with_snapshot(
        document: Document,
        host: Arc<ThemeHost>,
        version: u64,
        mode: Option<String>,
    ) -> Self {
        Self::with_session(Session::with_document(document), host, version, mode, None)
    }

    fn with_session(
        session: Session,
        host: Arc<ThemeHost>,
        version: u64,
        mode: Option<String>,
        save_snapshot: Option<Arc<SaveSnapshot>>,
    ) -> Self {
        let queue = Arc::new((
            Mutex::new(Queue {
                base_version: version,
                ..Queue::default()
            }),
            Condvar::new(),
        ));
        let (sender, events) = mpsc::channel();
        let (worker_queue, worker_host, worker_sender) =
            (queue.clone(), host.clone(), sender.clone());
        thread::spawn(move || worker(worker_queue, worker_host, worker_sender));
        Self {
            session,
            host,
            queue,
            events,
            sender,
            mode,
            save_snapshot,
            save_receiver: None,
        }
    }

    /// Read editor state for drawing and selection.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Run an editor command and request a preview when it changes the document.
    ///
    /// # Errors
    /// The command or document lookup failed.
    pub fn execute(
        &mut self,
        id: &str,
        args: &Value,
        mode: Option<&str>,
    ) -> Result<Value, EngineError> {
        if self
            .queue
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .saving
        {
            return Err(EngineError::Other("save is in progress".to_owned()));
        }
        let before = self.session.ds()?.revision;
        let result = self.session.execute(id, args)?;
        if self.session.ds()?.revision != before {
            self.request_preview(mode)?;
        }
        Ok(result)
    }

    /// Set a node's literal fill, replacing its theme fill binding if present.
    /// The fill and binding change form one engine command and undo step.
    ///
    /// # Errors
    /// The node data is malformed or the editor rejects the fill or command.
    pub fn set_fill(&mut self, id: NodeId, fill: &str) -> Result<(), EngineError> {
        let data = frame_doc::effective(self.session.doc()?, id);
        if !data.errors.is_empty() {
            return Err(EngineError::Other(format!(
                "invalid Frame data: {:?}",
                data.errors
            )));
        }
        let binds: Vec<_> = data.bind.into_iter().filter(|b| b.prop != "fill").collect();
        let binding = if binds.is_empty() {
            Value::Null
        } else {
            serde_json::json!(binds)
        };
        let args = serde_json::json!({"id": id, "props": {"fill": fill, "pluginData": {(frame_doc::keys::BIND): binding}}});
        self.execute("node.set", &args, None)?;
        Ok(())
    }

    /// Mutate through pointer tools or other engine APIs, then call
    /// [`Self::request_preview`] after a document edit.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Queue the current snapshot for compilation, replacing a pending snapshot.
    /// This only snapshots and queues; it never compiles on the caller thread.
    ///
    /// # Errors
    /// The session has no active document.
    pub fn request_preview(&mut self, mode: Option<&str>) -> Result<u64, EngineError> {
        let document = self.session.ds()?.document_itself().clone();
        let (lock, wake) = &*self.queue;
        let mut queue = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.saving {
            return Err(EngineError::Other("save is in progress".to_owned()));
        }
        queue.latest += 1;
        let request = queue.latest;
        let job = Job {
            request,
            document,
            mode: mode.map(str::to_owned).or_else(|| self.mode.clone()),
            host_version: queue.base_version,
        };
        if let Some(old) = queue.pending.replace(job) {
            let _ = self.sender.send(PreviewEvent {
                request: old.request,
                result: Err(CompileError::Superseded),
            });
        }
        wake.notify_one();
        Ok(request)
    }

    /// Return the next completed preview without waiting for the worker.
    pub fn try_event(&self) -> Option<PreviewEvent> {
        self.events.try_recv().ok()
    }

    /// Source files available as explicit save targets, in layer order.
    #[must_use]
    pub fn layer_files(&self) -> Vec<PathBuf> {
        self.save_snapshot
            .as_ref()
            .map(|snapshot| snapshot.files().to_vec())
            .unwrap_or_default()
    }

    /// Whether another publisher replaced the theme this editor opened.
    #[must_use]
    pub fn host_changed(&self) -> bool {
        let state = self
            .queue
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.base_version != self.host.version()
    }

    /// Save the selected role to an existing source file on a worker. Edits
    /// and preview requests must be idle; the UI must poll `try_save_event`.
    ///
    /// # Errors
    /// The editor is busy, has no source snapshot, or its document is unavailable.
    pub fn request_save(&mut self, role: String, target: PathBuf) -> Result<(), SaveError> {
        let snapshot = self.save_snapshot.clone().ok_or(SaveError::NoSnapshot)?;
        let document = self
            .session
            .ds()
            .map_err(|_| SaveError::NoSnapshot)?
            .document_itself()
            .clone();
        let mut queue = self
            .queue
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if queue.saving || queue.active || queue.pending.is_some() {
            return Err(SaveError::Busy);
        }
        if self.host.version() != queue.base_version {
            return Err(SaveError::HostChanged);
        }
        queue.saving = true;
        let version = queue.base_version;
        let host = self.host.clone();
        let (sender, receiver) = mpsc::channel();
        self.save_receiver = Some(receiver);
        thread::spawn(move || {
            let result = snapshot.save_role(&host, &document, &role, &target, version);
            let staged = match &result {
                Ok(()) => Some(version.wrapping_add(1)),
                Err(SaveError::Write { staged_version, .. }) => Some(*staged_version),
                _ => None,
            };
            let fresh = staged.and_then(|v| {
                (host.version() == v)
                    .then(|| SaveSnapshot::refresh(&host, &document).ok())
                    .flatten()
            });
            let _ = sender.send(SaveDone {
                result,
                snapshot: fresh,
                version: staged,
            });
        });
        Ok(())
    }

    /// Return the save outcome once without blocking the UI.
    pub fn try_save_event(&mut self) -> Option<Result<(), SaveError>> {
        let done = match self.save_receiver.as_ref()?.try_recv() {
            Ok(done) => done,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => SaveDone {
                result: Err(SaveError::WorkerStopped),
                snapshot: None,
                version: None,
            },
        };
        self.save_receiver = None;
        let mut queue = self
            .queue
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(version) = done.version {
            queue.base_version = version;
        }
        queue.saving = false;
        if let Some(snapshot) = done.snapshot {
            self.save_snapshot = Some(Arc::new(snapshot));
        }
        Some(done.result)
    }
}

fn prepare(host: &ThemeHost) -> Result<Prepared, OpenError> {
    let (theme, version) = host.snapshot().ok_or(OpenError::NoTheme)?;
    let mut document = frame_doc::scene_to_doc(&theme.source()).map_err(OpenError::Document)?;
    // Settle imported dirty nodes before the baseline; otherwise the first edit
    // also normalizes unrelated text and appears to change other roles.
    let before = document.clone();
    figcraft_engine::normalize(&mut document, &before);
    let session = Session::with_document(document);
    let document = session
        .ds()
        .map_err(|_| OpenError::SaveSnapshot(SaveError::NoSnapshot))?;
    let save_snapshot = Arc::new(
        SaveSnapshot::refresh(host, document.document_itself()).map_err(OpenError::SaveSnapshot)?,
    );
    if save_snapshot.version() != version {
        return Err(OpenError::Superseded);
    }
    Ok(Prepared {
        session,
        version,
        mode: Some(theme.mode().to_owned()),
        save_snapshot,
    })
}

impl Drop for LiveEditor {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        let mut queue = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        queue.closed = true;
        queue.pending = None;
        wake.notify_one();
    }
}

fn worker(queue: Shared, host: Arc<ThemeHost>, sender: mpsc::Sender<PreviewEvent>) {
    while let Some(job) = next_job(&queue) {
        let compiled = theme::compile(&job.document, host.schema(), job.mode.as_deref());
        let (lock, _) = &*queue;
        let mut state = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return;
        }
        state.active = false;
        let result = if state.latest != job.request {
            Err(CompileError::Superseded)
        } else if host.version() != job.host_version {
            Err(CompileError::HostChanged)
        } else {
            compiled.and_then(|theme| {
                host.set_if_version(job.host_version, theme)
                    .then_some(())
                    .ok_or(CompileError::HostChanged)
            })
        };
        if result.is_ok() {
            state.base_version = job.host_version.wrapping_add(1);
        }
        let _ = sender.send(PreviewEvent {
            request: job.request,
            result,
        });
    }
}

fn next_job(queue: &Shared) -> Option<Job> {
    let (lock, wake) = &**queue;
    let mut state = lock
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while !state.closed && state.pending.is_none() {
        state = wake
            .wait(state)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    if state.closed {
        None
    } else {
        let job = state.pending.take();
        state.active = job.is_some();
        job
    }
}
