//! A song's files, live: its links (the song's Drive project and Studio's
//! notes, both documents), and what Drive's node says of each file (the
//! indexes of every computer: where each file is, whether that computer is
//! on, whether the node keeps a copy). The model is `files`; this is the
//! reading and writing around it.
//!
//! The web build asks the node it came from (`/list`, `/search`,
//! `/copies`, `/sidecar`, `/fetch`, `/blobs`). The desktop and phone builds
//! have no Drive client yet: they keep the links in memory and, on a
//! computer with Drive's agent running, warm and open files through
//! `drive-local` (`local`).

use dioxus::prelude::*;
use loro::LoroDoc;

use crate::files::{self, FileKind, FileState, Hit, Link, NewLink};
use crate::songs::{Entry, backing};

/// One file of a song, as listed: the link it comes through and what the
/// index says of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub link: Link,
    pub hit: Hit,
    /// Its kind: as someone set it, else the link's for a linked file, else
    /// recognised from its name inside a linked folder.
    pub kind: FileKind,
    /// No index has it (any more).
    pub missing: bool,
}

impl Row {
    pub fn target(&self) -> String {
        self.hit.target()
    }

    /// The state readiness reads (`here` stays unknown: only `drive-local`
    /// can say, on a device).
    pub fn state(&self, others: &[files::Copy]) -> FileState {
        let mut copies = vec![files::Copy {
            device_name: self.hit.device_name.clone(),
            location_name: self.hit.location_name.clone(),
            online: self.hit.online,
        }];
        copies.extend(others.iter().cloned());
        FileState {
            target: self.target(),
            name: self.hit.name.clone(),
            size: self.hit.size,
            kind: Some(self.kind),
            copies: if self.missing { Vec::new() } else { copies },
            on_node: self.hit.available.is_some(),
            here: None,
            missing: self.missing,
        }
    }
}

/// A song's files, as the song's page and the set's readiness use them.
#[derive(Clone, PartialEq)]
pub struct SongFiles {
    pub owner: String,
    pub id: String,
    /// The org Studio is in (the song is another's when they differ).
    pub here: String,
    registry: backing::Backing,
    meta: backing::Backing,
    tree: backing::Backing,
    /// For a song another org shares: its project's parts, as `/shared`
    /// lists them (its registry is not this org's to read).
    shared_parts: Signal<Option<Vec<drive_files::projects::Part>>>,
}

pub fn use_song_files(owner: &str, id: &str) -> SongFiles {
    let here = (crate::state::use_app().here)();
    let registry = backing::use_backing(drive_files::space_doc(owner));
    let meta = backing::use_backing(crate::orgs::qualified(owner, &files::files_doc(id)));
    let tree = backing::use_backing(drive_files::files_doc(owner));
    let shared_parts = use_signal(|| None);
    let sf = SongFiles {
        owner: owner.to_string(),
        id: id.to_string(),
        here: here.clone(),
        registry,
        meta,
        tree,
        shared_parts,
    };
    #[cfg(feature = "drive")]
    {
        let (owner, id, here) = (owner.to_string(), id.to_string(), here);
        let mut parts = shared_parts;
        use_future(use_reactive!(|owner, id, here| async move {
            if owner == here {
                return;
            }
            parts.set(Some(web::shared_parts(&owner, &here, &id).await));
        }));
    }
    sf
}

impl SongFiles {
    pub fn own(&self) -> bool {
        self.owner == self.here
    }

    pub fn links(&self) -> Vec<Link> {
        let meta = self.meta.loro();
        if self.own() {
            files::links(&self.registry.loro(), &meta, &self.id, &self.owner)
        } else {
            let parts = (self.shared_parts)().unwrap_or_default();
            files::links_of(&parts, &meta, &self.owner)
        }
    }

