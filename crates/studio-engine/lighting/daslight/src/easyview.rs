//! EasyView 2 projects (`.ev2`): myDMX's 3D visualizer.
//!
//! Read from a project EasyView saved itself (README.md, "EasyView"):
//!
//! ```text
//! EV2FILE VERSION="3" TYPE="EV2"
//!   CONFIGURATION   ambient, beams, fog, quality, scene size (cm)
//!   FIXTURES        one SSLLIBRARY per profile, embedded as in a .dvc
//!   OBJECTS
//!     OBJECT "Scene"     the room: floor, back, front, left, right, top
//!     OBJECT "Fixtures"  a group per fixture type, holding
//!       FIXTURE POSITION ROTATION SCALE SSLNAME SSLMODE ADDRESS UNIVERS DASUID
//! ```
//!
//! Centimetres, Y up, +Z towards the back wall, the room centred on the
//! origin at floor level. A fixture is linked to the desk by its `DASUID`,
//! the same id the showfile's patch gives it.

use crate::Error;
use crate::patch::Patch;
use crate::xml::Element;

/// The room as six planes, in centimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    pub width: f64,
    pub depth: f64,
    pub height: f64,
}

/// Where one patched fixture hangs.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    /// The fixture's `DASUID` in the patch.
    pub uid: String,
    /// Centimetres, EasyView's axes.
    pub position: [f64; 3],
    /// Degrees about X, Y, Z.
    pub rotation: [f64; 3],
}

/// The rotation, in EasyView's degrees, that turns a fixture hung
/// straight down (its rest pose) to throw along `dir` (EasyView axes):
/// a tilt about X, then a turn about Y.
#[must_use]
pub fn rotation_towards(dir: [f64; 3]) -> [f64; 3] {
    let [x, y, z] = dir;
    let len = x.mul_add(x, y.mul_add(y, z * z)).sqrt();
    if len <= f64::EPSILON {
        return [0.0, 0.0, 0.0];
    }
    let (x, y, z) = (x / len, y / len, z / len);
    let tilt = (-y).clamp(-1.0, 1.0).acos().to_degrees();
    let turn = if x.abs() + z.abs() <= 1e-9 {
        0.0
    } else {
        (-x).atan2(-z).to_degrees()
    };
    [round(tilt), round(turn), 0.0]
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn triple(v: [f64; 3]) -> String {
    format!("{},{},{}", round(v[0]), round(v[1]), round(v[2]))
}

/// An `OBJECT` or `OBJECT3D` with the attributes every one carries.
fn object(
    tag: &str,
    name: &str,
    kind: u8,
    selectable: bool,
    position: [f64; 3],
    rotation: [f64; 3],
    scale: [f64; 3],
) -> Element {
    Element::new(tag)
        .with("NAME", name)
        .with("TYPE", kind)
        .with("SELECTABLE", u8::from(selectable))
        .with("VISIBLE", 1)
        .with("LOCK", 0)
        .with("DRAWTEXTURE", 1)
        .with("POSITION", triple(position))
        .with("ROTATION", triple(rotation))
        .with("ROTATIONOFFSET", "0,0,0")
        .with("SCALE", triple(scale))
}

fn plane(name: &str, position: [f64; 3], rotation: [f64; 3], scale: [f64; 3]) -> Element {
    object("OBJECT3D", name, 1, false, position, rotation, scale)
        .with("CULLING", 0)
        .with("FILE3D", ":/library/plane.dae")
}

/// A surface's look: an opaque colour, `0xAARRGGBB` as EasyView stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Material {
    pub color: u32,
    /// 0 matte .. 100 glossy.
    pub specular: u8,
}

impl Material {
    /// An opaque colour from 0..255 red, green, blue.
    #[must_use]
    pub fn rgb(r: u8, g: u8, b: u8, specular: u8) -> Self {
        let color = 0xff00_0000_u32 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        Self { color, specular }
    }

