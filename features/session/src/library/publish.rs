//! Putting songs into the library: a song's File Root on the org's media
//! volume, its files uploaded into it, its `song:` doc, the list that
//! plays it, the projects that hold the list, and a live link to it.
//!
//! Every call here is idempotent — it finds what is there before it makes
//! anything — so `studio ingest` can be run again and again over the same
//! songs: what exists comes back ([`Made::Found`]), what is missing is made
//! ([`Made::Created`]), what differs is brought in line ([`Made::Updated`]).
//! Deciding *what* to upload is the caller's (`session_engine::ingest`);
//! this is the Task side of it.

use collection_proto::{CollectionKind, CollectionServiceClient};
use files_client::ClientError;
use files_proto::model::{FileRootInfo, RootFlavor};
use files_proto::service::tree::Cursor;
use files_proto::service::upload::{ChunkRange, UploadFrame, UploadSpec};
use files_proto::service::version::VersionServiceClient;
use files_proto::service::write::OnConflict;
use links_proto::{Confidence, LinksServiceClient, NodeKind, NodeRef, Relation, TypedLink};
use project_proto::{ProjectInfo, ProjectServiceClient};
use resources_proto::ResourcesServiceClient;
use share_proto::{NewShareLink, ShareServiceClient, ShareTarget};

use super::{Library, ListKind, session_root_dir, session_root_for};

/// The Files lanes and the ids an uploader holds, for a caller that does
/// not depend on Task's crates itself.
pub use files_client::FilesClient;
pub use files_proto::id::{RootId, UploadId};
/// A project's form (album, concert, …), as Task files it.
pub use project_proto::Form;

/// Bytes per upload frame (`files_client::FRAME`).
const FRAME: usize = files_client::FRAME;

/// How many times a file's missing ranges are sent again before the
/// upload gives up — each time only what the server says it lacks.
const RESENDS: usize = 3;

/// Whether a call found what it was asked for, made it, or changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Made {
    Found,
    Created,
    Updated,
}

/// A song's File Root, as [`Library::song_root`] found or made it.
#[derive(Debug, Clone, PartialEq)]
pub struct SongRoot {
    pub root: FileRootInfo,
    pub made: Made,
}

impl SongRoot {
    /// The root's id as the lanes take it.
    #[must_use]
    pub fn id(&self) -> RootId {
        files_client::root_id(&self.root)
    }
}

/// One file of a root as the server catalogues it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// Relative to the root, `/`-separated.
    pub path: String,
    pub size: u64,
    /// Its content address, when the server has one for it yet.
    pub content: Option<String>,
}

/// A file to upload: where in the root, how long, and its content address
/// when the caller computed one (which lets the server skip bytes it
/// already holds).
#[derive(Debug, Clone, Copy)]
pub struct Upload<'a> {
    pub root: RootId,
    pub path: &'a str,
    pub size: u64,
    pub content: Option<&'a str>,
    /// An upload of this same file a previous run began and did not land:
    /// resumed from what the server says it still lacks, when the server
    /// still has it.
    pub resume: Option<UploadId>,
}

/// What an upload landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uploaded {
    /// The landed file's content address, as the server reports it.
    pub content: Option<String>,
    /// Bytes that crossed the wire (0 when the server held them already).
    pub sent: u64,
}

/// Where an upload's bytes come from: `read_at(offset, buf)` fills `buf`
/// from the file at `offset` (a positional read, so ranges can be sent in
/// any order and again).
pub trait ReadAt: Sync {
    /// # Errors
    /// The source could not be read.
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;
}

/// What a project to find or make is: its title, the project it sits
/// under, its form, and the details it is made with (an existing
/// project's details are left as they are).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSpec {
    pub title: String,
    pub parent: Option<uuid::Uuid>,
    pub form: Option<Form>,
    pub details: String,
}

/// A project as [`Library::ensure_project`] left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: uuid::Uuid,
    /// Its node on the links graph (`project:<path>`).
    pub node: String,
    pub made: Made,
}

