//! Effect → CSS box-shadow/filter values.

use frame_proto::effect::Effect;

/// Convert effects to a CSS box-shadow string.
pub fn effects_to_css_box_shadow(effects: &[Effect]) -> Option<String> {
    let shadows: Vec<String> = effects
        .iter()
        .filter_map(|e| e.to_css_box_shadow())
        .collect();

    if shadows.is_empty() {
        None
    } else {
        Some(shadows.join(", "))
    }
}

/// Convert blur effects to a CSS filter string.
pub fn effects_to_css_filter(effects: &[Effect]) -> Option<String> {
    for effect in effects {
        match effect {
            Effect::LayerBlur { radius, visible } if *visible => {
                return Some(format!("blur({radius}px)"));
            }
            _ => {}
        }
    }
    None
}

/// Convert background blur to a CSS backdrop-filter string.
pub fn effects_to_css_backdrop_filter(effects: &[Effect]) -> Option<String> {
    for effect in effects {
        match effect {
            Effect::BackgroundBlur { radius, visible } if *visible => {
                return Some(format!("blur({radius}px)"));
            }
            _ => {}
        }
    }
    None
}