    /// A colour as linear reflectance (0..1 a channel, as a glTF base
    /// colour), shown as the display value EasyView expects.
    #[must_use]
    pub fn linear(rgb: [f64; 3], specular: u8) -> Self {
        let [r, g, b] = rgb.map(|c| {
            let v = (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round();
            // In 0..=255 by the clamp.
            format!("{v}").parse::<u8>().unwrap_or(0)
        });
        Self::rgb(r, g, b, specular)
    }

    /// The attributes a coloured object carries. EasyView reads every one
    /// once `COLOR` is there, and crashes on a missing `TEXTURESCALE`.
    fn apply(self, el: Element) -> Element {
        el.with("COLOR", self.color)
            .with("TRANSPARENCY", 0)
            .with("EMISSIVE", 0)
            .with("SPECULAR", self.specular)
            .with("TEXTUREFILE", "")
            .with("TEXTURESCALE", "10,10")
    }
}

/// A coloured rectangle: EasyView's unit plane (horizontal, centred on its
/// origin) scaled to `size` (its X and Z) and turned by `rotation`.
///
/// `ROTATION` turns about X, then Y, then Z, each about the room's own
/// axes: a wall facing the audience is `[-90, 0, 0]` with its height in
/// `size[1]`; a side wall is `[0, -90, 90]` with its depth in `size[0]`.
#[must_use]
pub fn panel(
    name: &str,
    centre: [f64; 3],
    rotation: [f64; 3],
    size: [f64; 2],
    material: Material,
) -> Element {
    material.apply(plane(name, centre, rotation, [size[0], 1.0, size[1]]))
}

/// `v` turned `yaw` degrees about the vertical, as EasyView turns an
/// object by `ROTATION`'s Y.
fn turn(v: [f64; 3], yaw: f64) -> [f64; 3] {
    let (s, c) = yaw.to_radians().sin_cos();
    [v[0].mul_add(c, v[2] * s), v[1], (-v[0]).mul_add(s, v[2] * c)]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// A solid box, `size` wide (X), tall (Y) and deep (Z), centred on
/// `centre` and turned `yaw` degrees about the vertical: six panels in an
/// `OBJECT` named for it.
#[must_use]
pub fn block(name: &str, centre: [f64; 3], size: [f64; 3], yaw: f64, material: Material) -> Element {
    let [sx, sy, sz] = size;
    let (hx, hy, hz) = (sx / 2.0, sy / 2.0, sz / 2.0);
    let at = |v: [f64; 3]| add(centre, turn(v, yaw));
    let faces = [
        ("top", at([0.0, hy, 0.0]), [0.0, yaw, 0.0], [sx, sz]),
        ("bottom", at([0.0, -hy, 0.0]), [0.0, yaw, 0.0], [sx, sz]),
        ("front", at([0.0, 0.0, -hz]), [-90.0, yaw, 0.0], [sx, sy]),
        ("back", at([0.0, 0.0, hz]), [-90.0, yaw, 0.0], [sx, sy]),
        ("left", at([-hx, 0.0, 0.0]), [-90.0, yaw + 90.0, 0.0], [sz, sy]),
        ("right", at([hx, 0.0, 0.0]), [-90.0, yaw + 90.0, 0.0], [sz, sy]),
    ];
    let mut group = object("OBJECT", name, 0, true, [0.0; 3], [0.0; 3], [1.0; 3]);
    for (face, c, r, s) in faces {
        group.push(panel(&format!("{name} {face}"), c, r, s, material));
    }
    group
}

/// A mesh out of EasyView's own library (`./Library/…`, relative to the
/// `EasyView2` folder), stretched to fill a box.
#[derive(Debug, Clone, PartialEq)]
pub struct Model<'a> {
    /// `FILE3D`, e.g. `./Library/Music instrument/Drums.x`.
    pub file: &'a str,
    /// The mesh's own bounding box, in its units (what `assimp info`
    /// reports): the corner nearest minus infinity, then the farthest.
    pub bounds: [[f64; 3]; 2],
}

impl Model<'_> {
    /// The mesh standing with its footprint centred on `base` (the middle
    /// of its underside), `size` wide, tall and deep, turned `yaw`
    /// degrees about the vertical; coloured all over when `material` is
    /// given, else in the mesh's own materials.
    #[must_use]
    pub fn place(
        &self,
        name: &str,
        base: [f64; 3],
        size: [f64; 3],
        yaw: f64,
        material: Option<Material>,
    ) -> Element {
        let [lo, hi] = self.bounds;
        let scale: [f64; 3] = [0, 1, 2].map(|i| {
            let extent = hi.get(i).unwrap_or(&1.0) - lo.get(i).unwrap_or(&0.0);
            let want = size.get(i).copied().unwrap_or(1.0);
            if extent.abs() <= f64::EPSILON { 1.0 } else { want / extent }
        });
        // EasyView loads a mesh centred on its bounding box (seen in
        // EasyView 2.0.3: meshes placed by their own origin sank to the
        // waist), so the middle of the box goes halfway up from `base`.
        let middle = [0.0, size.get(1).copied().unwrap_or(0.0) / 2.0, 0.0];
        let el = object(
            "OBJECT3D",
            name,
            1,
            false,
            add(base, middle),
            [0.0, yaw, 0.0],
            scale,
        )
        .with("CULLING", 0)
        .with("FILE3D", self.file);
        match material {
            Some(m) => m.apply(el),
            None => el,
        }
    }
}