impl Library {
    /// The Files lanes of the org, on one connection of their own: what
    /// an uploader holds per worker.
    ///
    /// # Errors
    /// The server cannot be reached.
    pub async fn files_client(&self) -> eyre::Result<FilesClient> {
        self.files().await
    }

    /// The song `slug`'s File Root: the one the library already plays it
    /// from ([`session_root_for`] — a root at `session/<slug>`, or an
    /// older one named for the song), else a new one at `session/<slug>`
    /// on the org's media volume (`location`, or its one location),
    /// named `title`.
    ///
    /// # Errors
    /// The server cannot be reached, or refused (no media volume granted,
    /// or a folder there that is not a root).
    pub async fn song_root(
        &self,
        files: &FilesClient,
        slug: &str,
        title: &str,
        location: Option<uuid::Uuid>,
    ) -> eyre::Result<SongRoot> {
        if let Some(root) = self.find_song_root(files, slug).await? {
            return Ok(SongRoot {
                root,
                made: Made::Found,
            });
        }
        let dir = session_root_dir(slug);
        let root = files
            .ensure_root_on(location, &dir, title, RootFlavor::Media)
            .await
            .map_err(|e| eyre::eyre!("making the root {dir}: {e}"))?;
        Ok(SongRoot {
            root,
            made: Made::Created,
        })
    }

