//! Window ↔ surface geometry and winit → surface input mapping. Pure, so
//! the host-specific half of the interaction contract is unit-tested.

use frame::surface::runtime::{Bounds, Key, Modifiers};
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

/// Fit-to-window transform: the surface's content box scaled uniformly to
/// fill the window's physical pixels, centered (letterboxed). Rendering in
/// physical pixels makes it HiDPI-aware for free; winit reports cursor
/// positions in the same physical pixels, so one inverse maps them back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
}

impl View {
    #[must_use]
    pub fn fit(content: Bounds, width: f64, height: f64) -> Self {
        if content.width <= 0.0 || content.height <= 0.0 || width <= 0.0 || height <= 0.0 {
            return Self {
                scale: 1.0,
                tx: -content.x,
                ty: -content.y,
            };
        }
        let scale = (width / content.width).min(height / content.height);
        Self {
            scale,
            tx: (width - content.width * scale) / 2.0 - content.x * scale,
            ty: (height - content.height * scale) / 2.0 - content.y * scale,
        }
    }

    /// Window (physical px) → surface coordinates.
    #[must_use]
    pub fn to_surface(self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.tx) / self.scale, (y - self.ty) / self.scale)
    }

    /// Surface coordinates → window (physical px).
    #[must_use]
    pub fn to_window(self, x: f64, y: f64) -> (f64, f64) {
        (x * self.scale + self.tx, y * self.scale + self.ty)
    }

    #[must_use]
    pub fn affine(&self) -> kurbo::Affine {
        kurbo::Affine::new([self.scale, 0.0, 0.0, self.scale, self.tx, self.ty])
    }
}

/// Max gap between two presses to count as a double-click.
pub const DOUBLE_CLICK_MS: f64 = 350.0;
/// Max travel (logical px) between two presses of a double-click.
pub const DOUBLE_CLICK_PX: f64 = 4.0;

/// Detects double-clicks from successive presses (logical px, ms).
#[derive(Debug, Clone, Copy, Default)]
pub struct Clicks {
    last: Option<(f64, f64, f64)>,
}

impl Clicks {
    /// Record a press; true when it completes a double-click. A triple
    /// press is a double-click then a fresh first click.
    pub fn press(&mut self, x: f64, y: f64, t: f64) -> bool {
        let double = self.last.is_some_and(|(lx, ly, lt)| {
            t - lt <= DOUBLE_CLICK_MS && (x - lx).hypot(y - ly) <= DOUBLE_CLICK_PX
        });
        self.last = if double { None } else { Some((x, y, t)) };
        double
    }
}

/// Shift = fine, Cmd/Ctrl = alternate (reset-on-click).
#[must_use]
pub fn modifiers(m: ModifiersState) -> Modifiers {
    Modifiers {
        fine: m.shift_key(),
        alt: m.control_key() || m.super_key(),
        option: m.alt_key(),
    }
}

#[must_use]
pub fn key(k: &WinitKey) -> Option<Key> {
    let named = match k {
        WinitKey::Named(named) => named,
        // A single character (a shortcut with Cmd/Ctrl, e.g. select all).
        WinitKey::Character(s) => {
            let mut chars = s.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) => Some(Key::Char(c)),
                _ => None,
            };
        }
        _ => return None,
    };
    Some(match named {
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::PageUp => Key::PageUp,
        NamedKey::PageDown => Key::PageDown,
        NamedKey::Home => Key::Home,
        NamedKey::End => Key::End,
        NamedKey::Space => Key::Space,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Escape,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        _ => return None,
    })
}

/// Logical px of trackpad scroll per wheel notch.
const PX_PER_NOTCH: f64 = 20.0;

/// Wheel notches, positive = up/away = increase.
#[must_use]
pub fn wheel_notches(delta: MouseScrollDelta, scale_factor: f64) -> f64 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => f64::from(y),
        MouseScrollDelta::PixelDelta(p) => p.y / (PX_PER_NOTCH * scale_factor.max(0.1)),
    }
}
