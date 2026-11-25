//! The venue's room as EasyView scenery: the architecture, the band's
//! stands and gear, the people, read from the same files Ignition's own
//! room is built from, so both visualizers show one room.
//!
//! - `room.json`: the stands (`Column - …`), boxes by their centre.
//! - `environment.json` (with `environment.edits.json` over it): the
//!   room's measurements and materials, and every object Ignition draws,
//!   by the middle of its footprint on the floor. Objects baked into the
//!   room's model (walls, ceiling, the lights in it) sit at the origin and
//!   are drawn here from the measurements instead.
//! - `props.json`: the band, its kit and the singer's keyboard and mic.
//!
//! Ignition's axes are metres, x stage right (-) to left (+), y downstage
//! (-) to upstage (+), z up. EasyView's are centimetres, Y up, +Z to the
//! band wall, the room centred on the origin: [`ev`].

use std::path::Path;

use ignition_daslight::Error;
use ignition_daslight::easyview::{Material, Model, block, panel};
use ignition_daslight::xml::Element;
use serde_json::Value;

/// Room 138's centre on Ignition's depth axis: the band wall is at
/// y = +1.8288 and the back of the house at y = -8.2912
/// (room138-cbu/README.md, "The room").
pub const ROOM_CENTRE_Y: f64 = -3.2312;

/// A point in Ignition's venue (metres) in EasyView's room (centimetres).
pub fn ev(p: [f64; 3]) -> [f64; 3] {
    [p[0] * 100.0, p[2] * 100.0, (p[1] - ROOM_CENTRE_Y) * 100.0]
}

/// A size in Ignition's axes (width, depth, height) as EasyView's (width,
/// height, depth), in centimetres.
fn ev_size(s: [f64; 3]) -> [f64; 3] {
    [s[0] * 100.0, s[2] * 100.0, s[1] * 100.0]
}

/// Ignition turns an object about its up axis anticlockwise seen from
/// above; EasyView's Y turn with the depth axis flipped is the other way.
fn ev_yaw(degrees: f64) -> f64 {
    -degrees
}

// Where the measured room's parts are (tools/build_lab138_room.py, the
// script environment.json is written by): the soffits' inner edges, the
// curtain's gap, the corridor off the house-back stage-right corner.
const X_SOFFIT_SR: f64 = -5.15;
const X_SOFFIT_SL: f64 = 5.05;
const CURTAIN_GAP: (f64, f64) = (-0.30, 0.05);
const X_CORRIDOR_END: f64 = -9.85;
const Y_CORRIDOR_IN: f64 = -6.27;

/// Library meshes, with the bounding boxes `assimp info` gives for them
/// (EasyView loads them through assimp too).
const DRUMS: Model<'static> = Model {
    file: "./Library/Music instrument/Drums.x",
    bounds: [[-73.196, 0.0, -71.637], [91.709, 108.477, 40.0]],
};
const PERSON: Model<'static> = Model {
    file: "./Library/People/Man01.x",
    bounds: [[-25.093, -0.044, -14.307], [25.022, 175.922, 14.291]],
};
const KEYBOARD: Model<'static> = Model {
    file: "./Library/Music instrument/Synthesizer.x",
    bounds: [[-50.274, 0.178, -15.0], [49.803, 108.605, 20.529]],
};
const MIC: Model<'static> = Model {
    file: "./Library/Sound systems/Microphone stand.x",
    bounds: [[-28.18, -1.5, -20.192], [-0.708, 84.421, 10.838]],
};
const PIANO: Model<'static> = Model {
    file: "./Library/Music instrument/Grand piano.x",
    bounds: [[-69.612, 0.0, -92.04], [69.612, 121.503, 91.669]],
};

fn read(path: &Path) -> Result<Value, Error> {
    let text =
        std::fs::read_to_string(path).map_err(|e| Error::Io(path.display().to_string(), e))?;
    serde_json::from_str(&text).map_err(|e| Error::Xml(format!("{}: {e}", path.display())))
}

fn num(v: &Value, pointer: &str) -> f64 {
    v.pointer(pointer).and_then(Value::as_f64).unwrap_or(0.0)
}

fn xyz(v: &Value, key: &str) -> [f64; 3] {
    ["x", "y", "z"].map(|k| num(v, &format!("/{key}/{k}")))
}