/// The project: the room, and every placed fixture with the profiles it
/// uses embedded.
///
/// # Errors
/// A placement names a fixture the patch does not have.
pub fn project(patch: &Patch, room: Room, placements: &[Placement]) -> Result<Element, Error> {
    project_in(patch, room, Vec::new(), placements)
}

/// The project in a modelled room: `scenery` (panels, blocks, models) is
/// the `Scene` object in place of the plain six-plane box; `room` still
/// sizes the scene. Empty `scenery` is the plain box.
///
/// # Errors
/// A placement names a fixture the patch does not have.
pub fn project_in(
    patch: &Patch,
    room: Room,
    scenery: Vec<Element>,
    placements: &[Placement],
) -> Result<Element, Error> {
    let Room {
        width,
        depth,
        height,
    } = room;
    let config = Element::new("CONFIGURATION")
        .with("AMBIENT", 0.3)
        .with("BEAMS", 0.25)
        .with("FOG", 0)
        .with("QUALITY", 32)
        .with("SCENESIZEX", round(width))
        .with("SCENESIZEY", round(height))
        .with("SCENESIZEZ", round(depth));
    // No CAMERAPOS / CAMERAROT: their convention is not known, and without
    // them EasyView opens on its default view of the whole room.

    let scene = if scenery.is_empty() {
        room_scene(room)
    } else {
        let mut scene = object("OBJECT", "Scene", 0, true, [0.0; 3], [0.0; 3], [1.0; 3]);
        for el in scenery {
            scene.push(el);
        }
        scene
    };
    // One group per fixture type, in patch order.
    let mut groups: Vec<(String, Element)> = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    for p in placements {
        let f = patch
            .fixtures
            .iter()
            .find(|f| f.uid == p.uid)
            .ok_or_else(|| Error::Patch(format!("no fixture {} in the patch", p.uid)))?;
        let lib = patch
            .profile(&f.profile)
            .ok_or_else(|| Error::Patch(format!("{} has no profile", f.name)))?;
        let ssl_name = lib.get("SSLNAME").unwrap_or_default().to_owned();
        let mode = lib
            .find("SSLMODES")
            .and_then(|m| m.find("SSLMODE"))
            .and_then(|m| m.get("SSLMODEINDEX"))
            .unwrap_or("0")
            .to_owned();
        if !used.contains(&f.profile.as_str()) {
            used.push(&f.profile);
        }
        let type_name = ssl_name
            .rsplit('/')
            .next()
            .unwrap_or(&ssl_name)
            .trim_end_matches(".ssl2")
            .to_owned();
        let fixture = object(
            "FIXTURE", &f.name, 2, true, p.position, p.rotation, [1.0; 3],
        )
        .with("SSLNAME", &ssl_name)
        .with("SSLMODE", mode)
        .with("ADDRESS", f.address)
        .with("UNIVERS", f.universe)
        .with("DASUID", &f.uid)
        .with("INVERTX", 0)
        .with("INVERTY", 0)
        .child(Element::new("BEAMS"));
        if let Some((_, g)) = groups.iter_mut().find(|(n, _)| *n == type_name) {
            g.push(fixture);
        } else {
            let g =
                object("OBJECT", &type_name, 0, true, [0.0; 3], [0.0; 3], [1.0; 3]).child(fixture);
            groups.push((type_name, g));
        }
    }
    let mut fixtures_obj = object("OBJECT", "Fixtures", 0, true, [0.0; 3], [0.0; 3], [1.0; 3]);
    for (_, g) in groups {
        fixtures_obj.push(g);
    }

    let mut libraries = Element::new("FIXTURES");
    for uid in used {
        if let Some(lib) = patch.profile(uid) {
            libraries.push(lib.clone());
        }
    }
    Ok(Element::new("EV2FILE")
        .with("VERSION", 3)
        .with("TYPE", "EV2")
        .with("VERSION_SOFT", "")
        .child(config)
        .child(libraries)
        .child(Element::new("OBJECTS").child(scene).child(fixtures_obj)))
}

