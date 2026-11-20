//! Signal: every rig on one screen in the band's order, the loaded one
//! saying so, each with its preset and input; the set you play tonight a
//! badge that opens it in your rig.

use dioxus::prelude::*;

use super::parts::Icon;
use super::{INK, INK3, LIVE, OK, PAGE, hsl, use_studio};

#[component]
pub fn SignalView() -> Element {
    let studio = use_studio();
    let s = studio.read();
    let next = s.mine().first().copied().cloned();
    // Tonight's set you play, and the rig you play it on: the badge, and
    // the loaded rig's line (the set, which song of how many, its title).
    let quick = next.as_ref().and_then(|w| {
        let sr = s.series(&w.series)?;
        let rig = s
            .rigs
            .iter()
            .find(|r| Some(r.name.as_str()) == w.mine.as_deref())?;
        let first = w
            .places
            .first()
            .map(|p| p.song.title.clone())
            .unwrap_or_default();
        Some((
            sr.name.clone(),
            w.day.when(s.today),
            rig.icon,
            rig.hue,
            format!("{} · 1/{} · {first}", sr.name, w.places.len()),
        ))
    });
    let now = quick.as_ref().map(|q| q.4.clone());
    rsx! {
        div { style: "{PAGE}",
            super::parts::TopBar { title: "Signal",
                if let Some((name, when, icon, hue, _)) = quick {
                    div { style: "height: 44px; border-radius: 22px; display: flex; align-items: center; padding: 0 1px; \
                                  background: #1a1a1f; border: 1px solid {hsl(hue, 45, 34)}; box-shadow: 0 6px 20px rgba(0,0,0,0.45);",
                        button { "aria-label": "Open tonight's set in your rig",
                            style: "height: 40px; display: flex; align-items: center; gap: 7px; padding: 0 10px 0 12px; border: none; background: none; \
                                    border-radius: 20px; font-size: 14px; font-weight: 650; white-space: nowrap; color: {hsl(hue, 80, 78)};",
                            Icon { name: icon, size: 16, stroke: 2.0 }
                            "{name} · {when}"
                        }
                        super::parts::Hair {}
                        button { "aria-label": "Your sets",
                            style: "width: 34px; height: 40px; border: none; background: none; display: flex; align-items: center; justify-content: center; color: {INK3};",
                            Icon { name: "updown", size: 16, stroke: 2.0 }
                        }
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; gap: 8px;",
                for r in s.rigs.iter() {
                    div {
                        key: "{r.name}",
                        style: if r.loaded { format!("display: flex; align-items: center; gap: 14px; box-sizing: border-box; min-height: 76px; padding: 10px 16px 10px 12px; border-radius: 18px; \
                                                      background: linear-gradient(100deg, {}, #111114 70%); border: 1px solid {};", hsl(r.hue, 35, 14), hsl(r.hue, 45, 32)) }
                               else { "display: flex; align-items: center; gap: 14px; box-sizing: border-box; min-height: 76px; padding: 10px 16px 10px 12px; border-radius: 18px; background: #111114; border: 1px solid #1c1c21;".to_owned() },
                        div { style: "width: 52px; height: 52px; border-radius: 15px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                                      background: {hsl(r.hue, 40, 16)}; color: {hsl(r.hue, 75, 72)};",
                            Icon { name: r.icon, size: 26, stroke: 1.7 }
                        }
                        div { style: "flex: 1; min-width: 0;",
                            div { style: "font-size: 20px; font-weight: 720; letter-spacing: -0.02em; color: {INK};",
                                "{r.name}"
                                small { style: "font-size: 12px; font-weight: 550; color: {INK3}; margin-left: 8px; letter-spacing: 0;", "{r.input}" }
                            }
                            if r.loaded && now.is_some() {
                                div { style: "font-size: 14px; font-weight: 600; color: {hsl(r.hue, 70, 78)}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "{now.clone().unwrap_or_default()}"
                                }
                                div { style: "height: 3px; border-radius: 2px; background: #1f1f23; margin-top: 7px; overflow: hidden;",
                                    div { style: "width: 56%; height: 100%; border-radius: 2px; background: linear-gradient(90deg, {OK}, {LIVE});" }
                                }
                            } else {
                                div { style: "font-size: 14px; color: {INK3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                    "{r.preset.clone().unwrap_or_else(|| \"Not set up\".into())}"
                                }
                            }
                        }
                        if r.loaded {
                            span { style: "width: 8px; height: 8px; border-radius: 4px; background: {OK};" }
                        } else {
                            span { style: "color: {INK3};", Icon { name: "chev", size: 18 } }
                        }
                    }
                }
            }
        }
    }
}
