//! Turtle in and Turtle out, both HAND-WRITTEN.
//!
//! WHY NO RDF CRATE. `honeycomb-core` has zero dependencies and keeps zero —
//! that is the property which lets a host-side generator lay a diagram out
//! without linking a UI framework, and it is spent the moment one crate arrives.
//! The obvious dependency is also the wrong shape for the other half of this
//! component: oxrdf reaches wasm32 through `rand`/`getrandom` and fails to
//! compile there at all, so a parser built on it is a parser the browser half
//! cannot build — and a parser the browser cannot build cannot round-trip what
//! the browser writes.
//!
//! The subset is narrow and the reader says so by name rather than skipping what
//! it does not understand: blank nodes, property lists and collections are
//! REFUSED, because a placement or group written as a blank node is invisible to
//! a consumer gate that selects subjects by IRI prefix, and accepting one would
//! produce the single unchecked subject in an otherwise checked file.

mod read;
mod write;

pub use read::{ReadError, ReadOpts, read_turtle, read_turtle_all};
pub use write::{BadWriteOpts, WriteOpts, write_turtle};

/// The local-name prefix every placement subject carries.
///
/// A placement has no `hive:slug` — it is named by its cell and has no identity
/// a human types — so the only identifier a file carries for it is its own IRI,
/// and the reader recovers the `TileId` from it. That means the writer and the
/// reader must agree on one spelling, and this constant is that agreement: two
/// string literals a module apart can differ by one character and fail in
/// silence, with every tile coming back under a name nothing else in the
/// document uses.
///
/// It also keeps the placement subject clear of the tile subject in a standalone
/// document, where the tile's own IRI is forced to be `{ns}{slug}` by
/// `hsh:SlugMatchesIri`.
pub(crate) const PLACEMENT_PREFIX: &str = "at-";

/// `rdf:type`, spelled once so the reader's `extras()` and the writer's
/// `statement()` cannot name it two different ways. Both now have to agree on
/// this IRI for a reason neither used to have: the reader keeps SOME `a`
/// triples as extras (an unmodelled class alongside `hive:Tile`) and the writer
/// has to recognise them again to fold them back into the `a hive:Tile , ...`
/// head instead of printing a fresh predicate line for each.
pub(crate) const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Absolute IRIs have a scheme; anything else is relative and needs a base.
/// Nothing here dereferences one — the vocabulary namespace is deliberately not
/// serving yet, and an IRI is an identifier first.
pub(crate) fn has_scheme(iri: &str) -> bool {
    let Some(i) = iri.find(':') else {
        return false;
    };
    let scheme = &iri[..i];
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// The last `/`- or `#`-delimited segment of an IRI — what `hsh:SlugMatchesIri`
/// compares a `hive:slug` against, written once so the writer's naming and the
/// reader's check cannot drift.
pub(crate) fn last_segment(iri: &str) -> &str {
    match iri.rfind(['/', '#']) {
        Some(i) => &iri[i + 1..],
        None => iri,
    }
}