    /// What changes when the links do (to key what is listed).
    pub fn rev(&self) -> String {
        self.links()
            .iter()
            .map(|l| {
                format!(
                    "{}={}/{}:{}:{}:{}",
                    l.part,
                    l.space,
                    l.target(),
                    l.dir,
                    l.kind.id(),
                    l.available
                )
            })
            .collect::<Vec<_>>()
            .join("|")
            // A file's kind someone changed shows at once.
            + &format!("{:?}", self.meta.loro().get_map("kinds").get_deep_value())
    }

    fn stamp(&self, d: &LoroDoc) {
        crate::orgs::stamp_org(d, &self.owner);
    }

    /// Edit the registry and Studio's notes together: one edit each, in
    /// turn (an edit of one document inside another's lost the inner
    /// one's changes on their way to the node).
    fn edit_both<R>(&self, f: impl FnOnce(&LoroDoc, &LoroDoc) -> R) -> R {
        let m = self.meta.peek();
        self.stamp(&m);
        let r = self.registry.edit(|reg| f(reg, &m));
        // Commit and send what `f` wrote into the notes.
        self.meta.edit(|_| ());
        r
    }

    /// Link a file or folder where it is (its own org's song only).
    pub fn link(&self, entry: &Entry, l: NewLink) -> Option<String> {
        if !self.own() {
            return None;
        }
        let owner = self.owner.clone();
        self.edit_both(|reg, m| files::link(reg, m, &owner, entry, &l))
    }

    pub fn unlink(&self, part: &str) {
        if self.own() {
            self.edit_both(|reg, m| files::unlink(reg, m, &self.id, part));
        }
    }

    pub fn set_kind(&self, part: &str, kind: FileKind) {
        if self.own() {
            self.edit_both(|reg, m| files::set_link_kind(reg, m, &self.id, part, kind));
        }
    }

    pub fn file_kind(&self, target: &str) -> Option<FileKind> {
        files::file_kind(&self.meta.loro(), target)
    }

    pub fn set_file_kind(&self, target: &str, kind: Option<FileKind>) {
        if self.own() {
            self.meta.edit(|m| files::set_file_kind(m, target, kind));
        }
    }

    pub fn aliases(&self) -> Vec<String> {
        files::aliases(&self.meta.loro())
    }

    pub fn set_alias(&self, alias: &str, on: bool) {
        if self.own() {
            self.meta.edit(|m| files::set_alias(m, alias, on));
        }
    }

    /// Keep these parts on the node (Drive's "available"), or stop.
    pub fn keep(&self, parts: &[String], on: bool) {
        if self.own() && !parts.is_empty() {
            self.registry
                .edit(|reg| files::set_available(reg, &self.id, parts, on));
        }
    }

    /// The Drive tree as this org has it (for uploads and tree links).
    pub fn tree(&self) -> LoroDoc {
        self.tree.peek()
    }

    /// Put a new small file into the org's Drive tree, in the song's
    /// folder, and link it. Its bytes go to the node first (by their
    /// hash); the tree entry is the commit point.
    #[cfg_attr(not(feature = "drive"), allow(dead_code))]
    pub fn add_uploaded(&self, entry: &Entry, name: &str, size: u64, hash: &str) -> Option<String> {
        let folder = files::upload_folder(&entry.title);
        let name = name.replace(['/', '\\'], "-");
        let path = format!("{folder}/{name}");
        self.tree.edit(|t| {
            let mut parent = None;
            for seg in folder.split('/') {
                parent = Some(match drive_files::tree::lookup(t, parent, seg) {
                    Some(e) if e.dir => e.id,
                    _ => drive_files::tree::mkdir(t, parent, seg).ok()?,
                });
            }
            // A file of that name already: its new version.
            let id = match drive_files::tree::lookup(t, parent, &name) {
                Some(e) if !e.dir => e.id,
                _ => drive_files::tree::create_file(t, parent, &name).ok()?,
            };
            drive_files::tree::set_content(t, id, size, hash).ok()
        })?;
        self.link(
            entry,
            NewLink {
                space: self.owner.clone(),
                location: files::TREE.into(),
                path,
                dir: false,
                kind: None,
            },
        )
    }
}

