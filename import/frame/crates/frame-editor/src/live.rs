//! UI-owned editor session with a coalescing theme compilation worker.

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;

use figcraft_engine::{EngineError, Session};
use frame::theme::host::ThemeHost;
use frame_doc::doc::Document;
use frame_doc::doc::NodeId;
use serde_json::Value;

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
}

struct Prepared {
    session: Session,
    version: u64,
    mode: Option<String>,
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
        Self::with_session(Session::with_document(document), host, version, mode)
    }

    fn with_session(
        session: Session,
        host: Arc<ThemeHost>,
        version: u64,
        mode: Option<String>,
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
}

fn prepare(host: &ThemeHost) -> Result<Prepared, OpenError> {
    let (theme, version) = host.snapshot().ok_or(OpenError::NoTheme)?;
    let document = frame_doc::scene_to_doc(&theme.source()).map_err(OpenError::Document)?;
    Ok(Prepared {
        session: Session::with_document(document),
        version,
        mode: Some(theme.mode().to_owned()),
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
        state.pending.take()
    }
}