    /// The File Root the library plays the song `slug` from, if it has
    /// one ([`session_root_for`]); nothing is made.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn find_song_root(
        &self,
        files: &FilesClient,
        slug: &str,
    ) -> eyre::Result<Option<FileRootInfo>> {
        let roots = files
            .roots
            .list()
            .await
            .map_err(|e| eyre::eyre!("listing roots: {}", ClientError::from(e)))?;
        Ok(session_root_for(&roots, slug).cloned())
    }

    /// Every file in `root`, as the server catalogues it.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn catalogue(
        &self,
        files: &FilesClient,
        root: RootId,
    ) -> eyre::Result<Vec<RemoteFile>> {
        use files_proto::service::tree::EntryKind;
        let mut out = Vec::new();
        let mut cursor: Option<Cursor> = None;
        loop {
            let page = files
                .tree
                .catalogue(root, cursor.clone())
                .await
                .map_err(|e| eyre::eyre!("the catalogue of {root}: {}", ClientError::from(e)))?;
            out.extend(
                page.changed
                    .into_iter()
                    .filter(|e| e.kind == EntryKind::File)
                    .map(|e| RemoteFile {
                        path: e.path.as_str().to_owned(),
                        size: e.size,
                        content: e.content.map(|c| c.0),
                    }),
            );
            if !page.more {
                break;
            }
            cursor = Some(page.cursor);
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out.dedup_by(|a, b| a.path == b.path);
        Ok(out)
    }

    /// The content address each of `paths` in `root` was last
    /// checkpointed at (its version chain's newest), for the files the
    /// catalogue has not addressed yet (it publishes a resident file
    /// unverified). A path with no version is left out.
    ///
    /// # Errors
    /// The server cannot be reached.
    pub async fn addresses(
        &self,
        root: RootId,
        paths: &[String],
    ) -> eyre::Result<std::collections::HashMap<String, String>> {
        use futures_util::StreamExt as _;
        /// Chains asked for at once.
        const AT_ONCE: usize = 16;
        if paths.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let versions: VersionServiceClient = self.service().await?;
        let versions = &versions;
        Ok(futures_util::stream::iter(paths)
            .map(|path| async move {
                let at = files_proto::RootPath::parse(path).ok()?;
                let newest = versions.chain(root, at).await.ok()?.into_iter().next()?;
                Some((path.clone(), newest.file_id))
            })
            .buffer_unordered(AT_ONCE)
            .filter_map(std::future::ready)
            .collect()
            .await)
    }

    /// Checkpoint `root` now, described as `message`: one version of
    /// everything an ingest landed.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn checkpoint(&self, root: RootId, message: &str) -> eyre::Result<()> {
        let versions: VersionServiceClient = self.service().await?;
        versions
            .checkpoint(root, Some(message.to_owned()))
            .await
            .map_err(|e| eyre::eyre!("checkpointing {root}: {}", ClientError::from(e)))?;
        Ok(())
    }

    /// The song `slug` in the library, called `title`: made when it is not
    /// there; one that is keeps its title, writers and chart.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn ensure_song(&self, slug: &str, title: &str) -> eyre::Result<Made> {
        let resources: ResourcesServiceClient = self.service().await?;
        if resources.song(slug.to_owned()).await.is_ok() {
            return Ok(Made::Found);
        }
        let made = resources
            .upsert_song(resources_proto::SongDoc {
                slug: slug.to_owned(),
                title: title.to_owned(),
                ..resources_proto::SongDoc::default()
            })
            .await
            .map_err(|e| eyre::eyre!("making song:{slug}: {e:?}"))?;
        if made.slug != slug {
            eyre::bail!("the library made song:{} for song:{slug}", made.slug);
        }
        Ok(Made::Created)
    }

    /// The list titled `title` (of `kind`) holding exactly `songs` in that
    /// order: the one there is brought in line ([`Library::set_songs`]),
    /// else one is made. Returns its id.
    ///
    /// # Errors
    /// Two lists of that kind share the title, or the server refused.
    pub async fn ensure_list(
        &self,
        title: &str,
        kind: ListKind,
        songs: &[String],
    ) -> eyre::Result<(String, Made)> {
        let collections: CollectionServiceClient = self.service().await?;
        let mut named: Vec<collection_proto::Collection> = collections
            .list(
                self.org.clone(),
                Some(CollectionKind::new(kind.collection_kind())),
            )
            .await
            .map_err(|e| eyre::eyre!("listing lists: {e:?}"))?
            .into_iter()
            .filter(|c| c.title.trim().eq_ignore_ascii_case(title.trim()))
            .collect();
        if named.len() > 1 {
            eyre::bail!("{} lists are titled {title:?}; rename one", named.len());
        }
        let (id, made) = match named.pop() {
            Some(mut list) => {
                list.sort_items();
                let now: Vec<&str> = list
                    .items
                    .iter()
                    .filter(|i| i.node.kind == NodeKind::Song)
                    .map(|i| i.node.id.as_str())
                    .collect();
                let same = now.iter().copied().eq(songs.iter().map(String::as_str));
                (list.id, if same { Made::Found } else { Made::Updated })
            }
            None => (self.create_list(title, kind).await?.id, Made::Created),
        };
        if made != Made::Found {
            self.set_songs(&id, songs).await?;
        }
        Ok((id, made))
    }

    /// The project `spec` names — the one titled so under the same parent,
    /// its form set to `spec`'s when it has one — else a new one.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn ensure_project(&self, spec: &ProjectSpec) -> eyre::Result<Project> {
        let projects: ProjectServiceClient = self.service().await?;
        let found = projects
            .list()
            .await
            .map_err(|e| eyre::eyre!("listing projects: {e:?}"))?
            .into_iter()
            .find(|p| {
                p.parent_id == spec.parent && p.title.trim().eq_ignore_ascii_case(spec.title.trim())
            });
        let (info, made) = match found {
            Some(mut info) if spec.form.is_some() && info.form != spec.form => {
                info.form = spec.form;
                let saved = projects
                    .update(info)
                    .await
                    .map_err(|e| eyre::eyre!("setting {}'s form: {e:?}", spec.title))?;
                (saved, Made::Updated)
            }
            Some(info) => (info, Made::Found),
            None => {
                let draft = ProjectInfo {
                    title: spec.title.clone(),
                    status: "active".into(),
                    priority: "normal".into(),
                    project_type: "general".into(),
                    parent_id: spec.parent,
                    details: spec.details.clone(),
                    form: spec.form,
                    ..ProjectInfo::default()
                };
                let made = projects
                    .create(draft)
                    .await
                    .map_err(|e| eyre::eyre!("making the project {}: {e:?}", spec.title))?;
                (made, Made::Created)
            }
        };
        Ok(Project {
            id: info.id,
            node: project_node(&info.path),
            made,
        })
    }

    /// `project` holds the list `list_id` (the `Down` link `task project
    /// attach-setlist` makes), made once.
    ///
    /// # Errors
    /// The server cannot be reached, or refused.
    pub async fn hold_list(&self, project: &Project, list_id: &str) -> eyre::Result<Made> {
        let links: LinksServiceClient = self.service().await?;
        let source = NodeRef::new(NodeKind::Project, project.node.clone());
        let target = NodeRef::new(NodeKind::Collection, list_id.to_owned());
        let held = links
            .links_for(source.clone())
            .await
            .map_err(|e| eyre::eyre!("the links of {}: {e:?}", project.node))?
            .into_iter()
            .any(|l| l.source == source && l.relation == Relation::Down && l.target == target);
        if held {
            return Ok(Made::Found);
        }
        links
            .create(TypedLink::new(
                source,
                target,
                Relation::Down,
                Confidence::Certain,
            ))
            .await
            .map_err(|e| eyre::eyre!("attaching the list: {e:?}"))?;
        Ok(Made::Created)
    }

    /// A public live link to the list `list_id` (`task share live`): the
    /// one already minted for it when it is not disabled, else a new one.
    /// Returns its token.
    ///
    /// # Errors
    /// The server cannot be reached, or refused (sharing is off).
    pub async fn live_link(&self, list_id: &str, label: &str) -> eyre::Result<(String, Made)> {
        let share: ShareServiceClient = self.service().await?;
        let target = ShareTarget::Live {
            setlist: list_id.to_owned(),
            reset_secs: 0,
        };
        let existing = share
            .links_for_target(target.clone())
            .await
            .map_err(|e| eyre::eyre!("the list's links: {e:?}"))?
            .into_iter()
            .find(|l| !l.disabled && !l.password_protected && l.expires_unix == 0);
        if let Some(link) = existing {
            return Ok((link.token, Made::Found));
        }
        let link = share
            .create_link(
                target,
                NewShareLink {
                    label: label.to_owned(),
                    capabilities: None,
                    password: None,
                    expires_unix: None,
                },
            )
            .await
            .map_err(|e| eyre::eyre!("minting the link: {e:?}"))?;
        Ok((link.token, Made::Created))
    }

    /// Where a browser opens the live link `token`.
    #[must_use]
    pub fn live_url(&self, token: &str) -> String {
        format!("{}/session/?live={token}", self.http_base())
    }
}

