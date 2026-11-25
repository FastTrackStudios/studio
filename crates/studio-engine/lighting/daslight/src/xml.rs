//! A small owned XML tree that writes back what it read.
//!
//! Daslight's files are read with roxmltree and held here so they can be
//! edited. Attribute order and whitespace text are kept, so a file that is
//! read and written unchanged comes back the same apart from how characters
//! are escaped.

use crate::Error;

/// One node: an element or a run of text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Element(Element),
    Text(String),
}

/// An element with its attributes in document order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Builder form of [`Element::set`].
    #[allow(
        clippy::needless_pass_by_value,
        reason = "takes numbers and strings alike, by value, as a builder"
    )]
    #[must_use]
    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.set(key, value);
        self
    }

    /// Builder form of [`Element::push`].
    #[must_use]
    pub fn child(mut self, child: Self) -> Self {
        self.push(child);
        self
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Sets an attribute, keeping its place if it is already there.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "takes numbers and strings alike, by value, as a builder"
    )]
    pub fn set(&mut self, key: &str, value: impl ToString) {
        let value = value.to_string();
        if let Some(slot) = self.attrs.iter_mut().find(|(k, _)| k == key) {
            slot.1 = value;
        } else {
            self.attrs.push((key.to_owned(), value));
        }
    }

    pub fn push(&mut self, child: Self) {
        self.children.push(Node::Element(child));
    }

    pub fn elements(&self) -> impl Iterator<Item = &Self> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Self> {
        self.children.iter_mut().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Self> {
        self.elements().find(|e| e.name == name)
    }

    pub fn find_mut(&mut self, name: &str) -> Option<&mut Self> {
        self.elements_mut().find(|e| e.name == name)
    }

    /// Every descendant (not self) called `name`, depth first.
    #[must_use]
    pub fn descendants(&self, name: &str) -> Vec<&Self> {
        let mut out = Vec::new();
        for e in self.elements() {
            if e.name == name {
                out.push(e);
            }
            out.extend(e.descendants(name));
        }
        out
    }

    /// Removes every child element, keeping none of the old whitespace.
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    /// Parses a whole document and returns its root element.
    ///
    /// # Errors
    /// The text is not well-formed XML.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let doc = roxmltree::Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .map_err(|e| Error::Xml(e.to_string()))?;
        Ok(Self::from_node(doc.root_element()))
    }

    fn from_node(node: roxmltree::Node<'_, '_>) -> Self {
        let mut el = Self::new(node.tag_name().name());
        for a in node.attributes() {
            el.attrs.push((a.name().to_owned(), a.value().to_owned()));
        }
        for c in node.children() {
            if c.is_element() {
                el.children.push(Node::Element(Self::from_node(c)));
            } else if c.is_text()
                && let Some(t) = c.text()
            {
                el.children.push(Node::Text(t.to_owned()));
            }
        }
        el
    }

    /// The element as XML, with no declaration.
    #[must_use]
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    fn write(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for (k, v) in &self.attrs {
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            escape_into(v, true, out);
            out.push('"');
        }
        if self.children.is_empty() {
            out.push_str("/>");
            return;
        }
        out.push('>');
        for c in &self.children {
            match c {
                Node::Element(e) => e.write(out),
                Node::Text(t) => escape_into(t, false, out),
            }
        }
        out.push_str("</");
        out.push_str(&self.name);
        out.push('>');
    }
}

/// Daslight writes apostrophes bare inside double-quoted attributes
/// (`U'King`), so only what XML requires is escaped.
fn escape_into(text: &str, attribute: bool, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            '\n' if attribute => out.push_str("&#10;"),
            _ => out.push(ch),
        }
    }
}
