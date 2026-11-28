//! The rig's controls as actions.
//!
//! Each is a call of the guitar rig's own service (`guitar::proto::rig::Rig`)
//! on the backend this process runs, so the CLI, MCP and Luau do what a
//! pane's button does. What a person counts (songs, sections, switches,
//! patches) is numbered from 1; a `name` is taken where the rig has one.
//! Answers are the rig's own wire types as JSON.

use architect_extensions::linked::Builder;
use facet::Facet;
use guitar::proto::Applied;
use guitar::proto::audio::AudioSettings;
use guitar::proto::rig::Rig;
use keys::proto::keys::KeysRig as Keys;
use serde_json::{Value as Json, json};

use crate::{Rigs, rigs};

/// A wire value as JSON.
fn wire<'a, T: Facet<'a>>(value: &T) -> Result<Json, String> {
    let text = facet_json::to_string(value).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// JSON as a wire value.
fn unwire<T: Facet<'static>>(value: &Json) -> Result<T, String> {
    facet_json::from_str(&value.to_string()).map_err(|e| e.to_string())
}

/// Runs `call` on the rigs, off the runtime's threads (a rig's call may
/// open a device or read its library).
async fn on<F>(call: F) -> Result<Json, String>
where
    F: FnOnce(&'static Rigs) -> Result<Json, String> + Send + 'static,
{
    let rigs = rigs()?;
    tokio::task::spawn_blocking(move || call(rigs))
        .await
        .map_err(|e| e.to_string())?
}

/// The rig's `Applied` as an answer.
fn applied(applied: &Applied) -> Result<Json, String> {
    if applied.ok {
        Ok(json!({ "ok": true }))
    } else {
        Err(applied.message.clone())
    }
}

fn text(args: &Json, key: &str) -> Option<String> {
    args.get(key).and_then(Json::as_str).map(str::to_owned)
}

fn flag(args: &Json, key: &str) -> Option<bool> {
    args.get(key).and_then(Json::as_bool)
}

fn whole(args: &Json, key: &str) -> Option<i64> {
    args.get(key).and_then(Json::as_i64)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::as_conversions,
    reason = "a control's value, a macro's position: f32 on the wire"
)]
fn real(args: &Json, key: &str) -> Option<f32> {
    args.get(key).and_then(Json::as_f64).map(|v| v as f32)
}

/// `number` (from 1) as the rig's 0-based index.
fn index(args: &Json, key: &str) -> Result<Option<u32>, String> {
    match whole(args, key) {
        None => Ok(None),
        Some(n) if n >= 1 => u32::try_from(n.saturating_sub(1))
            .map(Some)
            .map_err(|e| e.to_string()),
        Some(n) => Err(format!("`{key}` counts from 1, got {n}")),
    }
}

/// `index` (0-based, as [`index`] made it) refused when `items` (a wire
/// list) has no such place: a person's number out of range is an error,
/// never the last one played instead (studio#560).
fn within(index: u32, items: &Json, key: &str) -> Result<u32, String> {
    let count = items.as_array().map_or(0, Vec::len);
    if usize::try_from(index).is_ok_and(|i| i < count) {
        Ok(index)
    } else {
        Err(format!(
            "no `{key}` {}: there are {count}",
            index.saturating_add(1)
        ))
    }
}

/// The 0-based place of the item named `name` in `items` (each a wire
/// struct with a `name`), any case.
fn named(items: &Json, name: &str) -> Option<u32> {
    let at = items.as_array()?.iter().position(|item| {
        item.get("name")
            .and_then(Json::as_str)
            .is_some_and(|n| n.eq_ignore_ascii_case(name.trim()))
    })?;
    u32::try_from(at).ok()
}

/// Where the rig is: what a caller reads after a move.
fn place(rigs: &Rigs) -> Result<Json, String> {
    let perf = wire(&Rig::perf(&rigs.guitar))?;
    let status = Rig::status(&rigs.guitar);
    let pick = |list: &str, at: &str| {
        let i = perf.get(at).and_then(Json::as_u64);
        let name = i
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| perf.get(list)?.as_array()?.get(i)?.get("name").cloned());
        json!({ "number": i.map(|i| i.saturating_add(1)), "name": name })
    };
    Ok(json!({
        "patch": status.active_patch,
        "mode": perf.get("perform_mode"),
        "song": pick("songs", "song_index"),
        "section": pick("parts", "part_index"),
    }))
}

