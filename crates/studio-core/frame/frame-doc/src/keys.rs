//! The `plugin_data` keys Frame keeps on figcraft nodes. Values are JSON of the
//! surface-proto / motion-proto / document-proto types, so figcraft needs no patch to carry them.

/// The node's [`Control`](frame::surface::proto::Control) without its primary binding
/// ([`BINDING`]) and slots ([`SLOT`]).
pub const CONTROL: &str = "frame:control";
/// The control's primary [`Binding`](frame::surface::proto::Binding).
pub const BINDING: &str = "frame:binding";
/// On a slot node: the roles it plays for its control ([`SlotMark`](crate::SlotMark)s).
pub const SLOT: &str = "frame:slot";
/// On a control: slots naming nodes that aren't in the document (kept so they round-trip).
pub const SLOT_EXTERN: &str = "frame:slot-extern";
/// The node's [`Motion`](frame::motion::proto::Motion) list.
pub const MOTION: &str = "frame:motion";
/// The node's [`ShaderFx`](frame::document::proto::ShaderFx).
pub const SHADER: &str = "frame:shader";
/// The node's [`Emission`](frame::document::proto::Emission).
pub const EMISSION: &str = "frame:emission";
/// Theme-language property bindings ([`Bind`](frame::document::proto::Bind) list).
pub const BIND: &str = "frame:bind";
/// Theme-language component arguments ([`Arg`](frame::document::proto::Arg) list).
pub const ARGS: &str = "frame:args";
/// Theme-language `of` (the component a use instantiates).
pub const OF: &str = "frame:of";

/// On an instance root: Frame data its layers override, by the id of the main's layer they
/// mirror (`{"<u64>": {"frame:control": "…", …}}`). The engine rebuilds an instance's layers from
/// its main on every sync, so their own `plugin_data` can't hold an override; the root's can.
pub const OVERRIDE: &str = "frame:override";

/// The v1 [`NodeId`](frame::document::proto::NodeId) (a UUID) the node had in its `Scene`.
pub const ID: &str = "frame:id";
/// The v1 fields figcraft can't hold exactly (see `convert`).
pub const V1: &str = "frame:v1";
/// On one page: the v1 scene's root record (root node, variables, assets).
pub const ROOT: &str = "frame:root";

/// The typed data keys, in the order [`FrameData`](crate::FrameData) writes them.
pub const DATA: [&str; 10] = [
    CONTROL,
    BINDING,
    SLOT,
    SLOT_EXTERN,
    MOTION,
    SHADER,
    EMISSION,
    BIND,
    ARGS,
    OF,
];

/// Whether `key` is one of Frame's.
#[must_use]
pub fn is_frame(key: &str) -> bool {
    key.starts_with("frame:")
}
