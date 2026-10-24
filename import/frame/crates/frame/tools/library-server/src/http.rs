//! A small read-only HTTP side: the index, downloads, assets and previews,
//! for browsers and `curl`. HTTP/1.1, GET only, one response per connection.
//!
//! A markup surface's textures and fonts are served by id at
//! `assets/<id>`, and its index entry lists them, so a download of the
//! `.frame` can be completed into `<name>.frame.assets/` (or fetched whole over
//! the rpc, whose `Content` carries them).

use std::borrow::Cow;

use frame::library::fs::{FsLibrary, ListQuery, SurfaceStore as _};
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

pub async fn serve(listener: tokio::net::TcpListener, lib: FsLibrary) {
    while let Ok((mut tcp, _)) = listener.accept().await {
        let lib = lib.clone();
        tokio::spawn(async move {
            let mut buf = vec![0u8; 8192];
            let Ok(n) = tcp.read(&mut buf).await else {
                return;
            };
            let request = String::from_utf8_lossy(&buf[..n]);
            let (status, mime, extra, body) = respond(&lib, &request);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n{extra}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = tcp.write_all(head.as_bytes()).await;
            let _ = tcp.write_all(&body).await;
            let _ = tcp.shutdown().await;
        });
    }
}

type Response = (&'static str, Cow<'static, str>, String, Vec<u8>);

fn not_found(what: &str) -> Response {
    (
        "404 Not Found",
        "text/plain; charset=utf-8".into(),
        String::new(),
        format!("not found: {what}\n").into_bytes(),
    )
}

/// Answer one request (the raw request text).
pub fn respond(lib: &FsLibrary, request: &str) -> Response {
    let mut first = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace();
    let (method, target) = (
        first.next().unwrap_or_default(),
        first.next().unwrap_or("/"),
    );
    if method != "GET" {
        return (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8".into(),
            String::new(),
            b"GET only\n".to_vec(),
        );
    }
    let path = decode(target.split('?').next().unwrap_or("/"));
    let path = path.trim_start_matches('/');
    if path.is_empty() || path == "index.json" {
        let index: Vec<_> = lib
            .list(ListQuery::default())
            .into_iter()
            .map(|m| {
                let assets: Vec<String> = lib
                    .get(m.id.clone(), None)
                    .map(|s| s.content.assets.iter().map(|a| format!("assets/{}", a.id)).collect())
                    .unwrap_or_default();
                json!({
                    "id": m.id, "name": m.name, "author": m.author, "description": m.description,
                    "tags": m.tags, "version": m.version, "license": m.license, "forked_from": m.forked_from,
                    "fits": m.fits, "preview": (!m.preview_png.is_empty()).then(|| format!("previews/{}.png", m.id)),
                    "download": format!("surfaces/{}", m.id),
                    "assets": assets,
                })
            })
            .collect();
        let body = serde_json::to_vec_pretty(&index).unwrap_or_default();
        return ("200 OK", "application/json".into(), String::new(), body);
    }
    if let Some(id) = path
        .strip_prefix("previews/")
        .and_then(|p| p.strip_suffix(".png"))
    {
        return match lib.get(id.to_owned(), None) {
            Ok(s) if !s.meta.preview_png.is_empty() => (
                "200 OK",
                "image/png".into(),
                String::new(),
                s.meta.preview_png,
            ),
            _ => not_found(path),
        };
    }
    if let Some(id) = path.strip_prefix("assets/") {
        return match lib.asset(id) {
            Some(a) => {
                let name = format!("{}.{}", a.id, frame::document::io::extension_for(&a.mime));
                (
                    "200 OK",
                    a.mime.into(),
                    format!("Content-Disposition: attachment; filename=\"{name}\"\r\n"),
                    a.bytes,
                )
            }
            None => not_found(path),
        };
    }
    if let Some(id) = path.strip_prefix("surfaces/") {
        return match lib.get(id.to_owned(), None) {
            Ok(s) => {
                let ext = s.content.format.extension();
                let name = id.rsplit('/').next().unwrap_or(id);
                let mime = if s.content.format == frame::library::link::Format::Markup {
                    "text/plain; charset=utf-8"
                } else {
                    "application/zip"
                };
                (
                    "200 OK",
                    mime.into(),
                    format!("Content-Disposition: attachment; filename=\"{name}.{ext}\"\r\n"),
                    s.content.bytes,
                )
            }
            Err(_) => not_found(path),
        };
    }
    not_found(path)
}

/// `%XX` escapes decoded.
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