/// The rows of a song's files, from Drive's indexes: a linked file as it
/// is, a linked folder walked (up to a few hundred files). Re-read when
/// the links change or `refresh` moves.
pub fn use_rows(sf: &SongFiles, refresh: Signal<u64>) -> Resource<Vec<Row>> {
    let sf = sf.clone();
    let rev = sf.rev();
    use_resource(use_reactive!(|rev| {
        let sf = sf.clone();
        let _ = (rev, refresh());
        async move { list_rows(&sf).await }
    }))
}

/// Every file of the links, listed.
pub async fn list_rows(sf: &SongFiles) -> Vec<Row> {
    let links = sf.links();
    let tree = sf.tree();
    let mut out = Vec::new();
    for l in links {
        let hits = if l.in_tree() {
            tree_hits(&tree, &l)
        } else {
            location_hits(&l).await
        };
        if hits.is_empty() {
            out.push(Row {
                hit: Hit {
                    space: l.space.clone(),
                    location: l.location.clone(),
                    path: l.path.clone(),
                    name: l.name().to_string(),
                    dir: l.dir,
                    ..Default::default()
                },
                kind: l.kind,
                missing: true,
                link: l,
            });
            continue;
        }
        for h in hits {
            let kind = sf.file_kind(&h.target()).unwrap_or(if l.dir {
                match files::recognise(&h.path, false) {
                    // A folder's own kind for what its names do not tell.
                    FileKind::Reference | FileKind::Other if l.kind != FileKind::Other => l.kind,
                    k => k,
                }
            } else {
                l.kind
            });
            out.push(Row {
                link: l.clone(),
                hit: h,
                kind,
                missing: false,
            });
        }
    }
    out
}

/// A tree link's files (from the org's tree document).
fn tree_hits(tree: &LoroDoc, l: &Link) -> Vec<Hit> {
    use drive_files::tree;
    let Some(start) = tree::resolve(tree, &l.path) else {
        return Vec::new();
    };
    match start {
        Some(e) if !e.dir => vec![Hit::tree(&l.space, &l.path, e.size as i64, &e.hash)],
        start => {
            let mut out = Vec::new();
            let mut stack = vec![start.map(|e| e.id)];
            while let Some(dir) = stack.pop() {
                for e in tree::children(tree, dir) {
                    if e.dir {
                        stack.push(Some(e.id));
                    } else {
                        let path = tree::path_of(tree, e.id);
                        out.push(Hit::tree(&l.space, &path, e.size as i64, &e.hash));
                    }
                }
                if out.len() > 400 {
                    break;
                }
            }
            out
        }
    }
}

#[cfg(feature = "drive")]
async fn location_hits(l: &Link) -> Vec<Hit> {
    web::location_hits(l).await
}

#[cfg(not(feature = "drive"))]
async fn location_hits(l: &Link) -> Vec<Hit> {
    // No index here: the link itself, its place unknown.
    vec![Hit {
        space: l.space.clone(),
        location: l.location.clone(),
        location_name: l.location.clone(),
        path: l.path.clone(),
        name: l.name().to_string(),
        dir: l.dir,
        ..Default::default()
    }]
}

/// The node's answers (in a browser and natively; the module keeps its
/// old name).
#[cfg(feature = "drive")]
pub mod web {
    use super::*;
    use serde_json::Value;

    pub fn enc(s: &str) -> String {
        drive_dioxus::env::encode_component(s)
    }

    /// A node path as something a page element can load: the page's own
    /// origin in a browser; natively the node's address (a relative URL
    /// would be the app's own `dioxus://` scheme).
    fn absolute(path: String) -> String {
        if cfg!(target_arch = "wasm32") || !path.starts_with('/') {
            path
        } else {
            format!("{}{path}", drive_dioxus::env::origin())
        }
    }

