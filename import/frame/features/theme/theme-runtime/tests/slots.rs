//! Host slots: places the theme lays out for what the host draws itself.

use kurbo::Rect;
use theme_runtime::{Fields, Schema, Theme, Value};

const THEME: &str = r##"<Document>
  <Page name="parts">
    <Component name="button" w=40 h=40 arg:icon="true">
      <Rectangle name="Plate" w=40 h=40 fill="#303030" />
      <Frame name="%icon" x=4 y=4 w=32 h=32 bind:visible="arg.icon" />
    </Component>
  </Page>
  <Page name="rail">
    <Frame name="Rail" w=44 h=200>
      <Frame name="%under" w=44 h=10 />
      <Instance name="@first" of="button" x=2 y=20 arg:icon="has.icon" />
      <Frame name="%graph" x=2 y=100 w=40 h=60 bind:y="h - 70" />
    </Frame>
  </Page>
</Document>
"##;

fn theme() -> Theme {
    Theme::from_markup(THEME, Schema::new().field("has.icon"), None).unwrap_or_else(|d| panic!("{d:?}"))
}

fn slots(t: &Theme, size: (f64, f64), icon: bool) -> Vec<(String, Rect)> {
    let f: &dyn Fields = &vec![Value::Bool(icon)];
    t.slots("rail", size, f)
}

#[test]
fn slots_are_where_the_theme_puts_them_in_paint_order() {
    let t = theme();
    let s = slots(&t, (44.0, 200.0), true);
    assert_eq!(
        s,
        [
            ("under".to_owned(), Rect::new(0.0, 0.0, 44.0, 10.0)),
            // Inside a placed component, where its instance put it.
            ("icon".to_owned(), Rect::new(6.0, 24.0, 38.0, 56.0)),
            ("graph".to_owned(), Rect::new(2.0, 130.0, 42.0, 190.0)),
        ]
    );
    // Moved by its binding at another size.
    let tall = slots(&t, (44.0, 400.0), true);
    assert_eq!(tall.last().map(|(_, r)| *r), Some(Rect::new(2.0, 330.0, 42.0, 390.0)));
}

#[test]
fn a_hidden_slot_is_not_offered() {
    let t = theme();
    let names: Vec<String> = slots(&t, (44.0, 200.0), false).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["under", "graph"]);
    // And a slot is not a hit target.
    let f: &dyn Fields = &vec![Value::Bool(true)];
    assert_eq!(t.targets("rail", (44.0, 200.0), f), ["first"]);
}