/// The room: a floor, four walls and a ceiling.
fn room_scene(room: Room) -> Element {
    let Room {
        width,
        depth,
        height,
    } = room;
    let mut scene = object("OBJECT", "Scene", 0, true, [0.0; 3], [0.0; 3], [1.0; 3]);
    let (half_w, half_d, half_h) = (width / 2.0, depth / 2.0, height / 2.0);
    // A plane with a colour carries the whole material: EasyView reads
    // every one of these once COLOR is there, and crashes on a missing
    // TEXTURESCALE.
    scene.push(
        plane("floor", [0.0; 3], [0.0; 3], [width, 1.0, depth])
            .with("COLOR", 4_288_716_964_u32)
            .with("TRANSPARENCY", 0)
            .with("EMISSIVE", 0)
            .with("SPECULAR", 100)
            .with("TEXTUREFILE", "")
            .with("TEXTURESCALE", "10,10"),
    );
    scene.push(plane(
        "back",
        [0.0, half_h, half_d],
        [-90.0, 0.0, 0.0],
        [width, 1.0, height],
    ));
    scene.push(plane(
        "front",
        [0.0, half_h, -half_d],
        [-90.0, 180.0, 0.0],
        [width, 1.0, height],
    ));
    scene.push(plane(
        "left",
        [-half_w, half_h, 0.0],
        [0.0, -90.0, 90.0],
        [depth, 1.0, height],
    ));
    scene.push(plane(
        "right",
        [half_w, half_h, 0.0],
        [0.0, 90.0, 90.0],
        [depth, 1.0, height],
    ));
    scene.push(plane(
        "top",
        [0.0, height, 0.0],
        [180.0, 0.0, 0.0],
        [width, 1.0, depth],
    ));

    scene
}

/// The project as a file's text.
#[must_use]
pub fn to_text(project: &Element) -> String {
    format!("\n{}\n", project.to_xml())
}

/// A profile as a library file (`ScanLibrary/<SSLNAME>`), in the plain
/// form the desk reads alongside its encrypted ones.
#[must_use]
pub fn library_file(library: &Element) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><DLMFILE VERSION=\"3\" TYPE=\"\">{}</DLMFILE>",
        library.to_xml()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_is_straight_down() {
        assert_eq!(rotation_towards([0.0, -1.0, 0.0]), [0.0, 0.0, 0.0]);
        // Thrown towards the audience (-Z), level: tilted 90 degrees.
        assert_eq!(rotation_towards([0.0, 0.0, -1.0]), [90.0, 0.0, 0.0]);
        // Straight up: a mover standing on its base.
        assert_eq!(rotation_towards([0.0, 1.0, 0.0])[0], 180.0);
    }

    #[test]
    fn a_turn_of_ninety_takes_depth_to_width() {
        let v = turn([0.0, 0.0, 1.0], 90.0);
        assert!((v[0] - 1.0).abs() < 1e-9 && v[2].abs() < 1e-9);
    }

    #[test]
    fn a_block_is_six_coloured_faces_round_its_centre() {
        let b = block("box", [0.0, 50.0, 0.0], [200.0, 100.0, 40.0], 0.0, Material::rgb(1, 2, 3, 0));
        let faces: Vec<&Element> = b.elements().collect();
        assert_eq!(faces.len(), 6);
        let top = faces.first().and_then(|f| f.get("POSITION"));
        assert_eq!(top, Some("0,100,0"));
        assert!(faces.iter().all(|f| f.get("COLOR") == Some("4278256131")));
    }

    #[test]
    fn a_model_fills_its_box_from_the_floor_up() {
        let m = Model { file: "./Library/x.x", bounds: [[-25.0, 0.0, -10.0], [25.0, 50.0, 10.0]] };
        let el = m.place("m", [100.0, 0.0, 0.0], [100.0, 200.0, 20.0], 0.0, None);
        assert_eq!(el.get("SCALE"), Some("2,4,1"));
        assert_eq!(el.get("POSITION"), Some("100,100,0"));
        assert_eq!(el.get("COLOR"), None);
    }
}