/// The engine and the guitar rig, in one answer.
fn state(rigs: &Rigs) -> Result<Json, String> {
    let status = Rig::status(&rigs.guitar);
    Ok(json!({
        "up": true,
        "url": crate::url(),
        "pid": std::process::id(),
        "instrument": guitar::instrument::current().id(),
        "library": guitar::library::rig_dir().display().to_string(),
        "design": guitar::library::rig_is_design(),
        "silent": guitar::library::rig_is_silent(),
        "ephemeral": guitar::library::rig_is_ephemeral(),
        "running": status.running,
        "audio_error": status.audio_error,
        "at": place(rigs)?,
        "status": wire(&status)?,
    }))
}

/// Adds every action to `ext`.
#[expect(
    clippy::too_many_lines,
    reason = "one registration per action, each a few lines: a table"
)]
pub fn register(ext: &mut Builder) {
    ext.action("STUDIO_RIG_STATE", |_| async {
        match rigs() {
            Ok(_) => on(state).await,
            Err(why) => Ok(json!({ "up": false, "why": why })),
        }
    })
    .action("STUDIO_RIG_START", |_| {
        on(|r| {
            Rig::start(&r.guitar);
            state(r)
        })
    })
    .action("STUDIO_RIG_STOP", |_| {
        on(|r| {
            Rig::stop(&r.guitar);
            state(r)
        })
    })
    .action("STUDIO_RIG_RESTART", |_| {
        on(|r| {
            Rig::restart(&r.guitar);
            state(r)
        })
    })
    .action("STUDIO_RIG_PANIC", |_| {
        on(|r| {
            Rig::panic(&r.guitar);
            Ok(json!({ "ok": true }))
        })
    })
    .action("STUDIO_RIG_MUTE", |args| {
        on(move |r| {
            let now = Rig::perf(&r.guitar).headphone;
            let house = flag(&args, "house").unwrap_or(now.main_mute);
            let phones = flag(&args, "phones").unwrap_or(now.phones_mute);
            // With neither named: the house mute, flipped (the bar's button).
            let house = if args.get("house").is_none() && args.get("phones").is_none() {
                !now.main_mute
            } else {
                house
            };
            Rig::set_mutes(&r.guitar, house, phones);
            Ok(json!({ "house": house, "phones": phones }))
        })
    })
    .action("STUDIO_RIG_PERFORMANCE", |_| {
        on(|r| wire(&Rig::perf(&r.guitar)))
    })
    .action("STUDIO_RIG_MODE", |args| {
        on(move |r| {
            let mode = match args.get("mode") {
                Some(Json::String(name)) => match name.to_lowercase().as_str() {
                    "preset" | "presets" => 0,
                    "profile" => 1,
                    "setlist" | "set" => 2,
                    other => return Err(format!("no mode {other}: preset, profile or setlist")),
                },
                Some(Json::Number(n)) => n
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n <= 2)
                    .ok_or("mode is 0 (preset), 1 (profile) or 2 (setlist)")?,
                _ => return Err("no mode".to_owned()),
            };
            Rig::set_perform_mode(&r.guitar, mode);
            place(r)
        })
    })
    .action("STUDIO_RIG_SWITCH", |args| {
        on(move |r| {
            let switch = index(&args, "switch")?.ok_or("no switch (from 1)")?;
            if flag(&args, "hold").unwrap_or(false) {
                Rig::hold_switch(&r.guitar, switch);
            } else {
                Rig::tap_switch(&r.guitar, switch);
            }
            place(r)
        })
    })
    .action("STUDIO_RIG_STACK", |args| {
        on(move |r| {
            let perf = wire(&Rig::perf(&r.guitar))?;
            let stack = match text(&args, "name") {
                Some(name) => named(perf.get("stacks").unwrap_or(&Json::Null), &name)
                    .ok_or_else(|| format!("no stack named {name}"))?,
                None => within(
                    index(&args, "stack")?.ok_or("no stack (from 1) or name")?,
                    perf.get("stacks").unwrap_or(&Json::Null),
                    "stack",
                )?,
            };
            match index(&args, "patch")? {
                // One of the stack's patches, by its place in it.
                Some(position) => applied(&Rig::play_stack_patch(&r.guitar, stack, position))?,
                // The switch's press: the stack's current patch, or its next.
                None => {
                    Rig::press_stack(&r.guitar, stack);
                    Rig::release_stack(&r.guitar, stack);
                    json!({ "ok": true })
                }
            };
            place(r)
        })
    })
    .action("STUDIO_RIG_PATCHES", |_| {
        on(|r| wire(&Rig::patches(&r.guitar)))
    })
    .action("STUDIO_RIG_PATCH", |args| {
        on(move |r| {
            let patches = wire(&Rig::patches(&r.guitar))?;
            let patch = match text(&args, "name") {
                Some(name) => {
                    named(&patches, &name).ok_or_else(|| format!("no patch named {name}"))?
                }
                None => within(
                    index(&args, "patch")?.ok_or("no patch (from 1) or name")?,
                    &patches,
                    "patch",
                )?,
            };
            applied(&Rig::select_patch(&r.guitar, patch))?;
            place(r)
        })
    })
    .action("STUDIO_RIG_PRESETS", |_| {
        on(|r| wire(&Rig::presets(&r.guitar)))
    })
    .action("STUDIO_RIG_PRESET", |args| {
        on(move |r| {
            if let Some(preset) = text(&args, "preset") {
                // A rig preset, by name, on one of its variations.
                Rig::choose_preset(
                    &r.guitar,
                    preset,
                    text(&args, "variation").unwrap_or_default(),
                );
            } else {
                let presets = wire(&Rig::presets(&r.guitar))?;
                let preset = match text(&args, "name") {
                    Some(name) => {
                        named(&presets, &name).ok_or_else(|| format!("no preset named {name}"))?
                    }
                    None => within(
                        index(&args, "number")?.ok_or("no preset: number, name or preset")?,
                        &presets,
                        "number",
                    )?,
                };
                Rig::play_preset(&r.guitar, preset);
            }
            place(r)
        })
    })
    .action("STUDIO_RIG_LIBRARY", |_| {
        on(|r| wire(&Rig::library(&r.guitar)))
    })
    .action("STUDIO_RIG_SONG", |args| {
        on(move |r| {
            match whole(&args, "step") {
                Some(step) if step < 0 => Rig::prev_song(&r.guitar),
                Some(_) => Rig::next_song(&r.guitar),
                None => {
                    let perf = wire(&Rig::perf(&r.guitar))?;
                    let song = match text(&args, "name") {
                        Some(name) => named(perf.get("songs").unwrap_or(&Json::Null), &name)
                            .ok_or_else(|| format!("no song named {name} in the set"))?,
                        None => within(
                            index(&args, "song")?.ok_or("no song (from 1), name or step")?,
                            perf.get("songs").unwrap_or(&Json::Null),
                            "song",
                        )?,
                    };
                    applied(&Rig::select_song(&r.guitar, song))?;
                }
            }
            place(r)
        })
    })
    .action("STUDIO_RIG_SECTION", |args| {
        on(move |r| {
            match whole(&args, "step") {
                Some(step) => {
                    let dir = if step < 0 { -1 } else { 1 };
                    applied(&Rig::step_part(&r.guitar, dir, false))?;
                }
                None => {
                    let perf = wire(&Rig::perf(&r.guitar))?;
                    let part = match text(&args, "name") {
                        Some(name) => named(perf.get("parts").unwrap_or(&Json::Null), &name)
                            .ok_or_else(|| format!("no section named {name} in the song"))?,
                        None => within(
                            index(&args, "section")?.ok_or("no section (from 1), name or step")?,
                            perf.get("parts").unwrap_or(&Json::Null),
                            "section",
                        )?,
                    };
                    applied(&Rig::select_part(&r.guitar, part))?;
                }
            }
            place(r)
        })
    })
    .action("STUDIO_RIG_SETLIST", |args| {
        on(move |r| {
            let setlist = index(&args, "setlist")?.ok_or("no setlist (from 1)")?;
            applied(&Rig::select_setlist(&r.guitar, setlist))?;
            place(r)
        })
    })
    .action("STUDIO_RIG_PROFILE", |args| {
        on(move |r| {
            let name = text(&args, "name").ok_or("no name")?;
            applied(&Rig::select_profile(&r.guitar, name))?;
            place(r)
        })
    })
    .action("STUDIO_RIG_MACROS", |_| {
        on(|r| wire(&Rig::macros(&r.guitar)))
    })
    .action("STUDIO_RIG_MACRO", |args| {
        on(move |r| {
            let id = text(&args, "id").ok_or("no id (STUDIO_RIG_MACROS lists them)")?;
            let result = if flag(&args, "reset").unwrap_or(false) {
                Rig::reset_macro(&r.guitar, id)
            } else if let Some(on) = flag(&args, "pad") {
                Rig::set_macro_pad(&r.guitar, id, on)
            } else {
                let value = real(&args, "value").ok_or("no value (0 to 1), pad or reset")?;
                Rig::set_macro(&r.guitar, id, value)
            };
            wire(&result)
        })
    })
    .action("STUDIO_RIG_BLOCKS", |_| {
        on(|r| wire(&Rig::chain(&r.guitar)))
    })
    .action("STUDIO_RIG_NODES", |_| on(|r| wire(&Rig::nodes(&r.guitar))))
    .action("STUDIO_RIG_BLOCK", |args| {
        on(move |r| {
            let id = text(&args, "id").ok_or("no id (STUDIO_RIG_BLOCKS lists them)")?;
            if let Some(bypassed) = flag(&args, "bypass") {
                Rig::set_block_bypass(&r.guitar, id.clone(), bypassed);
            }
            if let Some(param) = text(&args, "param") {
                let value = real(&args, "value").ok_or("a param needs a value")?;
                applied(&Rig::set_block_param(&r.guitar, id.clone(), param, value))?;
            }
            if let Some(option) = whole(&args, "option") {
                let option = u32::try_from(option).map_err(|e| e.to_string())?;
                applied(&Rig::set_block_option(&r.guitar, id.clone(), option))?;
            }
            let chain = wire(&Rig::chain(&r.guitar))?;
            let block = chain
                .as_array()
                .and_then(|blocks| {
                    blocks
                        .iter()
                        .find(|b| b.get("id").and_then(Json::as_str) == Some(id.as_str()))
                })
                .cloned();
            block.ok_or_else(|| format!("no block {id} in the patch playing"))
        })
    })
    .action("STUDIO_RIG_BLOCK_PRESET", |args| {
        on(move |r| {
            let preset = text(&args, "preset").ok_or("no preset")?;
            let took = match (text(&args, "module"), text(&args, "block")) {
                (Some(module), _) => Rig::choose_module(
                    &r.guitar,
                    module,
                    preset,
                    text(&args, "variation").unwrap_or_default(),
                ),
                (None, Some(block)) => Rig::choose_block(&r.guitar, block, preset),
                (None, None) => return Err("no block or module".to_owned()),
            };
            applied(&took)
        })
    })
    .action("STUDIO_RIG_TUNER", |args| {
        on(move |r| {
            if flag(&args, "toggle").unwrap_or(false) {
                Rig::toggle_tuner(&r.guitar);
            }
            wire(&Rig::tuner(&r.guitar))
        })
    })
    .action("STUDIO_RIG_TAP", |_| {
        on(|r| {
            Rig::tap_tempo(&r.guitar);
            Ok(json!({ "ok": true }))
        })
    })
    .action("STUDIO_RIG_INSTRUMENT", |args| {
        on(move |r| {
            if let Some(asked) = text(&args, "instrument") {
                let known = ["guitar", "bass"];
                if !known.iter().any(|k| k.eq_ignore_ascii_case(asked.trim())) {
                    return Err(format!("no instrument {asked}: guitar or bass"));
                }
            }
            // The one playing already: nothing to do (the Rig window asks for
            // the instrument it opens on, whichever plays).
            let current = guitar::instrument::current().id();
            // The rig answers what it did in a line (`now the bass`), never
            // a refusal: the name was checked above.
            let asked = text(&args, "instrument").map(|a| a.trim().to_ascii_lowercase());
            let said = asked
                .as_ref()
                .filter(|asked| !asked.eq_ignore_ascii_case(current))
                .map(|instrument| Rig::set_instrument(&r.guitar, instrument.clone()));
            let now = guitar::instrument::current().id();
            if let Some(asked) = asked.filter(|a| !a.eq_ignore_ascii_case(now)) {
                return Err(format!(
                    "the rig is still the {now}, not the {asked}: {}",
                    said.unwrap_or_default()
                ));
            }
            Ok(json!({
                "instrument": now,
                "library": guitar::library::rig_dir().display().to_string(),
            }))
        })
    })
    .action("STUDIO_RIG_SETUP", |args| {
        on(move |r| {
            if let Some(setup) = args.get("setup") {
                Rig::save_setup(&r.guitar, unwire(setup)?);
            }
            wire(&Rig::setup(&r.guitar))
        })
    })
    .action("STUDIO_RIG_AUDIO", |args| {
        on(move |r| {
            if let Some(prefs) = args.get("prefs") {
                AudioSettings::save_prefs(&r.guitar, unwire(prefs)?);
                // Saved prefs are read when the device opens: reopen it
                // when it is open, unless told to wait.
                if flag(&args, "restart").unwrap_or(true) && Rig::status(&r.guitar).running {
                    Rig::restart(&r.guitar);
                }
            }
            Ok(json!({
                "prefs": wire(&AudioSettings::prefs(&r.guitar))?,
                "devices": wire(&AudioSettings::devices(&r.guitar))?,
                "running": Rig::status(&r.guitar).running,
            }))
        })
    })
    .action("STUDIO_RIG_MIDI", |args| {
        on(move |r| {
            if let Some(target) = text(&args, "learn") {
                Rig::midi_learn(&r.guitar, target);
            }
            if let Some(target) = text(&args, "unlearn") {
                Rig::midi_unlearn(&r.guitar, target);
            }
            if flag(&args, "cancel").unwrap_or(false) {
                Rig::midi_learn_cancel(&r.guitar);
            }
            let perf = wire(&Rig::perf(&r.guitar))?;
            Ok(json!({
                "ports": Rig::midi_ports(&r.guitar),
                "recent": Rig::midi_recent(&r.guitar),
                "learn": perf.get("learn"),
            }))
        })
    })
    .action("STUDIO_RIG_RENDER", |args| {
        on(move |_| {
            // Offline: every patch of the profile built as the live rig
            // builds it and played a recorded DI, on no device. Nothing is
            // written (a dry run of the rig's own levelling).
            let profile = text(&args, "profile");
            let began = std::time::Instant::now();
            let measured = guitar::levelling::level_profile(profile.as_deref(), 48_000, true)?;
            let patches: Vec<Json> = measured
                .iter()
                .map(|m| json!({ "patch": m.patch, "lufs": m.lufs, "rendered": m.lufs.is_some() }))
                .collect();
            Ok(json!({
                "profile": profile,
                "sample_rate": 48_000,
                "rendered": measured.iter().filter(|m| m.lufs.is_some()).count(),
                "of": measured.len(),
                "seconds": began.elapsed().as_secs_f64(),
                "patches": patches,
            }))
        })
    })
    .action("STUDIO_RIG_KEYS_STATE", |_| {
        on(|r| {
            Ok(json!({
                "status": wire(&Keys::status(&r.keys))?,
                "perform": wire(&Keys::perform(&r.keys))?,
            }))
        })
    })
    .action("STUDIO_RIG_KEYS_START", |_| {
        on(|r| {
            Keys::start(&r.keys);
            wire(&Keys::status(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_STOP", |_| {
        on(|r| {
            Keys::stop(&r.keys);
            wire(&Keys::status(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_PRESETS", |_| {
        on(|r| wire(&Keys::presets(&r.keys)))
    })
    .action("STUDIO_RIG_KEYS_PRESET", |args| {
        on(move |r| {
            let presets = wire(&Keys::presets(&r.keys))?;
            let preset = match text(&args, "name") {
                Some(name) => {
                    named(&presets, &name).ok_or_else(|| format!("no keys preset named {name}"))?
                }
                None => within(
                    index(&args, "number")?.ok_or("no preset: number (from 1) or name")?,
                    &presets,
                    "number",
                )?,
            };
            Keys::load_preset(&r.keys, preset);
            wire(&Keys::status(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_STACK", |args| {
        on(move |r| {
            let perform = wire(&Keys::perform(&r.keys))?;
            let stacks = perform.get("stacks").unwrap_or(&Json::Null);
            let stack = match text(&args, "name") {
                Some(name) => {
                    named(stacks, &name).ok_or_else(|| format!("no keys stack named {name}"))?
                }
                None => within(
                    index(&args, "stack")?.ok_or("no stack (from 1) or name")?,
                    stacks,
                    "stack",
                )?,
            };
            Keys::press_stack(&r.keys, stack);
            wire(&Keys::perform(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_MIXER", |args| {
        on(move |r| {
            let db = real(&args, "db");
            let muted = flag(&args, "mute");
            match (text(&args, "layer"), text(&args, "engine")) {
                (Some(layer), _) => {
                    if let Some(db) = db {
                        Keys::set_layer_gain(&r.keys, layer.clone(), db);
                    }
                    if let Some(muted) = muted {
                        Keys::set_layer_mute(&r.keys, layer.clone(), muted);
                    }
                    if let Some(solo) = flag(&args, "solo") {
                        Keys::set_layer_solo(&r.keys, layer, solo);
                    }
                }
                (None, Some(engine)) => {
                    if let Some(db) = db {
                        Keys::set_engine_gain(&r.keys, engine.clone(), db);
                    }
                    if let Some(muted) = muted {
                        Keys::set_engine_mute(&r.keys, engine, muted);
                    }
                }
                (None, None) => {
                    if let Some(db) = db {
                        Keys::set_master_gain(&r.keys, db);
                    }
                }
            }
            wire(&Keys::mixer(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_MACROS", |_| {
        on(|r| wire(&Keys::rig_macros(&r.keys)))
    })
    .action("STUDIO_RIG_KEYS_MACRO", |args| {
        on(move |r| {
            let id = text(&args, "id").ok_or("no id (STUDIO_RIG_KEYS_MACROS lists them)")?;
            let value = real(&args, "value").ok_or("no value")?;
            match text(&args, "engine") {
                Some(engine) => Keys::set_engine_global(&r.keys, engine, id, value),
                None => Keys::set_rig_global(&r.keys, id, value),
            }
            wire(&Keys::rig_macros(&r.keys))
        })
    })
    .action("STUDIO_RIG_KEYS_NOTE", |args| {
        on(move |r| {
            let note = whole(&args, "note")
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n <= 127)
                .ok_or("no note (0 to 127)")?;
            let velocity = whole(&args, "velocity")
                .and_then(|v| u32::try_from(v).ok())
                .unwrap_or(100)
                .min(127);
            Keys::trigger(&r.keys, note, velocity);
            Ok(json!({ "note": note, "velocity": velocity }))
        })
    })
    .action("STUDIO_RIG_KEYS_MIDI", |args| {
        on(move |r| {
            if let Some(port) = text(&args, "port") {
                Keys::set_midi_port(&r.keys, port);
            }
            if let Some(target) = text(&args, "learn") {
                Keys::midi_learn(&r.keys, target);
            }
            if let Some(target) = text(&args, "unlearn") {
                Keys::midi_unlearn(&r.keys, target);
            }
            if flag(&args, "cancel").unwrap_or(false) {
                Keys::midi_learn_cancel(&r.keys);
            }
            Ok(json!({
                "ports": Keys::midi_ports(&r.keys),
                "port": Keys::status(&r.keys).midi_port,
            }))
        })
    })
    .action("STUDIO_RIG_RUN", |args| async move {
        let id = text(&args, "id").ok_or("no id (an FTS_SIGNAL_* command)")?;
        let rigs = rigs()?;
        let list =
            guitar::signal_commands::command_list(&rigs.guitar).map_err(|e| e.to_string())?;
        let none = Json::Null;
        let with = args.get("args").unwrap_or(&none).to_string();
        let out = list
            .run_json_async(&id, with.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::from_slice(&out)
            .unwrap_or_else(|_| Json::String(String::from_utf8_lossy(&out).into_owned())))
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{index, json, within};

    #[test]
    fn a_number_counts_from_one_and_stays_inside_its_list() {
        let three = json!(["a", "b", "c"]);
        let at = |n: i64| index(&json!({ "song": n }), "song").map(|i| i.unwrap());
        assert_eq!(within(at(3).unwrap(), &three, "song"), Ok(2));
        assert_eq!(
            within(at(4).unwrap(), &three, "song"),
            Err("no `song` 4: there are 3".to_owned())
        );
        assert!(at(0).unwrap_err().contains("counts from 1"));
        assert!(within(0, &json!([]), "patch").is_err());
        assert_eq!(index(&json!({}), "song"), Ok(None));
    }
}
