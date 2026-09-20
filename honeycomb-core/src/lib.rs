//! The honeycomb lattice, its model, and the vocabulary it serialises into —
//! with no UI and no dependencies at all.
//!
//! WHY THIS CRATE HAS NO YEW IN IT. Two consumers need this arithmetic and only
//! one of them is a browser. The editor drags tiles in WebAssembly; a static
//! site generator lays the same diagram out on the host and writes the same
//! Turtle, with no wasm anywhere. If the geometry lived with the component,
//! every generator that wanted a hexagon would have to link a UI framework to
//! get one — and the two would be free to disagree about which cell a point
//! falls in, which is exactly the bug that has no symptom until a drop lands
//! somewhere nobody expected.
//!
//! WHY IT HAS NO OTHER DEPENDENCY EITHER, not even for RDF. `cargo tree` here is
//! one line, and that is the property the paragraph above is spending: a host
//! that wants to lay a diagram out links this and nothing else. The serialiser
//! and the parser are therefore hand-written, which also keeps them buildable
//! for wasm32 — the obvious RDF crate reaches that target through
//! `rand`/`getrandom` and does not compile there at all, so a parser built on it
//! could not read back what the browser half writes.
//!
//! ```text
//!   lattice   integer cells, axial arithmetic, and the one place a cell
//!             becomes a point
//!   model     the document, and the three shape rules it makes
//!             unrepresentable rather than checked
//!   rules     what a drag may do: pure `check`, atomic `apply`, undo
//!   ttl       Turtle out and Turtle back in, both hand-written
//! ```

mod lattice;
mod model;
mod rules;
mod ttl;

pub use lattice::{Axial, Cell, Frame, Lattice, route};
pub use model::{
    BadLang, BadSlug, BadTimestamp, BadVersion, Content, Diagram, DiagramSpec, Endpoint, Group,
    GroupId, Iri, LatticeConvention, Link, LinkId, Mode, ModelError, NewTile, OwnTile, PinnedTile,
    Routing, Slug, Statement, Term, Text, TileId, Timestamp, Version, anchors, components, holes,
};
pub use rules::{Command, History, Plan, Rejection};
pub use ttl::{
    BadWriteOpts, ReadError, ReadOpts, WriteOpts, read_turtle, read_turtle_all, write_turtle,
};

/// The terms a diagram is written in.
///
/// TWO NAMESPACES, NOT ONE, and the split is the point of the `{type}` segment:
/// the terms a diagram is written in live under `vocab/`, and the SHACL that
/// constrains them lives under `shapes/`. That mirrors how the consuming
/// repositories already separate `vocab/` from `shapes/` on disk, so a reader
/// who knows one knows the other.
///
/// THE IRI DOES NOT DEREFERENCE YET, and that is a deliberate, recorded state
/// rather than an oversight: semantic.ds-labs.org is not serving at the time of
/// writing. An IRI is an identifier first; `ns.ttl` at the root of this
/// repository is the authoritative bytes, and standing the host up is a separate
/// job that changes nothing here when it happens. The alternative — minting the
/// namespace from whatever host happened to be convenient — is how a vocabulary
/// ends up named after a hosting decision it later regrets.
pub const NS: &str = "https://semantic.ds-labs.org/vocab/honeycomb#";

/// The namespace the shipped SHACL shapes are written in.
pub const SHAPES_NS: &str = "https://semantic.ds-labs.org/shapes/honeycomb#";

/// What `ns.ttl` must assert in `hive:tboxVersion`. A term cannot change without
/// a version bump only if something fails when it does; the test that compares
/// this constant to the file is that something.
pub const TBOX_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Local names of every term the writer emits and the reader accepts, in ONE
/// place. Spelled from two string literals, a writer and a reader can differ by
/// one character and fail in silence: the reader simply never sees a group
/// membership, and every tile moves alone for no visible reason.
pub mod terms {
    pub mod class {
        pub const DIAGRAM: &str = "Diagram";
        pub const PLACEMENT: &str = "Placement";
        pub const GROUP: &str = "Group";
        pub const TILE: &str = "Tile";
        pub const MODE: &str = "Mode";
        pub const LATTICE: &str = "Lattice";
        pub const LINK: &str = "Link";
    }
    pub mod individual {
        pub const PINNED: &str = "pinned";
        pub const STANDALONE: &str = "standalone";
        pub const ODD_R_POINTY_TOP: &str = "oddRPointyTop";
    }
    pub mod prop {
        pub const MODE: &str = "mode";
        pub const LATTICE: &str = "lattice";
        pub const PINNED_TO: &str = "pinnedTo";
        pub const PINNED_REVISION: &str = "pinnedRevision";
        pub const PLACEMENT: &str = "placement";
        pub const GROUP: &str = "group";
        pub const GENERATOR: &str = "generator";
        pub const GENERATED_AT: &str = "generatedAt";
        pub const FORMAT_VERSION: &str = "formatVersion";
        pub const COL: &str = "col";
        pub const ROW: &str = "row";
        pub const IN_GROUP: &str = "inGroup";
        pub const REPRESENTS: &str = "represents";
        pub const TILE: &str = "tile";
        pub const SLUG: &str = "slug";
        pub const STYLE_KEY: &str = "styleKey";
        pub const NOTE: &str = "note";
        pub const TBOX_VERSION: &str = "tboxVersion";
        pub const LINK: &str = "link";
        pub const FROM: &str = "from";
        pub const TO: &str = "to";
        pub const ROUTING: &str = "routing";
    }
}