/// The links-graph id of the project at `path`: its directory, the
/// project page (`org_proto::PROJECT_PAGE`) left off — what `task project
/// attach-setlist` links.
fn project_node(path: &str) -> String {
    let path = path.trim().trim_matches('/');
    path.strip_suffix("project.md")
        .map_or(path, |dir| dir.trim_end_matches('/'))
        .to_owned()
}

/// Upload one file into a root, streamed from `source`, resumable: a
/// dropped stream is re-sent from the server's own account of what it
/// lacks ([`RESENDS`] times), and `upload.resume` picks up a previous
/// run's. `began` is told the upload's id as soon as it has one (what a
/// caller keeps to resume with), `sent` every frame's length as it goes.
/// Lands over whatever is at the path (the old one stays in history).
///
/// # Errors
/// The source could not be read, or the server refused or kept missing
/// bytes.
pub async fn upload(
    files: &FilesClient,
    upload: Upload<'_>,
    source: &dyn ReadAt,
    began: &(dyn Fn(UploadId) + Sync),
    sent: &(dyn Fn(u64) + Sync),
) -> eyre::Result<Uploaded> {
    let (id, mut needed) = match resumed(files, &upload).await {
        Some(resumed) => resumed,
        None => {
            let path = files_proto::RootPath::parse(upload.path)
                .map_err(|e| eyre::eyre!("{}: {e:?}", upload.path))?;
            let plan = files
                .uploads
                .begin(UploadSpec {
                    root_id: upload.root,
                    path,
                    size: upload.size,
                    content: upload.content.map(files_proto::id::ContentId::new),
                    modified_at: None,
                    expect: None,
                })
                .await
                .map_err(|e| eyre::eyre!("{}: {}", upload.path, ClientError::from(e)))?;
            (plan.upload_id, plan.needed)
        }
    };
    began(id);
    let mut total = 0;
    for _ in 0..=RESENDS {
        if needed.is_empty() {
            break;
        }
        match send_ranges(files, id, &needed, source, sent).await {
            Ok((after, n)) => {
                total += n;
                needed = after;
            }
            Err(e) => {
                // The stream dropped: what the server has is the truth.
                needed = files
                    .uploads
                    .progress(id)
                    .await
                    .map_err(|p| {
                        eyre::eyre!("{}: {e} (and then: {})", upload.path, ClientError::from(p))
                    })?
                    .needed;
            }
        }
    }
    if !needed.is_empty() {
        eyre::bail!(
            "{}: the server still lacks {} range(s) after {RESENDS} resends",
            upload.path,
            needed.len()
        );
    }
    let landed = files
        .uploads
        .complete(id, OnConflict::Replace)
        .await
        .map_err(|e| eyre::eyre!("{}: {}", upload.path, ClientError::from(e)))?;
    Ok(Uploaded {
        content: landed.content.map(|c| c.0),
        sent: total,
    })
}