    fn headers() -> Vec<(String, String)> {
        drive_dioxus::auth::bearer()
            .into_iter()
            .map(|b| ("authorization".to_string(), b))
            .collect()
    }

    pub async fn get(url: &str) -> Option<Value> {
        get_in(None, url).await
    }

    /// A call to Drive as a span: under `parent` when the page is timing
    /// something larger, its traceparent sent so the node's spans join it.
    fn call_span(
        parent: Option<&crate::trace::Span>,
        method: &str,
        url: &str,
    ) -> crate::trace::Span {
        let path = url.split('?').next().unwrap_or(url);
        let name = format!("{method} {path}");
        let mut s = match parent {
            Some(p) => p.client(&name),
            None => crate::trace::Span::root(&name),
        };
        s.set("http.request.method", method);
        s.set("url.path", path);
        s
    }

    pub async fn get_in(parent: Option<&crate::trace::Span>, url: &str) -> Option<Value> {
        let mut span = call_span(parent, "GET", url);
        let mut h = headers();
        h.push(("traceparent".to_string(), span.traceparent()));
        let h: Vec<(&str, &str)> = h.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let got = drive_dioxus::auth::fetch("GET", url, &h, None).await;
        match got {
            Some((200, v)) => {
                span.set("http.response.status_code", 200);
                Some(v)
            }
            Some((code, _)) => {
                span.set("http.response.status_code", code);
                span.fail(format!("answered {code}"));
                None
            }
            None => {
                span.fail("no answer");
                None
            }
        }
    }

    fn st(v: &Value, k: &str) -> String {
        v[k].as_str().unwrap_or_default().to_string()
    }

    /// A hit of `/search` or `/list`, in `space`.
    pub fn hit(space: &str, v: &Value) -> Hit {
        Hit {
            space: space.to_string(),
            location: st(v, "location"),
            location_name: st(v, "location_name"),
            device: st(v, "device"),
            device_name: st(v, "device_name"),
            online: v["online"].as_bool().unwrap_or(false),
            path: st(v, "path"),
            name: st(v, "name"),
            dir: v["dir"].as_bool().unwrap_or(false),
            size: v["size"].as_i64().unwrap_or(0),
            mtime: v["mtime"].as_i64().unwrap_or(0),
            sampled: st(v, "sampled"),
            available: v["available"].as_str().map(str::to_string),
            copies: v["copies"].as_i64().unwrap_or(1),
        }
    }

    /// One folder of a location's index.
    pub async fn list(space: &str, location: &str, path: &str) -> Vec<Hit> {
        list_in(None, space, location, path).await
    }

