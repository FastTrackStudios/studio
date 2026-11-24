//! The views' layout switches: the views' contract's
//! (`daw_view::settings`), whose focus defaults are the template's.

pub use daw_view::settings::*;

#[cfg(test)]
mod tests {
    #[test]
    fn the_focus_defaults_are_the_template_s() {
        let focus = dynamic_template::scenes::TABLES.focus;
        assert!((super::FOCUS_FRACTION - focus.fraction).abs() < f64::EPSILON);
        assert!((super::FOCUS_GUTTER - focus.gutter).abs() < f64::EPSILON);
        assert!((super::FOCUS_LEGIBLE - focus.legible).abs() < f64::EPSILON);
    }
}