/// A previous run's upload of the same file, if the server still has it:
/// its id and what it still lacks.
async fn resumed(files: &FilesClient, upload: &Upload<'_>) -> Option<(UploadId, Vec<ChunkRange>)> {
    let id = upload.resume?;
    let progress = files.uploads.progress(id).await.ok()?;
    (progress.total == upload.size).then_some((id, progress.needed))
}

/// Send `ranges` of the source for upload `id`, a frame at a time. Returns
/// what the server still lacks afterwards and the bytes sent.
async fn send_ranges(
    files: &FilesClient,
    id: UploadId,
    ranges: &[ChunkRange],
    source: &dyn ReadAt,
    sent: &(dyn Fn(u64) + Sync),
) -> eyre::Result<(Vec<ChunkRange>, u64)> {
    let (tx, rx) = vox::channel::<UploadFrame>();
    let sending = async move {
        let mut buf = vec![0u8; FRAME];
        let mut total = 0u64;
        for range in ranges {
            let mut at = range.start;
            while at < range.end {
                let n = usize::try_from((range.end - at).min(FRAME as u64)).unwrap_or(FRAME);
                source
                    .read_at(at, &mut buf[..n])
                    .map_err(|e| eyre::eyre!("reading at {at}: {e}"))?;
                tx.send(UploadFrame::Chunk {
                    offset: at,
                    bytes: buf[..n].to_vec(),
                })
                .await
                .map_err(|e| eyre::eyre!("sending: {e:?}"))?;
                sent(n as u64);
                total += n as u64;
                at += n as u64;
            }
        }
        let _ = tx.send(UploadFrame::Finished).await;
        Ok::<u64, eyre::Report>(total)
    };
    let (received, total) = futures_util::join!(files.uploads.send_bytes(id, rx), sending);
    let total = total?;
    let received = received.map_err(|e| eyre::eyre!("{}", ClientError::from(e)))?;
    Ok((received.needed, total))
}

#[cfg(test)]
mod tests {
    use super::project_node;

    #[test]
    fn a_project_is_linked_by_its_directory_not_its_page() {
        assert_eq!(
            project_node("jts-senior-recital/project.md"),
            "jts-senior-recital"
        );
        assert_eq!(project_node("/a/b/project.md"), "a/b");
        assert_eq!(project_node("Projects/x.md"), "Projects/x.md");
    }
}