    pub async fn list_in(
        parent: Option<&crate::trace::Span>,
        space: &str,
        location: &str,
        path: &str,
    ) -> Vec<Hit> {
        if location == files::DISK {
            return disk::list(space, path);
        }
        let url = format!(
            "/list?space={}&location={}&path={}",
            enc(space),
            enc(location),
            enc(path)
        );
        get_in(parent, &url)
            .await
            .and_then(|v| v["entries"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|x| hit(space, x))
            .collect()
    }

    /// A link's files: the file, or every file under the folder (up to
    /// 400, four folders deep).
    pub async fn location_hits(l: &Link) -> Vec<Hit> {
        let (dir, name) = match l.path.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), l.path.clone()),
        };
        let up = list(&l.space, &l.location, &dir).await;
        let Some(me) = up.into_iter().find(|h| h.name == name) else {
            return Vec::new();
        };
        if !me.dir {
            return vec![me];
        }
        let mut out = Vec::new();
        let mut queue = vec![(me.path.clone(), 0)];
        while let Some((p, depth)) = queue.pop() {
            for h in list(&l.space, &l.location, &p).await {
                if h.dir {
                    if depth < 4 {
                        queue.push((h.path.clone(), depth + 1));
                    }
                } else {
                    out.push(h);
                }
            }
            if out.len() >= 400 {
                break;
            }
        }
        out.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
        out
    }

    /// Files and folders whose names hold every word, in a space's
    /// indexes (every computer of that org).
    pub async fn search(space: &str, q: &str, exts: &[String], limit: u32) -> Vec<Hit> {
        search_in(None, space, q, exts, limit).await
    }

    pub async fn search_in(
        parent: Option<&crate::trace::Span>,
        space: &str,
        q: &str,
        exts: &[String],
        limit: u32,
    ) -> Vec<Hit> {
        let url = format!(
            "/search?space={}&q={}&exts={}&limit={limit}",
            enc(space),
            enc(q),
            enc(&exts.join(","))
        );
        get_in(parent, &url)
            .await
            .and_then(|v| v["results"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|x| hit(space, x))
            .collect()
    }

    /// Every other place holding a file's content.
    pub async fn copies(h: &Hit) -> Vec<files::Copy> {
        if h.sampled.is_empty() {
            return Vec::new();
        }
        let url = format!(
            "/copies?space={}&sampled={}&size={}",
            enc(&h.space),
            enc(&h.sampled),
            h.size
        );
        get(&url)
            .await
            .and_then(|v| v["copies"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|x| hit(&h.space, x))
            .filter(|c| !(c.location == h.location && c.path == h.path))
            .map(|c| files::Copy {
                device_name: c.device_name,
                location_name: c.location_name,
                online: c.online,
            })
            .collect()
    }

    /// A song of another org: its project's parts, from Drive's
    /// projects shared with this org.
    pub async fn shared_parts(
        owner: &str,
        here: &str,
        id: &str,
    ) -> Vec<drive_files::projects::Part> {
        let url = format!("/shared?org={}", enc(here));
        let want = files::song_project(id);
        let Some(v) = get(&url).await else {
            return Vec::new();
        };
        v["shared"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter(|x| {
                x["org"].as_str() == Some(owner)
                    && x["project"]["id"].as_str() == Some(want.as_str())
            })
            .flat_map(|x| {
                x["project"]["parts"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            })
            .map(|p| drive_files::projects::Part {
                id: st(&p, "id"),
                label: st(&p, "label"),
                location: st(&p, "location"),
                path: st(&p, "path"),
                available: p["available"].as_bool().unwrap_or(false),
                space: st(&p, "space"),
            })
            .collect()
    }

    /// Where a file's bytes stream from (ranges honoured, so audio
    /// seeks): the node's copy when it keeps one, else the computer that
    /// has it, through the node.
    pub fn file_url(h: &Hit) -> String {
        if h.location == files::DISK {
            return disk::url(&h.path);
        }
        absolute(drive_dioxus::mount::api(&match &h.available {
            Some(hash) if !hash.is_empty() => format!("/blobs/{hash}?name={}", enc(&h.name)),
            _ => format!(
                "/fetch/{}/{}?path={}&name={}",
                enc(&h.space),
                enc(&h.location),
                enc(&h.path),
                enc(&h.name)
            ),
        }))
    }

    /// A sidecar's URL (`thumb-256`, `thumb-1024`, `peaks`): made by the
    /// computer that has the file when the node has none yet.
    pub fn sidecar_url(h: &Hit, kind: &str) -> Option<String> {
        sidecar_path(h, kind).map(|p| absolute(drive_dioxus::mount::api(&p)))
    }

    fn sidecar_path(h: &Hit, kind: &str) -> Option<String> {
        if let Some(hash) = h.available.as_ref().filter(|x| !x.is_empty())
            && h.location == files::TREE
        {
            return Some(format!(
                "/sidecar/{hash}/{kind}?name={}&hash={hash}",
                enc(&h.name)
            ));
        }
        if h.sampled.is_empty() {
            return None;
        }
        Some(format!(
            "/sidecar/s{}/{kind}?space={}&location={}&path={}&name={}",
            h.sampled,
            enc(&h.space),
            enc(&h.location),
            enc(&h.path),
            enc(&h.name)
        ))
    }

    pub async fn bytes(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<(u16, Vec<u8>), String> {
        bytes_in(None, method, url, body).await
    }

    pub async fn bytes_in(
        parent: Option<&crate::trace::Span>,
        method: &str,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<(u16, Vec<u8>), String> {
        if let Some(r) = disk::bytes(url) {
            return r;
        }
        let mut span = call_span(parent, method, url);
        let r = bytes_raw(method, url, body, &span.traceparent()).await;
        match &r {
            Ok((code, b)) => {
                span.set("http.response.status_code", *code);
                span.set("http.response.body.size", b.len() as u64);
            }
            Err(e) => span.fail(e.clone()),
        }
        r
    }

    /// The bytes `range` of a file at `url` (a [`file_url`]): `206` and
    /// that stretch, or `200` and the whole file from a node that ignores
    /// ranges.
    pub async fn range(url: &str, range: std::ops::Range<u64>) -> Result<(u16, Vec<u8>), String> {
        if let Some(r) = disk::bytes(url) {
            return r.map(|(code, b)| {
                let (start, end) = (range.start as usize, (range.end as usize).min(b.len()));
                (code, b.get(start..end).unwrap_or_default().to_vec())
            });
        }
        let wanted = format!("bytes={}-{}", range.start, range.end.saturating_sub(1));
        let mut span = call_span(None, "GET", url);
        let r = bytes_with("GET", url, None, &span.traceparent(), &[("range", &wanted)]).await;
        match &r {
            Ok((code, b)) => {
                span.set("http.response.status_code", *code);
                span.set("http.response.body.size", b.len() as u64);
            }
            Err(e) => span.fail(e.clone()),
        }
        r
    }

    async fn bytes_raw(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        traceparent: &str,
    ) -> Result<(u16, Vec<u8>), String> {
        bytes_with(method, url, body, traceparent, &[]).await
    }

    async fn bytes_with(
        method: &str,
        url: &str,
        body: Option<&[u8]>,
        traceparent: &str,
        extra: &[(&str, &str)],
    ) -> Result<(u16, Vec<u8>), String> {
        let auth = headers();
        let mut h: Vec<(&str, &str)> = vec![("traceparent", traceparent)];
        h.extend(auth.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        h.extend_from_slice(extra);
        drive_dioxus::env::http_bytes(
            method,
            &drive_dioxus::mount::api(url),
            &h,
            body.map(<[u8]>::to_vec),
        )
        .await
        .ok_or_else(|| "the node did not answer".to_string())
    }

    /// A file's waveform, `width` columns of (min, max), from its peaks
    /// sidecar (made on the computer that has it, kept by the node).
    pub async fn peaks(h: &Hit, width: usize) -> Result<(Vec<(f32, f32)>, f64), String> {
        let url = sidecar_url(h, "peaks").ok_or("no content key")?;
        let (status, b) = bytes("GET", &url, None).await?;
        if status != 200 {
            let why: Value = serde_json::from_slice(&b).unwrap_or(Value::Null);
            return Err(why["why"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("no waveform ({status})")));
        }
        let p = drive_sidecars::peaks::Peaks::decode(&b)?;
        Ok((p.columns(width), p.seconds()))
    }

    /// A small text file's text (an LRC, a keyflow chart).
    pub async fn text(h: &Hit) -> Option<String> {
        match bytes("GET", &file_url(h), None).await {
            Ok((200 | 206, b)) => String::from_utf8(b).ok(),
            _ => None,
        }
    }

    /// Open a PDF in a new tab (the node hands files out as downloads; a
    /// blob of the right type opens in the browser's viewer).
    #[cfg(target_arch = "wasm32")]
    pub async fn open_pdf(url: &str, name: &str) {
        let Ok((200, b)) = bytes("GET", url, None).await else {
            return;
        };
        let arr = js_sys::Array::new();
        arr.push(&js_sys::Uint8Array::from(b.as_slice()).into());
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("application/pdf");
        let Ok(blob) = web_sys::Blob::new_with_u8_array_sequence_and_options(&arr, &opts) else {
            return;
        };
        if let (Ok(u), Some(w)) = (
            web_sys::Url::create_object_url_with_blob(&blob),
            web_sys::window(),
        ) {
            let _ = w.open_with_url_and_target(&u, "_blank");
        }
        let _ = name;
    }

    /// Natively: the system's viewer opens it, from the node.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn open_pdf(url: &str, _name: &str) {
        drive_dioxus::env::go(&absolute(url.to_string()));
    }

    /// Keep bytes on the node: their full hash.
    pub async fn put_blob(b: &[u8]) -> Result<String, String> {
        let hash = drive_files::hash::of_bytes(b);
        match bytes("PUT", &format!("/blobs/{hash}"), Some(b)).await? {
            (200..=299, _) => Ok(hash),
            (s, _) => Err(format!("the node refused it ({s})")),
        }
    }
}

/// Files in Studio's own folder (`files::DISK`), read from the disk: a
/// native build's sets and charts without Drive. A browser has none.
#[cfg_attr(not(feature = "drive"), allow(dead_code))]
pub mod disk {
    use super::*;

    const SCHEME: &str = "file://";

    /// A disk file's URL: `file://` and its path.
    pub fn url(path: &str) -> String {
        match crate::storage::resolve(path) {
            Some(p) => format!("{SCHEME}{}", p.display()),
            None => format!("{SCHEME}{path}"),
        }
    }

    /// A `file://` URL's bytes (`None`: not a disk URL).
    pub fn bytes(url: &str) -> Option<Result<(u16, Vec<u8>), String>> {
        let path = url.strip_prefix(SCHEME)?;
        Some(
            std::fs::read(path)
                .map(|b| (200, b))
                .map_err(|e| format!("{path}: {e}")),
        )
    }

    /// A folder's entries (a path in the root, or absolute), as hits.
    pub fn list(space: &str, path: &str) -> Vec<Hit> {
        let Some(dir) = crate::storage::resolve(path) else {
            return Vec::new();
        };
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut out: Vec<Hit> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let md = e.metadata().ok()?;
                Some(hit(
                    space,
                    &format!("{}/{name}", path.trim_end_matches('/')),
                    &md,
                ))
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn hit(space: &str, path: &str, md: &std::fs::Metadata) -> Hit {
        let mtime = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as i64);
        Hit {
            space: space.into(),
            location: files::DISK.into(),
            location_name: "Studio".into(),
            online: true,
            path: path.into(),
            name: path.rsplit('/').next().unwrap_or(path).into(),
            dir: md.is_dir(),
            size: md.len() as i64,
            mtime,
            ..Default::default()
        }
    }
}

/// Drive's agent on this computer, for the desktop build: warm files
/// here ahead of a set, hold them while it is open, and open any of them
/// as a real path (`drive-local`, docs/app-files.md in Drive).
#[cfg(all(feature = "local", not(target_arch = "wasm32")))]
pub mod local {
    use std::sync::mpsc;

    use drive_local::{Client, Event, Priority, Warm};

    /// What a warm job says as it goes.
    #[derive(Clone, Debug, PartialEq, Default)]
    pub struct WarmState {
        pub files: u64,
        pub files_done: u64,
        pub files_failed: u64,
        pub bytes: u64,
        pub bytes_done: u64,
        pub done: bool,
        /// Targets that are here now (its own file, cached or fetched).
        pub here: Vec<String>,
        pub error: Option<String>,
    }

    /// A connection to the agent for as long as a set is open: what it
    /// warms stays pinned until it is dropped.
    pub struct Holder {
        client: Client,
    }

    impl Holder {
        pub fn connect() -> Result<Holder, String> {
            Client::connect("studio")
                .map(|client| Holder { client })
                .map_err(|e| e.to_string())
        }

        #[allow(dead_code)]
        pub fn connect_in(dir: &std::path::Path) -> Result<Holder, String> {
            Client::connect_in(dir, "studio")
                .map(|client| Holder { client })
                .map_err(|e| e.to_string())
        }

        /// Bring these files here, reporting each step on `tx`; the
        /// files stay pinned while this holder lives.
        pub fn warm(
            &self,
            targets: Vec<String>,
            tx: mpsc::Sender<WarmState>,
        ) -> Result<(), String> {
            let job = self
                .client
                .warm(Warm {
                    targets,
                    priority: Priority::High,
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
            let mut st = WarmState {
                files: job.started.files,
                bytes: job.started.bytes,
                ..Default::default()
            };
            let _ = tx.send(st.clone());
            while let Some(ev) = job.recv() {
                let (p, done) = match ev {
                    Event::Progress(p) => (p, false),
                    Event::Done(p) => (p, true),
                    _ => continue,
                };
                st.files = p.files;
                st.files_done = p.files_done;
                st.files_failed = p.files_failed;
                st.bytes = p.bytes;
                st.bytes_done = p.bytes_done;
                if let Some(f) = &p.file
                    && f.state == drive_local::FileState::Ready
                    && !st.here.contains(&f.target)
                {
                    st.here.push(f.target.clone());
                }
                st.done = done;
                let _ = tx.send(st.clone());
                if done {
                    break;
                }
            }
            Ok(())
        }

        /// A real path for a file (its own, on this computer, or a cached
        /// copy fetched for it).
        pub fn open(&self, target: &str) -> Result<std::path::PathBuf, String> {
            self.client
                .open(target)
                .map(|f| f.path)
                .map_err(|e| e.to_string())
        }

        /// What Studio holds pinned now (the targets it gave).
        #[allow(dead_code)]
        pub fn pinned(&self) -> Result<Vec<String>, String> {
            self.client
                .pins()
                .map(|v| {
                    v.into_iter()
                        .flat_map(|p| p.holders)
                        .filter(|h| h.app == "studio")
                        .map(|h| h.target)
                        .collect()
                })
                .map_err(|e| e.to_string())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn no_agent_says_so() {
            let dir =
                std::env::temp_dir().join(format!("studio-proto-noagent-{}", std::process::id()));
            let e = Holder::connect_in(&dir).err().unwrap_or_default();
            assert!(e.contains("not running"), "{e}");
        }

        /// Against a running agent: `DRIVE_AGENT_DIR=… STUDIO_WARM=TARGET,…
        /// cargo test --features local -- --ignored warm_and_hold`.
        #[test]
        #[ignore]
        fn warm_and_hold() {
            let dir = std::env::var("DRIVE_AGENT_DIR").expect("DRIVE_AGENT_DIR");
            let targets: Vec<String> = std::env::var("STUDIO_WARM")
                .expect("STUDIO_WARM")
                .split(',')
                .map(str::to_string)
                .collect();
            let h = Holder::connect_in(std::path::Path::new(&dir)).unwrap();
            let (tx, rx) = mpsc::channel();
            h.warm(targets.clone(), tx).unwrap();
            let last = rx.try_iter().last().unwrap();
            assert!(last.done);
            assert_eq!(last.files_failed, 0, "{last:?}");
            assert_eq!(last.files_done, last.files);
            // Held while the set is open.
            let pinned = h.pinned().unwrap();
            assert!(!pinned.is_empty(), "{pinned:?}");
            for t in &targets {
                let p = h.open(t).unwrap();
                assert!(p.exists(), "{}", p.display());
            }
            drop(h);
            // The set closed: its connection's leases ended.
            std::thread::sleep(std::time::Duration::from_millis(300));
            let h2 = Holder::connect_in(std::path::Path::new(&dir)).unwrap();
            assert!(h2.pinned().unwrap().is_empty());
        }
    }
}
