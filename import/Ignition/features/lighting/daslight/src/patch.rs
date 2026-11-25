//! The patch (`PATCHS DATA`): which profile sits at which address.
//!
//! Inside the payload, fixtures are grouped by type:
//!
//! ```text
//! PATCH NBFIXTURE=n
//!   FIXTURES                 one block per profile
//!     SSLLIBRARY ...         the whole profile, embedded
//!     FIXTURE DASUID NAME ADDRESS UNIVERS POSX POSY ANGLE OUTMODE FLAG
//!       BEAM INDEX POSX POSY one per cell, on the desk's 2D stage view
//! ```

use crate::Error;
use crate::container::{Encoding, pack_raw};
use crate::profile::{cells, channels};
use crate::xml::Element;

/// The desk draws a cell 30 units across; profile cell grids are in
/// hundredths of a cell.
const CELL: i32 = 30;

/// One patched fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fixture {
    /// `DASUID`: what scenes and groups refer to.
    pub uid: String,
    pub name: String,
    /// The `SSLFIXUID` of its profile.
    pub profile: String,
    /// 1-based start address.
    pub address: u16,
    /// 1-based universe.
    pub universe: u16,
    /// Position on the desk's 2D stage view (schematic, not metric).
    pub x: i32,
    pub y: i32,
    pub angle: i32,
}

impl Fixture {
    /// A fixture whose `DASUID` is derived from its name, so regenerating a
    /// showfile keeps every fixture's identity.
    #[must_use]
    pub fn named(name: &str, profile: &str, address: u16) -> Self {
        Self {
            uid: stable_uid(&format!("fixture:{name}")),
            name: name.to_owned(),
            profile: profile.to_owned(),
            address,
            universe: 1,
            x: 0,
            y: 0,
            angle: 0,
        }
    }

    #[must_use]
    pub const fn at(mut self, x: i32, y: i32, angle: i32) -> Self {
        self.x = x;
        self.y = y;
        self.angle = angle;
        self
    }
}

/// A `DASUID`-shaped id that is the same for the same key.
#[must_use]
pub fn stable_uid(key: &str) -> String {
    uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_URL,
        format!("ignition-daslight:{key}").as_bytes(),
    )
    .to_string()
}

/// The patch: profiles and the fixtures that use them.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    /// `SSLLIBRARY` elements, one per profile.
    pub profiles: Vec<Element>,
    pub fixtures: Vec<Fixture>,
}

impl Patch {
    /// Reads a decoded `PATCH` element.
    #[must_use]
    pub fn from_element(patch: &Element) -> Self {
        let mut out = Self::default();
        for block in patch.descendants("FIXTURES") {
            let Some(lib) = block.find("SSLLIBRARY") else {
                continue;
            };
            let uid = lib.get("SSLFIXUID").unwrap_or_default().to_owned();
            if !out
                .profiles
                .iter()
                .any(|p| p.get("SSLFIXUID") == Some(uid.as_str()))
            {
                out.profiles.push(lib.clone());
            }
            for f in block.elements().filter(|e| e.name == "FIXTURE") {
                let num = |k: &str| f.get(k).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                out.fixtures.push(Fixture {
                    uid: f.get("DASUID").unwrap_or_default().to_owned(),
                    name: f.get("NAME").unwrap_or_default().to_owned(),
                    profile: uid.clone(),
                    address: u16::try_from(num("ADDRESS")).unwrap_or(0),
                    universe: u16::try_from(num("UNIVERS")).unwrap_or(1),
                    x: num("POSX"),
                    y: num("POSY"),
                    angle: num("ANGLE"),
                });
            }
        }
        out
    }

    /// A profile by its `SSLFIXUID`.
    #[must_use]
    pub fn profile(&self, uid: &str) -> Option<&Element> {
        self.profiles
            .iter()
            .find(|p| p.get("SSLFIXUID") == Some(uid))
    }

    /// A profile by its library name (`UKing/ZQ01334.ssl2`).
    #[must_use]
    pub fn profile_named(&self, name: &str) -> Option<&Element> {
        self.profiles
            .iter()
            .find(|p| p.get("SSLNAME") == Some(name))
    }