/// The room's materials by name, as EasyView colours.
struct Materials(Vec<(String, Material)>);

impl Materials {
    fn read(env: &Value) -> Self {
        let list = env
            .get("materials")
            .and_then(Value::as_array)
            .map(|m| {
                m.iter()
                    .filter_map(|m| {
                        let name = m.get("name")?.as_str()?.to_owned();
                        let c = m.get("base_color_linear")?.as_array()?;
                        let rgb = [0, 1, 2].map(|i| c.get(i).and_then(Value::as_f64).unwrap_or(0.5));
                        let rough = m.get("roughness").and_then(Value::as_f64).unwrap_or(0.5);
                        // Rough is matte; EasyView's specular runs 0..100.
                        let gloss = ((1.0 - rough.clamp(0.0, 1.0)) * 100.0).round();
                        let specular = format!("{gloss}").parse().unwrap_or(0);
                        Some((name, Material::linear(rgb, specular)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self(list)
    }

    fn get(&self, name: &str) -> Material {
        self.0
            .iter()
            .find(|(n, _)| n == name)
            .map_or_else(|| Material::rgb(128, 128, 128, 20), |(_, m)| *m)
    }
}

/// The material an object of the room is drawn in, by its name.
fn material_for(name: &str) -> &'static str {
    const BY_NAME: [(&str, &str); 24] = [
        ("Door", "Door Wood"),
        ("Exit Sign", "Glow Exit"),
        ("Grand Piano", "Piano Black"),
        ("Piano Bench", "Piano Black"),
        ("Keyboard - Red", "Keyboard Red"),
        ("Drum Mat", "Rug Dark"),
        ("Rug - Tables", "Rug Dark"),
        ("Rug", "Woven Tan"),
        ("Diffuser", "Diffuser Frame"),
        ("Mixing Console", "Walnut"),
        ("Coffee Table", "Walnut"),
        ("Table", "Table White"),
        ("Couch", "Leather Tan"),
        ("Beanbag", "Woven Tan"),
        ("Armchair", "Woven Tan"),
        ("Chair", "Chair Fabric"),
        ("Stool", "Metal Grey"),
        ("Plant", "Plant Green"),
        ("Whiteboard", "Whiteboard"),
        ("Projector", "Fixture White"),
        ("TV", "Screen"),
        ("Console Screen", "Screen"),
        ("Bongos", "Drum Skin"),
        ("Column", "Frame Black"),
    ];
    BY_NAME
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map_or("Frame Black", |(_, m)| m)
}

/// The room's shell: floor, walls, the ceiling, the corridor, the curtain
/// and its track.
fn shell(env: &Value, mats: &Materials) -> Vec<Element> {
    let width = num(env, "/measured/width_m");
    let depth = num(env, "/measured/depth_m");
    let soffit = num(env, "/measured/soffit_m");
    let curtain_top = num(env, "/measured/curtain_top_m");
    let (half_w, half_d) = (width * 50.0, depth * 50.0);
    let h = soffit * 100.0;
    let wall = mats.get("Wall Paint");
    let floor = mats.get("Floor Wood");
    // The floor's colour comes from a procedural plank texture over white;
    // the photo puts it at a fifth of the wall's brightness.
    let floor = Material { color: Material::linear([0.30, 0.17, 0.08], 0).color, ..floor };
    let mut out = vec![
        panel("Floor", [0.0, 0.0, 0.0], [0.0; 3], [width * 100.0, depth * 100.0], floor),
        panel("Wall - Band", [0.0, h / 2.0, half_d], [-90.0, 0.0, 0.0], [width * 100.0, h], wall),
        panel(
            "Wall - House Back",
            [0.0, h / 2.0, -half_d],
            [-90.0, 180.0, 0.0],
            [width * 100.0, h],
            wall,
        ),
        panel(
            "Wall - Stage Left",
            [half_w, h / 2.0, 0.0],
            [0.0, 90.0, 90.0],
            [depth * 100.0, h],
            wall,
        ),
    ];
    // The stage-right wall stops where the corridor opens off it.
    let corridor_in = ev([0.0, Y_CORRIDOR_IN, 0.0])[2];
    let sr_len = half_d - corridor_in;
    out.push(panel(
        "Wall - Stage Right",
        [-half_w, h / 2.0, corridor_in + sr_len / 2.0],
        [0.0, -90.0, 90.0],
        [sr_len, h],
        wall,
    ));
    // The corridor: 2.85 m long, to the exit door at its end.
    let end = X_CORRIDOR_END * 100.0;
    let (c_len, c_wid) = (-half_w - end, corridor_in + half_d);
    let c_mid = [end + c_len / 2.0, corridor_in - c_wid / 2.0];
    out.push(panel("Corridor - Floor", [c_mid[0], 0.0, c_mid[1]], [0.0; 3], [c_len, c_wid], floor));
    out.push(panel(
        "Corridor - Ceiling",
        [c_mid[0], h, c_mid[1]],
        [180.0, 0.0, 0.0],
        [c_len, c_wid],
        mats.get("Soffit Paint"),
    ));
    out.push(panel(
        "Corridor - House Side",
        [c_mid[0], h / 2.0, -half_d],
        [-90.0, 180.0, 0.0],
        [c_len, h],
        wall,
    ));
    out.push(panel(
        "Corridor - Room Side",
        [c_mid[0], h / 2.0, corridor_in],
        [-90.0, 0.0, 0.0],
        [c_len, h],
        wall,
    ));
    out.push(panel(
        "Corridor - End",
        [end, h / 2.0, c_mid[1]],
        [0.0, -90.0, 90.0],
        [c_wid, h],
        wall,
    ));

    out.extend(ceiling(env, mats));

    // The curtain, two drapes 12 cm off the band wall, and its track.
    let velvet = mats.get("Curtain Velvet");
    let curtain_y = ev([0.0, 1.8288 - 0.12, 0.0])[2];
    let ct = curtain_top * 100.0;
    for (name, x0, x1) in [
        ("Curtain - Stage Right", -width / 2.0 + 0.05, CURTAIN_GAP.0),
        ("Curtain - Stage Left", CURTAIN_GAP.1, width / 2.0 - 0.15),
    ] {
        out.push(block(
            name,
            [(x0 + x1) * 50.0, ct / 2.0 + 1.0, curtain_y],
            [(x1 - x0) * 100.0, ct - 2.0, 9.0],
            0.0,
            velvet,
        ));
    }
    out.push(block(
        "Curtain Track",
        [0.0, ct + 3.0, curtain_y],
        [width * 100.0 - 10.0, 6.0, 5.0],
        0.0,
        mats.get("Fixture White"),
    ));
    out
}

/// The ceiling: flat soffits at both ends, the tile pyramid between.
fn ceiling(env: &Value, mats: &Materials) -> Vec<Element> {
    let width = num(env, "/measured/width_m");
    let depth = num(env, "/measured/depth_m");
    let (h, top) = (num(env, "/measured/soffit_m") * 100.0, num(env, "/measured/apex_m") * 100.0);
    let (half_w, half_d) = (width * 50.0, depth * 50.0);
    let mut out = Vec::new();
    let soffit_mat = mats.get("Soffit Paint");
    let (sr, sl) = (X_SOFFIT_SR * 100.0, X_SOFFIT_SL * 100.0);
    for (name, x0, x1) in [("Soffit - Stage Right", -half_w, sr), ("Soffit - Stage Left", sl, half_w)] {
        out.push(panel(
            name,
            [f64::midpoint(x0, x1), h, 0.0],
            [180.0, 0.0, 0.0],
            [x1 - x0, depth * 100.0],
            soffit_mat,
        ));
    }
    // The tile pyramid between them, a face per side, each from its edge
    // at the soffits up to the apex over the room's middle. A face is
    // drawn as the rectangle round its triangle: outside the triangle it
    // runs above the neighbouring face, so from below only the pyramid
    // shows.
    let tile = mats.get("Ceiling Tile");
    let rise = top - h;
    let apex_x = f64::midpoint(sr, sl);
    let mid_h = f64::midpoint(h, top);
    let run_z = half_d;
    let tilt_z = rise.atan2(run_z).to_degrees();
    let slope_z = rise.hypot(run_z);
    for (name, sign) in [("Ceiling - House Side", -1.0), ("Ceiling - Band Side", 1.0)] {
        out.push(panel(
            name,
            [apex_x, mid_h, sign * run_z / 2.0],
            [sign * tilt_z, 0.0, 0.0],
            [sl - sr, slope_z],
            tile,
        ));
    }
    for (name, edge) in [("Ceiling - Stage Right Side", sr), ("Ceiling - Stage Left Side", sl)] {
        let run = (apex_x - edge).abs();
        let tilt = rise.atan2(run).to_degrees();
        let sign = if edge < apex_x { 1.0 } else { -1.0 };
        out.push(panel(
            name,
            [f64::midpoint(edge, apex_x), mid_h, 0.0],
            [0.0, 0.0, sign * tilt],
            [rise.hypot(run), depth * 100.0],
            tile,
        ));
    }
    out
}

/// Objects Ignition draws that are not baked into the room's model, with
/// the venue's edits applied: (name, footprint middle, size, yaw).
fn objects(env: &Value, edits: &Value) -> Vec<(String, [f64; 3], [f64; 3], f64)> {
    let Some(list) = env.get("objects").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|o| {
            let name = o.get("name")?.as_str()?.to_owned();
            let edit = edits.pointer(&format!("/objects/{}", name.replace('/', "~1")));
            let visible = edit
                .and_then(|e| e.get("visible"))
                .or_else(|| o.get("visible"))
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let position = edit
                .filter(|e| e.get("position").is_some())
                .map_or_else(|| xyz(o, "position"), |e| xyz(e, "position"));
            let size = xyz(o, "size");
            let yaw = num(o, "/eulers/z");
            let baked = position.iter().all(|v| v.abs() <= f64::EPSILON);
            (visible && !baked).then_some((name, position, size, yaw))
        })
        .collect()
}

/// Everything in the room but the fixtures, for [`easyview::project_in`].
///
/// # Errors
/// A venue file is missing or not JSON.
pub fn scenery(venue: &Path) -> Result<Vec<Element>, Error> {
    let env = read(&venue.join("environment.json"))?;
    let edits = read(&venue.join("environment.edits.json")).unwrap_or(Value::Null);
    let mats = Materials::read(&env);
    let mut out = shell(&env, &mats);

    // The stands: tower columns, the trees' poles and T-bars.
    for c in read(&venue.join("room.json"))?.as_array().into_iter().flatten() {
        let name = c.get("name").and_then(Value::as_str).unwrap_or_default();
        if !name.starts_with("Column") {
            continue;
        }
        out.push(block(
            name,
            ev(xyz(c, "position")),
            ev_size(xyz(c, "size")),
            ev_yaw(num(c, "/eulers/z")),
            mats.get("Frame Black"),
        ));
    }

    for (name, pos, size, yaw) in objects(&env, &edits) {
        // Thin things (rugs, the drum mat) are a centimetre thick.
        let size = [size[0], size[1], size[2].max(0.01)];
        let (at, s, turn) = (ev(pos), ev_size(size), ev_yaw(yaw));
        // Doors stand in their wall's plane; set them just proud of it,
        // into the room (the corridor's end door: into the corridor).
        let at = match name.as_str() {
            n if n.starts_with("Door") && pos[0] <= X_CORRIDOR_END + 0.01 => [at[0] + 5.0, at[1], at[2]],
            n if n.starts_with("Door") => [at[0], at[1], at[2] + 5.0],
            _ => at,
        };
        if name == "Grand Piano" {
            out.push(PIANO.place(&name, at, s, turn, None));
            continue;
        }
        out.push(block(
            &name,
            [at[0], at[1] + s[1] / 2.0, at[2]],
            s,
            turn,
            mats.get(material_for(&name)),
        ));
    }

    // The band and its instruments.
    for p in read(&venue.join("props.json"))?.as_array().into_iter().flatten() {
        let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
        let (at, s, turn) = (
            ev(xyz(p, "position")),
            ev_size(xyz(p, "size")),
            ev_yaw(num(p, "/eulers/z")),
        );
        let model = if name.starts_with("Person") {
            PERSON
        } else if name.starts_with("Drum") {
            DRUMS
        } else if name.starts_with("Keyboard") {
            KEYBOARD
        } else if name.starts_with("Mic") {
            MIC
        } else {
            out.push(block(name, [at[0], at[1] + s[1] / 2.0, at[2]], s, turn, mats.get("Frame Black")));
            continue;
        };
        out.push(model.place(name, at, s, turn, None));
    }
    Ok(out)
}
