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

use honeycomb_core::{ReadError, ReadOpts, read_turtle, write_turtle};

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
/// THE SECOND MATCH ARM IS THE ONE THAT SAYS WHY THIS MATTERS. Accepting the
/// document is not merely lenient: the very next `write_turtle` emits
/// `hive:from d:at-hall`, so a reader that guesses here is a reader that edits
/// the author's file on their behalf, silently, on a save they asked for for a
/// completely different reason.
///
/// THE ERROR VARIANT AND ITS FIELDS ARE ASSERTED, NOT JUST "IT ERRED". A bare
/// `read.is_err()` stays green if the reader starts refusing this document for
/// an unrelated reason — a syntax regression upstream of the end resolution,
/// say — and the rewrite this test exists to catch could come back with nobody
/// noticing until a document that used to round-trip stopped. Naming
/// `ReadError::UnresolvedLinkEnd` and its three fields ties the test to the
/// one failure it is about.
#[test]
fn a_link_end_that_names_a_tile_subject_is_refused_rather_than_re_homed() {
    match read_turtle(END_NAMES_A_TILE_SUBJECT, &ReadOpts::default()) {
        Err(ReadError::UnresolvedLinkEnd {
            link,
            predicate,
            iri,
        }) => {
            assert_eq!(link, "https://example.org/honeycomb/ends/corridor");
            assert_eq!(predicate, "hive:from");
            assert_eq!(iri, "https://example.org/honeycomb/ends/hall");
        }
        Ok(d) => {
            let back = write_turtle(
                &d,
                &honeycomb_core::WriteOpts::new(
                    "https://example.org/honeycomb/ends/",
                    "d",
                    "https://example.org/honeycomb/ends/",
                )
                .expect("well-formed write options"),
            );
            panic!(
                "`hive:from d:hall` was accepted. `d:hall` is the TILE subject and a link end \
                 is a hive:Placement, so this document does not satisfy hsh:LinkShape — and the \
                 reader did not refuse it, it re-homed it. The next export writes the statement \
                 the author never wrote:\n{}",
                back.lines()
                    .find(|l| l.contains("hive:from"))
                    .unwrap_or("<no hive:from line>")
            );
        }
        Err(other) => panic!(
            "the document was refused, but not for the rewrite this test exists to catch: \
             {other:?}"
        ),
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

/// `d:dual` is listed under BOTH `hive:placement` and `hive:group`, and
/// carries every predicate either role needs: `hive:col`/`hive:row`/`hive:tile`
/// for a placement, `hive:slug`/`rdfs:label` for a group. If a reader could
/// resolve one IRI to either kind, the order `end()` tries them in — see
/// `ttl/read.rs`'s "PLACEMENTS FIRST, THEN GROUPS" — would decide which link
/// end a document like this one draws, with nothing pinning that choice.
const IRI_NAMED_AS_BOTH_A_PLACEMENT_AND_A_GROUP: &str = r#"
@base <https://example.org/honeycomb/ends/> .
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d: <https://example.org/honeycomb/ends/> .

d:sheet a hive:Diagram ;
    hive:slug "sheet" ;
    rdfs:label "Sheet" ;
    hive:mode hive:standalone ;
    hive:lattice hive:oddRPointyTop ;
    hive:placement d:dual , d:at-library ;
    hive:group d:dual .

d:dual a hive:Placement, hive:Group ;
    hive:col 0 ; hive:row 0 ; hive:tile d:hall ;
    hive:slug "dual" ; rdfs:label "Dual" .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Town Hall" .
d:library a hive:Tile ; hive:slug "library" ; rdfs:label "Library" .
d:at-library a hive:Placement ; hive:col 1 ; hive:row 0 ; hive:tile d:library .
"#;

/// THE REASON THE ORDER CANNOT MATTER, PINNED RATHER THAN ASSERTED IN PROSE.
/// `read_placement`'s shape is CLOSED over exactly `hive:col`, `hive:row`,
/// `hive:inGroup`, `hive:represents` and `hive:tile` — `hive:slug` is not
/// among them — while registering a subject as a group is IMPOSSIBLE without
/// one (`GroupId` comes from it). So a subject with the predicates a group
/// needs is a subject the placement closed-shape check refuses the moment it
/// is also read as a placement, and this document is refused for exactly that
/// — before the diagram it describes exists, let alone before any link end is
/// resolved against it. THE SETS `end()` CHOOSES BETWEEN ARE THEREFORE ALWAYS
/// DISJOINT: no document that reaches link resolution at all can have an IRI
/// in both, so trying groups before placements there would produce the
/// identical result for every input — an equivalent mutation, not a
/// behaviour. If `read_placement`'s allowed predicates ever grow to include
/// `hive:slug`, this test is what stops being true, and is the signal to
/// revisit `end()`'s ordering comment along with it.
#[test]
fn an_iri_cannot_be_read_as_both_a_placement_and_a_group() {
    match read_turtle(
        IRI_NAMED_AS_BOTH_A_PLACEMENT_AND_A_GROUP,
        &ReadOpts::default(),
    ) {
        Err(ReadError::ContractViolated {
            subject, predicate, ..
        }) => {
            assert_eq!(subject, "https://example.org/honeycomb/ends/dual");
            assert_eq!(
                predicate, "https://semantic.ds-labs.org/vocab/honeycomb#slug",
                "refused for a predicate other than the group-only one this test is about"
            );
        }
        other => panic!(
            "an IRI serving as both a placement and a group was not refused as a closed-shape \
             violation on the placement side: {other:?}"
        ),
    }
}
