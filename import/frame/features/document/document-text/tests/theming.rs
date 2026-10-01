//! Tokens and bindings survive the text form.

use std::collections::BTreeMap;

use document_proto::{ResizeY, Scene};

const SRC: &str = r##"<Document id="00000000-0000-0000-0000-000000000001">
  <Variables modes="dark light">
    <Var name="strip.ground" value="#1c1c1c" light="#ececec" />
    <Var name="strip.ink" dark="#e0e0e0" light="#202020" />
    <Var name="pad" value="4" />
  </Variables>
  <Page name="mcp.strip" id="00000000-0000-0000-0000-000000000002">
    <Frame name="Strip" id="00000000-0000-0000-0000-000000000003" w=86 h=400 bind:fill="$strip.ground">
      <Text name="Name" id="00000000-0000-0000-0000-000000000004" y=380 w=86 h=16 resize-y="bottom" bind:visible="w >= 40" bind:text="track.name">Vocal</Text>
    </Frame>
  </Page>
</Document>
"##;

fn scene() -> Scene {
    document_text::parse(SRC, BTreeMap::new()).expect("parses")
}

#[test]
fn variables_are_read_with_their_modes() {
    let s = scene();
    assert_eq!(s.variables.modes, ["dark", "light"]);
    let ground = &s.variables.vars["strip.ground"];
    assert_eq!(ground.in_mode("dark"), "#1c1c1c");
    assert_eq!(ground.in_mode("light"), "#ececec");
    // No value=: the first mode's stands in.
    assert_eq!(s.variables.vars["strip.ink"].value, "#e0e0e0");
    assert_eq!(s.variables.vars["pad"].in_mode("light"), "4");
}

#[test]
fn bindings_and_vertical_resizing_are_read() {
    let s = scene();
    let name = s.nodes.values().find(|n| n.name == "Name").expect("Name");
    let binds: Vec<(&str, &str)> = name.bind.iter().map(|b| (b.prop.as_str(), b.expr.as_str())).collect();
    assert_eq!(binds, [("visible", "w >= 40"), ("text", "track.name")]);
    assert_eq!(name.layout_child.resize_y, Some(ResizeY::Bottom));
}

#[test]
fn the_text_form_round_trips() {
    let s = scene();
    let printed = document_text::print(&s);
    let again = document_text::parse(&printed, BTreeMap::new()).expect("reparses");
    assert_eq!(s, again, "{printed}");
    assert!(printed.contains("<Variables modes=\"dark light\">"), "{printed}");
}

#[test]
fn variables_outside_the_document_are_refused() {
    let src = r#"<Document><Page><Variables modes="dark" /></Page></Document>"#;
    let err = document_text::parse(src, BTreeMap::new()).unwrap_err();
    assert!(err.to_string().contains("belongs directly inside <Document>"), "{err}");
}

#[test]
fn an_unknown_binding_is_refused() {
    let src = r#"<Document><Page><Frame bind:colour="$x" /></Page></Document>"#;
    assert!(document_text::parse(src, BTreeMap::new()).is_err());
}
