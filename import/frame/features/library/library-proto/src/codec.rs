//! [`Content`] ⇄ [`Scene`]: markup text or a `.frame` bundle.

use std::collections::BTreeMap;

use document_proto::Scene;

use crate::{Content, Format};

impl Content {
    /// `scene` as frame markup.
    #[must_use]
    pub fn markup(scene: &Scene) -> Self {
        Self { format: Format::Markup, bytes: document_text::print(scene).into_bytes() }
    }

    /// Markup text as content, unparsed.
    #[must_use]
    pub fn markup_text(text: impl Into<String>) -> Self {
        Self { format: Format::Markup, bytes: text.into().into_bytes() }
    }

    /// `scene` as a `.frame` bundle titled `title`.
    ///
    /// # Errors
    /// When the bundle can't be written (an asset without bytes).
    pub fn frame(scene: &Scene, title: &str) -> Result<Self, String> {
        let bytes = document_bundle::to_bytes(scene, &document_bundle::Manifest::new(title)).map_err(|e| e.to_string())?;
        Ok(Self { format: Format::Frame, bytes })
    }

    /// Decode to a scene. Errors are one line; markup errors are
    /// `line:col: message`.
    ///
    /// # Errors
    /// Not UTF-8, a markup parse error, or a broken bundle.
    pub fn decode(&self) -> Result<Scene, String> {
        match self.format {
            Format::Markup => {
                let text = std::str::from_utf8(&self.bytes).map_err(|e| format!("not UTF-8: {e}"))?;
                document_text::parse(text, BTreeMap::new()).map_err(|e| e.to_string())
            }
            Format::Frame => document_bundle::from_bytes(&self.bytes).map(|(_, doc)| doc.scene()).map_err(|e| e.to_string()),
        }
    }

    /// The bundle's manifest title (`.frame` only).
    #[must_use]
    pub fn title(&self) -> Option<String> {
        match self.format {
            Format::Frame => document_bundle::from_bytes(&self.bytes).ok().map(|(m, _)| m.title),
            Format::Markup => None,
        }
    }
}

/// A display name for a surface: its first page's first top-level node
/// (`Vari-Mu`), else `None`.
#[must_use]
pub fn scene_name(scene: &Scene) -> Option<String> {
    let page = *scene.pages().first()?;
    let top = *scene.node(page)?.children.first()?;
    let name = scene.node(top)?.name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r##"<Document><Page><Frame name="Panel" w=100 h=50 fill="#101010" /></Page></Document>"##;

    #[test]
    fn markup_and_bundle_round_trip() {
        let scene = document_text::parse(SRC, BTreeMap::new()).unwrap();
        assert_eq!(Content::markup(&scene).decode().unwrap(), scene);
        let frame = Content::frame(&scene, "Panel").unwrap();
        assert_eq!(frame.decode().unwrap(), scene);
        assert_eq!(frame.title().as_deref(), Some("Panel"));
        assert_eq!(scene_name(&scene).as_deref(), Some("Panel"));
    }

    #[test]
    fn broken_content_errors() {
        let err = Content::markup_text("<Document>\n  <Page>\n</Document>").decode().unwrap_err();
        assert!(err.split(':').next().is_some_and(|l| l.parse::<usize>().is_ok()), "{err}");
        assert!(Content { format: Format::Frame, bytes: b"nope".to_vec() }.decode().is_err());
    }
}
