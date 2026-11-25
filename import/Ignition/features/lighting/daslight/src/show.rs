//! A whole showfile.
//!
//! ```text
//! DLMFILE TYPE="Daslight" VERSION="5"
//!   CONFIGURATION   view and touch-dock state
//!   PATCHS DATA=    the patch, packed (crate::patch)
//!   FIXTUREGROUPS   named fixture selections
//!   SCENES          banks of scenes; scene data is per fixture DASUID
//!   SHORTCUTS       keyboard/MIDI triggers, pointing at scenes
//!   TOUCH           touch pages, pointing at scenes
//!   DEVICES         the interface (myDMX 5 Express), its zone's scenes
//! ```
//!
//! A new showfile is made from an existing one, so the parts this crate does
//! not model (the interface, the view) are the desk's own.

use std::path::Path;

use crate::Error;
use crate::container::{Encoding, pack_prefixed, unpack_prefixed};
use crate::patch::{Patch, stable_uid};
use crate::scene::Scene;
use crate::xml::{Element, Node};

/// A Daslight 5 showfile, held as its XML tree.
#[derive(Debug, Clone)]
pub struct Showfile {
    root: Element,
}

impl Showfile {
    /// Reads a `.dvc` file.
    ///
    /// # Errors
    /// The file cannot be read, or is not a Daslight 5 showfile.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let text =
            std::fs::read_to_string(path).map_err(|e| Error::Io(path.display().to_string(), e))?;
        Self::parse(&text)
    }

    /// Parses a showfile's text.
    ///
    /// # Errors
    /// Not XML, or not rooted at `DLMFILE TYPE="Daslight"`.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let root = Element::parse(text)?;
        if root.name != "DLMFILE" || root.get("TYPE") != Some("Daslight") {
            return Err(Error::Xml(
                "not a Daslight showfile (no DLMFILE TYPE=\"Daslight\")".into(),
            ));
        }
        Ok(Self { root })
    }

    /// The showfile as the desk writes it.
    #[must_use]
    pub fn to_text(&self) -> String {
        format!("\n{}\n", self.root.to_xml())
    }

    /// Writes the showfile.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save(&self, path: &Path) -> Result<(), Error> {
        std::fs::write(path, self.to_text()).map_err(|e| Error::Io(path.display().to_string(), e))
    }

    #[must_use]
    pub const fn root(&self) -> &Element {
        &self.root
    }

    /// The patch.
    ///
    /// # Errors
    /// The `PATCHS DATA` payload is missing or corrupt.
    pub fn patch(&self) -> Result<Patch, Error> {
        let data = self
            .root
            .find("PATCHS")
            .and_then(|p| p.get("DATA"))
            .ok_or(Error::Container("no PATCHS DATA"))?;
        let body = unpack_prefixed(data)?;
        let text = String::from_utf8(body).map_err(|_| Error::Container("patch is not UTF-8"))?;
        Ok(Patch::from_element(&Element::parse(&text)?))
    }

    /// Replaces the patch.
    ///
    /// # Errors
    /// The patch does not check out, or cannot be packed.
    pub fn set_patch(&mut self, patch: &Patch) -> Result<(), Error> {
        let packed = pack_prefixed(patch.to_element()?.to_xml().as_bytes(), Encoding::Base64)?;
        let slot = self
            .root
            .find_mut("PATCHS")
            .ok_or(Error::Container("no PATCHS"))?;
        slot.set("DATA", packed);
        Ok(())
    }

    /// Replaces every fixture group. Each is `(name, fixture DASUIDs)`.
    pub fn set_groups(&mut self, groups: &[(&str, Vec<String>)]) {
        let mut el = Element::new("FIXTUREGROUPS");
        for (name, uids) in groups {
            let mut fixtures = Element::new("FIXTURES");
            for uid in uids {
                fixtures.push(Element::new("FIXTURE").with("DASUID", uid));
            }
            el.push(
                Element::new("FIXTUREGROUP")
                    .with("DASUID", stable_uid(&format!("group:{name}")))
                    .with("NAME", name)
                    .with("TOGGLE_BLACKOUT", 0)
                    .with("TOGGLE_STROBE", 0)
                    .with("TOGGLE_FULL", 0)
                    .with("TOGGLE_EXCLUSIVE_FULL", 0)
                    .with("TOGGLE_FLASH", 0)
                    .with("TOGGLE_EXCLUSIVE_FLASH", 0)
                    .child(fixtures)
                    .child(Element::new("BEAMSELECTION")),
            );
        }
        replace_child(&mut self.root, el);
    }

    /// Adds a bank of scenes after the existing ones, replacing a bank of
    /// the same name. In a bank one scene plays at a time, so pressing the
    /// next scene releases the last.
    ///
    /// # Errors
    /// A scene names a fixture the patch does not have, or has the wrong
    /// number of values for it.
    pub fn add_bank(
        &mut self,
        name: &str,
        color: &str,
        scenes: &[Scene],
        patch: &Patch,
    ) -> Result<(), Error> {
        let mut bank = Element::new("BANK")
            .with("DASUID", stable_uid(&format!("bank:{name}")))
            .with("NAME", name)
            .with("COLOR", color)
            .with("COLLAPSED", 0)
            .with("PAUSED", 0)
            .with("HIDDEN", 0)
            .with("NBSCENE", scenes.len());
        for s in scenes {
            bank.push(s.to_element(name, patch)?);
        }
        let list = self
            .root
            .find_mut("SCENES")
            .ok_or(Error::Container("no SCENES"))?;
        list.children.retain(
            |n| !matches!(n, Node::Element(e) if e.name == "BANK" && e.get("NAME") == Some(name)),
        );
        let at = list
            .children
            .iter()
            .position(|n| matches!(n, Node::Element(e) if e.name == "DAS_SELECTIONS"))
            .unwrap_or(list.children.len());
        list.children.insert(at, Node::Element(bank));
        Ok(())
    }

    /// The names of every scene, by bank.
    #[must_use]
    pub fn scene_names(&self) -> Vec<(String, Vec<String>)> {
        self.root
            .find("SCENES")
            .map(|s| {
                s.elements()
                    .filter(|b| b.name == "BANK")
                    .map(|b| {
                        let scenes = b.elements().filter(|e| e.name == "SCENE");
                        (
                            b.get("NAME").unwrap_or_default().to_owned(),
                            scenes
                                .map(|e| e.get("NAME").unwrap_or_default().to_owned())
                                .collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Starts programming over: every scene, shortcut and touch control is
    /// removed, leaving the named empty banks. Scenes store raw values per
    /// fixture in the old patch's channel layout, so after a repatch they
    /// would play the wrong channels.
    pub fn clear_programming(&mut self, banks: &[&str]) {
        let mut scenes = Element::new("SCENES");
        for name in banks {
            scenes.push(
                Element::new("BANK")
                    .with("DASUID", stable_uid(&format!("bank:{name}")))
                    .with("NAME", name)
                    .with("COLOR", "#ff6f6f6f")
                    .with("COLLAPSED", 0)
                    .with("PAUSED", 0)
                    .with("HIDDEN", 0)
                    .with("NBSCENE", 0),
            );
        }
        scenes.push(Element::new("DAS_SELECTIONS").with("SELECTED_LIVE_BANK", 0));
        replace_child(&mut self.root, scenes);
        replace_child(&mut self.root, Element::new("SHORTCUTS"));

        if let Some(touch) = self.root.find_mut("TOUCH") {
            let first = touch.elements().find(|e| e.name == "TOUCHPAGE").cloned();
            touch.clear_children();
            if let Some(mut page) = first {
                page.clear_children();
                page.set("TOUCHCONTROL", 0);
                touch.push(page);
            }
        }
        if let Some(devices) = self.root.find_mut("DEVICES") {
            for dev in devices.elements_mut() {
                for zones in dev.elements_mut().filter(|e| e.name == "DASZONES") {
                    for zone in zones.elements_mut() {
                        if let Some(s) = zone.find_mut("SCENES") {
                            s.clear_children();
                            s.set("DASTLNB", 0);
                        }
                    }
                }
            }
        }
    }
}

/// Replaces the root's child of the same name in place, or appends it.
fn replace_child(root: &mut Element, new: Element) {
    for node in &mut root.children {
        if let Node::Element(e) = node
            && e.name == new.name
        {
            *e = new;
            return;
        }
    }
    root.push(new);
}