    /// Adds a profile, replacing one with the same `SSLFIXUID`.
    pub fn add_profile(&mut self, library: Element) {
        let uid = library.get("SSLFIXUID").map(str::to_owned);
        self.profiles
            .retain(|p| p.get("SSLFIXUID").map(str::to_owned) != uid);
        self.profiles.push(library);
    }

    /// How many channels a fixture uses.
    #[must_use]
    pub fn footprint(&self, fixture: &Fixture) -> usize {
        self.profile(&fixture.profile)
            .map_or(0, |p| channels(p).len())
    }

    /// Every fixture's channel span, checked: each has a profile, starts at
    /// 1 or more, ends by 512, and overlaps nothing.
    ///
    /// # Errors
    /// The first problem found, naming the fixtures.
    pub fn check(&self) -> Result<(), Error> {
        let mut spans: Vec<(u16, usize, usize, &str)> = Vec::new();
        for f in &self.fixtures {
            if self.profile(&f.profile).is_none() {
                return Err(Error::Patch(format!(
                    "{} has no profile {}",
                    f.name, f.profile
                )));
            }
            let start = usize::from(f.address);
            let end = start.saturating_add(self.footprint(f)).saturating_sub(1);
            if start == 0 || end > 512 {
                return Err(Error::Patch(format!(
                    "{} runs {start}-{end}, outside 1-512",
                    f.name
                )));
            }
            if let Some(other) = spans
                .iter()
                .find(|(u, s, e, _)| *u == f.universe && start <= *e && *s <= end)
            {
                return Err(Error::Patch(format!(
                    "{} ({start}-{end}) overlaps {} ({}-{})",
                    f.name, other.3, other.1, other.2
                )));
            }
            spans.push((f.universe, start, end, &f.name));
        }
        Ok(())
    }

    /// The `PATCH` element, fixtures grouped by profile in first-use order.
    ///
    /// # Errors
    /// [`Patch::check`] fails, or a payload cannot be packed.
    pub fn to_element(&self) -> Result<Element, Error> {
        self.check()?;
        let mut patch = Element::new("PATCH").with("NBFIXTURE", self.fixtures.len());
        let mut order: Vec<&str> = Vec::new();
        for f in &self.fixtures {
            if !order.contains(&f.profile.as_str()) {
                order.push(&f.profile);
            }
        }
        let mut index = 0_usize;
        for uid in order {
            let lib = self
                .profile(uid)
                .ok_or_else(|| Error::Patch(format!("no profile {uid}")))?;
            let chans = channels(lib);
            let grid = cells(lib);
            let outmode = pack_raw(&vec![0_u8; chans.len()], Encoding::Hex)?;
            let flag = pack_raw(
                &chans.iter().map(|c| c.kind.flag()).collect::<Vec<_>>(),
                Encoding::Hex,
            )?;
            let mut block = Element::new("FIXTURES").child(lib.clone());
            for f in self.fixtures.iter().filter(|f| f.profile == uid) {
                index = index.saturating_add(1);
                let mut el = Element::new("FIXTURE")
                    .with("TYPE", 0)
                    .with("DASUID", &f.uid)
                    .with("NAME", &f.name)
                    .with("INDEX", index)
                    .with("SHAPE", 1)
                    .with("SIZE", CELL)
                    .with("POSX", f.x)
                    .with("POSY", f.y)
                    .with("ANGLE", f.angle)
                    .with("ADDRESS", f.address)
                    .with("UNIVERS", f.universe)
                    .with("OUTMODE", &outmode)
                    .with("FLAG", &flag);
                let beams: Vec<(i32, i32)> = if grid.is_empty() {
                    vec![(0, 0)]
                } else {
                    grid.clone()
                };
                for (i, (bx, by)) in beams.iter().enumerate() {
                    let dx = bx.saturating_mul(CELL).checked_div(100).unwrap_or(0);
                    let dy = by.saturating_mul(CELL).checked_div(100).unwrap_or(0);
                    el.push(
                        Element::new("BEAM")
                            .with("INDEX", i)
                            .with("POSX", f.x.saturating_add(dx))
                            .with("POSY", f.y.saturating_add(dy)),
                    );
                }
                block.push(el);
            }
            patch.push(block);
        }
        Ok(patch)
    }
}
