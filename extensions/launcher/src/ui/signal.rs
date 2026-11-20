//! Signal: every rig on one screen in the band's order, the loaded one
//! saying so, each with its preset and input; the set you play tonight a
//! badge that opens it in your rig.

use dioxus::prelude::*;

use super::parts::{Cover, Icon};
use super::{INK, INK3, OK, PAGE, hsl, use_studio};

#[component]
pub fn SignalView() -> Element {
    let studio = use_studio();
    let s = studio.read();
    let next = s.mine().first().copied().cloned();
    let quick = next.as_ref().and_then(|w| {
        let sr = s.series(&w.series)?;
        let rig = s
            .rigs
            .iter()
            .find(|r| Some(r.name.as_str()) == w.mine.as_deref())?;
        Some((
            sr.name.clone(),
            w.day.when(s.today),
            rig.icon,
            rig.hue,
            sr.logo.clone(),
            sr.hue,
        ))
    });
    rsx! {
        div { style: "{PAGE}",
            super::parts::Title { text: "Signal" }
            if let Some((name, when, icon, hue, logo, shue)) = quick {
                button {
                    style: "display: flex; align-items: center; gap: 10px; width: 100%; margin: 4px 0 12px; padding: 8px 12px 8px 8px; border-radius: 14px; \
                            background: #111114; border: 1px solid {hsl(hue, 40, 28)}; color: inherit; text-align: left;",
                    div { style: "width: 32px; height: 32px; border-radius: 8px; overflow: hidden; background: #000; flex-shrink: 0;",
                        Cover { art: logo, hue: shue, radius: 8 }
                    }
                    div { style: "flex: 1; min-width: 0; font-size: 14px; color: {INK3};",
                        b { style: "color: {INK}; font-weight: 650;", "{name}" }
                        " · {when}"
                    }
                    span { style: "display: flex; align-items: center; gap: 6px; font-size: 13px; font-weight: 650; color: {hsl(hue, 75, 72)};",
                        Icon { name: icon, size: 15, stroke: 2.0 }
                        "Open in rig"
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; gap: 8px;",
                for r in s.rigs.iter() {
                    div {
                        key: "{r.name}",
                        style: if r.loaded { format!("display: flex; align-items: center; gap: 14px; min-height: 76px; padding: 12px 16px 12px 12px; border-radius: 18px; \
                                                      background: linear-gradient(100deg, {}, #111114 70%); border: 1px solid {};", hsl(r.hue, 35, 14), hsl(r.hue, 45, 32)) }
                               else { "display: flex; align-items: center; gap: 14px; min-height: 76px; padding: 12px 16px 12px 12px; border-radius: 18px; background: #111114; border: 1px solid #1c1c21;".to_owned() },
                        div { style: "width: 52px; height: 52px; border-radius: 15px; flex-shrink: 0; display: flex; align-items: center; justify-content: center; \
                                      background: {hsl(r.hue, 40, 16)}; color: {hsl(r.hue, 75, 72)};",
                            Icon { name: r.icon, size: 26, stroke: 1.7 }
                        }
                        div { style: "flex: 1; min-width: 0;",
                            div { style: "font-size: 20px; font-weight: 720; letter-spacing: -0.02em; color: {INK};",
                                "{r.name}"
                                small { style: "font-size: 12px; font-weight: 550; color: {INK3}; margin-left: 8px; letter-spacing: 0;", "{r.input}" }
                            }
                            div { style: "font-size: 14px; color: {INK3}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                "{r.preset.clone().unwrap_or_else(|| \"Not set up\".into())}"
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
