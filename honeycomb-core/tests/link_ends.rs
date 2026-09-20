//! What a `hive:from` or `hive:to` is allowed to name, and what happens to an
//! IRI that names none of it.
//!
//! WHAT BREAKS WITHOUT THIS FILE. The reader resolved a link end by taking the
//! last segment of the IRI, stripping a leading `at-` IF ONE WAS THERE, and
//! calling whatever was left a `TileId`. Nothing checked the result against the
//! document's own placements. That is not a parse, it is a GUESS, and it has
//! two silent outcomes that this file pins:
//!
//!   * `hive:from d:hall` in a standalone document names the TILE subject, not
//!     the placement — `hsh:LinkShape` says an end is a `hive:Placement` and
//!     this is a `hive:Tile`. The old reader stripped nothing, produced
//!     `TileId("hall")`, found `hall` placed, and accepted the document. The
//!     next export then wrote `hive:from d:at-hall`: the reader had silently
//!     REWRITTEN a statement the author wrote, into one they did not, with
//!     nothing anywhere saying a correction had happened.
//!   * `hive:from d:nowhere-at-all` produced `TileId("nowhere-at-all")` and
//!     reached `Diagram::try_new`, which refused it as `LinkToNowhere` — the
//!     right outcome by luck rather than by design, and the luck runs out the
//!     moment an end may legitimately name something that is not a placement.
//!
//! The rule this file asserts is: resolve an end against the document's
//! placements, and REFUSE an IRI that resolves to nothing rather than inventing
//! an identity for it.

use honeycomb_core::{ReadOpts, read_turtle, write_turtle};

/// A standalone document with two placed tiles and one link whose `hive:from`
/// points at the TILE subject `d:hall` instead of the placement `d:at-hall`.
/// Every other statement in it is well formed, so nothing but the end itself is
/// on trial here.
const END_NAMES_A_TILE_SUBJECT: &str = r#"
@base <https://example.org/honeycomb/ends/> .
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d: <https://example.org/honeycomb/ends/> .

d:sheet a hive:Diagram ;
    hive:slug "sheet" ;
    rdfs:label "Sheet" ;
    hive:mode hive:standalone ;
    hive:lattice hive:oddRPointyTop ;
    hive:placement d:at-hall , d:at-library ;
    hive:link d:corridor .

d:corridor a hive:Link ;
    hive:slug "corridor" ;
    hive:from d:hall ;
    hive:to d:at-library .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Town Hall" .
d:library a hive:Tile ; hive:slug "library" ; rdfs:label "Library" .

d:at-hall a hive:Placement ; hive:col 0 ; hive:row 0 ; hive:tile d:hall .
d:at-library a hive:Placement ; hive:col 1 ; hive:row 0 ; hive:tile d:library .
"#;

/// The same document with an end that names nothing in it at all.
const END_NAMES_NOTHING: &str = r#"
@base <https://example.org/honeycomb/ends/> .
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d: <https://example.org/honeycomb/ends/> .

d:sheet a hive:Diagram ;
    hive:slug "sheet" ;
    rdfs:label "Sheet" ;
    hive:mode hive:standalone ;
    hive:lattice hive:oddRPointyTop ;
    hive:placement d:at-hall , d:at-library ;
    hive:link d:corridor .

d:corridor a hive:Link ;
    hive:slug "corridor" ;
    hive:from d:at-hall ;
    hive:to d:at-nowhere .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Town Hall" .
d:library a hive:Tile ; hive:slug "library" ; rdfs:label "Library" .

d:at-hall a hive:Placement ; hive:col 0 ; hive:row 0 ; hive:tile d:hall .
d:at-library a hive:Placement ; hive:col 1 ; hive:row 0 ; hive:tile d:library .
"#;

/// THE REWRITE, CAUGHT. The document says `hive:from d:hall`; `d:hall` is a
/// `hive:Tile`, not a `hive:Placement`, so the file is already wrong by
/// `hsh:LinkShape` and the reader must say so rather than quietly deciding what
/// the author probably meant.
///
/// The second assertion is the one that says WHY this matters. Accepting the
/// document is not merely lenient: the very next `write_turtle` emits
/// `hive:from d:at-hall`, so a reader that guesses here is a reader that edits
/// the author's file on their behalf, silently, on a save they asked for for a
/// completely different reason.
#[test]
fn a_link_end_that_names_a_tile_subject_is_refused_rather_than_re_homed() {
    let read = read_turtle(END_NAMES_A_TILE_SUBJECT, &ReadOpts::default());

    if let Ok(d) = &read {
        let back = write_turtle(
            d,
            &honeycomb_core::WriteOpts::new(
                "https://example.org/honeycomb/ends/",
                "d",
                "https://example.org/honeycomb/ends/",
            )
            .expect("well-formed write options"),
        );
        panic!(
            "`hive:from d:hall` was accepted. `d:hall` is the TILE subject and a link end is a \
             hive:Placement, so this document does not satisfy hsh:LinkShape — and the reader \
             did not refuse it, it re-homed it. The next export writes the statement the author \
             never wrote:\n{}",
            back.lines()
                .find(|l| l.contains("hive:from"))
                .unwrap_or("<no hive:from line>")
        );
    }
}

/// The same rule for an end that names nothing at all.
///
/// STATED PLAINLY: THIS TEST WAS GREEN BEFORE THE FIX IT ACCOMPANIES. The
/// document was already refused — by `Diagram::try_new`, one layer down, as
/// `LinkToNowhere` — but only because the identity the reader invented out of
/// `at-nowhere` happened not to match a placed tile. It is a characterisation
/// of an outcome that was right for the wrong reason, kept because the reason
/// is what stops being safe once an end may legitimately name something other
/// than a placement: the sibling test above is the one that was red.
#[test]
fn a_link_end_that_names_nothing_in_the_document_is_refused() {
    let read = read_turtle(END_NAMES_NOTHING, &ReadOpts::default());
    assert!(
        read.is_err(),
        "`hive:to d:at-nowhere` names no placement this document declares and was accepted"
    );
}
