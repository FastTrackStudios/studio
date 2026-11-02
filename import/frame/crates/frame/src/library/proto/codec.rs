//! [`Content`] ⇄ [`Scene`]: markup text (plus its assets) or a `.frame` bundle.

use crate::document::proto::{Asset, Scene};

use crate::library::proto::{Content, Format};

impl Content {
    /// `scene` as frame markup, with the assets it carries.
    #[must_use]
    pub fn markup(scene: &Scene) -> Self {
        Self::markup_with(
            crate::document::text::print(scene),
            scene.assets.values().cloned(),
        )
    }

    /// Markup text as content, unparsed, without assets.
    #[must_use]
    pub fn markup_text(text: impl Into<String>) -> Self {
        Self::markup_with(text, [])
    }

    /// Markup text as content, unparsed, with the assets it names (as read
    /// from a `.frame` and its `.frame.assets/` sidecar).
    #[must_use]
    pub fn markup_with(text: impl Into<String>, assets: impl IntoIterator<Item = Asset>) -> Self {
        let mut assets: Vec<Asset> = assets.into_iter().collect();
        assets.sort_by(|a, b| a.id.cmp(&b.id));
        assets.dedup_by(|a, b| a.id == b.id);
        Self {
            format: Format::Markup,
            bytes: text.into().into_bytes(),
            assets,
        }
    }

    /// `scene` as a `.frame` bundle titled `title`.
    ///
    /// # Errors
    /// When the bundle can't be written (an asset without bytes).
    pub fn frame(scene: &Scene, title: &str) -> Result<Self, String> {
        let bytes = crate::document::bundle::to_bytes(
            scene,
            &crate::document::bundle::Manifest::new(title),
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            format: Format::Frame,
            bytes,
            assets: Vec::new(),
        })
    }

    /// Decode to a scene. Errors are one line; markup errors are
    /// `line:col: message`.
    ///
    /// # Errors
    /// Not UTF-8, a markup parse error, or a broken bundle.
    pub fn decode(&self) -> Result<Scene, String> {
        match self.format {
            Format::Markup => {
                let text =
                    std::str::from_utf8(&self.bytes).map_err(|e| format!("not UTF-8: {e}"))?;
                let assets = self
                    .assets
                    .iter()
                    .map(|a| (a.id.clone(), a.clone()))
                    .collect();
                crate::document::text::parse(text, assets).map_err(|e| e.to_string())
            }
            Format::Frame => crate::document::bundle::from_bytes(&self.bytes)
                .map(|(_, doc)| doc.scene())
                .map_err(|e| e.to_string()),
        }
    }

    /// The bundle's manifest title (`.frame` only).
    #[must_use]
    pub fn title(&self) -> Option<String> {
        match self.format {
            Format::Frame => crate::document::bundle::from_bytes(&self.bytes)
                .ok()
                .map(|(m, _)| m.title),
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
