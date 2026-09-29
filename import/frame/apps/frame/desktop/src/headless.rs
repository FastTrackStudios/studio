//! `--screenshot`: the player loop offscreen, with optional scripted input
//! that goes through the same window → surface mapping as real events.

use std::time::Instant;

use surface_runtime::Modifiers;

use crate::input::View;
use crate::player::{Player, banner};
use crate::{Args, Script, source};

/// Frame period of the simulated clock (60 fps).
const FRAME_MS: f64 = 1000.0 / 60.0;
/// Scripted drag: press on frame 1, move up for `DRAG_STEPS` frames, release.
const DRAG_STEPS: u32 = 12;
/// Surface px moved up per step (0.3 of a full knob sweep in total).
const DRAG_STEP_PX: f64 = 5.0;

pub fn run(args: &Args) -> Result<(), String> {
    let out = args.screenshot.as_ref().ok_or("--screenshot needs a path")?;
    let (surface, error) = match args.source.load() {
        Ok(s) => (s, None),
        Err(e) => (source::placeholder(), Some(e)),
    };
    if let Some(e) = &error {
        eprintln!("{e}");
    }
    let mut player = Player::new(surface_memory::Memory::demo(), surface)?;
    let (w, h) = args.size;
    let view = View::fit(player.content(), f64::from(w), f64::from(h));

    let knob = match args.script {
        Some(Script::DragFirstKnob) => {
            let (id, b) = player.first_knob().ok_or("--script drag-first-knob: the surface has no knob")?;
            let addr = player.param_of(id).ok_or("first knob isn't bound to a param")?;
            let before = player.engine().value(&addr).unwrap_or_default();
            if args.frames < DRAG_STEPS + 3 {
                eprintln!("frame: --frames {} is too few to finish the drag (need {})", args.frames, DRAG_STEPS + 3);
            }
            Some((addr, before, view.to_window(b.x + b.width / 2.0, b.y + b.height / 2.0)))
        }
        None => None,
    };

    let mut offscreen = render::offscreen::Offscreen::new()?;
    let mut scene = vello::Scene::new();
    let mut pixels = None;
    let mut spent = 0.0;
    for i in 0..args.frames {
        let t = f64::from(i) * FRAME_MS;
        let started = Instant::now();
        if let Some((_, _, (wx, wy))) = &knob {
            // Scripted events arrive in window px, like winit's.
            let at = |dy: f64| view.to_surface(*wx, wy - dy * view.scale);
            let step = f64::from(i.saturating_sub(1).min(DRAG_STEPS)) * DRAG_STEP_PX;
            match i {
                1 => player.pointer_down(at(0.0), (*wx, *wy), Modifiers::default(), t),
                i if (2..DRAG_STEPS + 2).contains(&i) => player.pointer_move(at(step), Modifiers::default(), t),
                i if i == DRAG_STEPS + 2 => player.pointer_up(t),
                _ => {}
            }
        }
        player.tick(t);
        scene.reset();
        player.paint(&mut scene, &view, t);
        if let Some((b, page)) = error.as_deref().and_then(|e| banner(&args.source.short_error(e), f64::from(w))) {
            render::paint_scene_into(&mut scene, &b, page, kurbo::Affine::IDENTITY, Some(crate::player::font()));
        }
        pixels = Some(offscreen.render(&scene, w, h, player.background())?);
        spent += started.elapsed().as_secs_f64() * 1000.0;
    }

    let px = pixels.ok_or("--frames must be at least 1")?;
    let distinct = {
        let mut seen: Vec<[u8; 4]> = px.rgba.as_chunks::<4>().0.iter().step_by(97).copied().collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    image::RgbaImage::from_raw(px.width, px.height, px.rgba)
        .ok_or("bad pixel buffer")?
        .save(out)
        .map_err(|e| format!("writing {}: {e}", out.display()))?;

    let avg = spent / f64::from(args.frames.max(1));
    let mut report = format!(
        "{{\"ok\":true,\"out\":\"{}\",\"width\":{w},\"height\":{h},\"frames\":{},\"avg_frame_ms\":{avg:.3},\"distinct_colors\":{distinct}",
        out.display(),
        args.frames,
    );
    if let Some((addr, before, _)) = &knob {
        let after = player.engine().value(addr).unwrap_or_default();
        report += &format!(",\"param\":\"{}\",\"before\":{before:.4},\"after\":{after:.4}", addr.0);
    }
    if let Some(e) = &error {
        report += &format!(",\"error\":{}", serde_json::to_string(e).unwrap_or_default());
    }
    println!("{report}}}");
    Ok(())
}
