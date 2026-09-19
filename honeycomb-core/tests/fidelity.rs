//! Round-trip fidelity: what a save is not allowed to throw away.
//!
//! `ttl/read.rs` walks a diagram OUTWARD from `a hive:Diagram`, through
//! `hive:group`, `hive:placement`, `hive:tile` and `hive:link`. That is a
//! choice about which subjects the MODEL needs, not about which subjects the
//! FILE contains — and until now the two were treated as the same thing.
//! `extras()` also filtered every `rdf:type` unconditionally, on the
//! assumption that a subject's type is always exactly the one class this
//! crate itself assigns.
//!
//! Both are confirmed losses against real files, not hypothetical ones: a
//! conformant `hive:Tile` this diagram does not place, a host's own subject
//! named from nowhere the model reaches, an ontology header, and — because
//! every Protégé/OWL-API export adds `owl:NamedIndividual` to every subject —
//! a second `rdf:type` on ordinary, already-placed content. This file pins
//! that all four now survive a save, and that a second save changes nothing:
//! the fixed-point check this codebase has been bitten by twice (see
//! `read.rs`'s own comments on `consumed` lists) is not optional here.
//!
//! LANGUAGE TAGS ARE DELIBERATELY NOT COVERED. `"Mairie"@fr` losing its tag on
//! the way out is a separate, real defect — `read.rs`'s own `label_of` names
//! it — but fixing it needs a model change (`Label { value, lang }` in place
//! of a bare `String`), which is a decision nobody has taken yet. Extending
//! this file to cover it would be building on a premise this crate does not
//! hold.

use honeycomb_core::{ReadOpts, WriteOpts, read_turtle, read_turtle_all, write_turtle};

const BASE: &str = "https://example.org/d/";
const SUBJECT_NS: &str = "https://example.org/d/";

fn opts() -> WriteOpts {
    WriteOpts::new(BASE, "d", SUBJECT_NS)
        .unwrap_or_else(|e| panic!("the fixture write options were refused ({e:?})"))
}

/// AN UNPLACED `hive:Tile`, A HOST'S OWN SUBJECT, AND AN ONTOLOGY HEADER — THE
/// THREE CONCRETE LOSSES THE DECISION NAMES — ALL IN ONE DOCUMENT, so that
/// fixing one kind while leaving another broken cannot pass this test by
/// accident.
///
/// `d:annex` is a conformant `hive:Tile`: valid slug, a label, nothing
/// `hsh:TileShape` would reject. It is simply never named by any
/// `hive:placement`, which is exactly the situation `hive:Tile`'s own
/// documentation describes as ordinary — "kept for later". `d:style-civic` is
/// the kind of subject a host hangs style facts on and points AT from
/// `hive:styleKey`, never one this crate walks to. `<https://example.org/d>`
/// stands in for an ontology or dataset header a hand-edited file might carry
/// alongside its diagram.
#[test]
fn subjects_no_walk_from_the_diagram_reaches_survive_a_round_trip() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" ; hive:styleKey d:style-civic .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .

d:annex a hive:Tile ; hive:slug "annex" ; rdfs:label "Annex (kept for later)" .
d:style-civic a owl:NamedIndividual ; rdfs:label "Civic ground" .
<https://example.org/d> a owl:Ontology ; rdfs:label "Planning data" .
"#;
    let d = read_turtle(src, &ReadOpts::default()).unwrap_or_else(|e| {
        panic!(
            "a document this crate's own writer could plausibly have hand-edited was refused: {e:?}"
        )
    });

    let o = opts();
    let once = write_turtle(&d, &o);

    assert!(
        once.contains("annex") && once.contains("Annex (kept for later)"),
        "an unplaced hive:Tile this diagram never reaches through hive:placement vanished on \
         export, so a document with content \"kept for later\" quietly loses it the first time \
         anybody drags a hexagon: {once}"
    );
    assert!(
        once.contains("style-civic") && once.contains("Civic ground"),
        "a host's own subject that the diagram points AT (via hive:styleKey) but never walks TO \
         vanished on export: {once}"
    );
    assert!(
        once.contains("Planning data"),
        "a subject with no relationship to the diagram at all — an ontology header, in a real \
         file — vanished on export even though nothing about it conflicts with anything this \
         crate models: {once}"
    );

    let reread = read_turtle(&once, &ReadOpts::default()).unwrap_or_else(|e| {
        panic!("the writer's own output, carrying unreached subjects, does not parse: {e:?}")
    });
    let twice = write_turtle(&reread, &o);
    assert_eq!(
        once, twice,
        "a document carrying unreached subjects is not a fixed point: a subject parsed but left \
         out of every 'consumed' list is read back as an extra and written a SECOND time — the \
         exact trap this file's own module doc names"
    );
}

/// `d:hall a hive:Tile , ex:Building` MUST COME BACK WITH `ex:Building` STILL
/// ON IT. `extras()` used to filter `rdf:type` unconditionally — see its old
/// doc, reproduced faithfully in this crate's git history — so ANY second type
/// was gone on the very first save, not merely an exotic one.
#[test]
fn an_extra_rdf_type_alongside_the_modelled_one_survives_a_round_trip() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .
@prefix ex:   <https://example.org/planning#> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall .

d:hall a hive:Tile , ex:Building ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#;
    let d = read_turtle(src, &ReadOpts::default()).expect("the document parses");

    let o = opts()
        .with_prefix("ex", "https://example.org/planning#")
        .unwrap();
    let once = write_turtle(&d, &o);
    assert!(
        once.contains("ex:Building"),
        "a hive:Tile's extra rdf:type (ex:Building) was dropped on export: {once}"
    );
    assert!(
        once.contains("a hive:Tile , ex:Building"),
        "the surviving type was not folded into the head's own `a` list, so it printed as an \
         ordinary predicate line under a raw rdf:type IRI instead of the shape the file arrived \
         in: {once}"
    );
    assert!(
        !once.contains("a hive:Tile , hive:Tile"),
        "the modelled class was written twice — once from the head this crate always writes, \
         once from the extra it should have been recognised as already covering: {once}"
    );

    let reread = read_turtle(&once, &ReadOpts::default()).expect("the writer's own output parses");
    assert_eq!(
        write_turtle(&reread, &o),
        once,
        "a subject carrying an extra rdf:type is not a fixed point"
    );
}

/// THE FAILURE AS IT ACTUALLY ARRIVES: every Protégé/OWL-API export adds
/// `owl:NamedIndividual` to EVERY subject, not just tiles — so a Protégé-touched
/// file loses a type on the diagram, every group, every tile and every link,
/// all at once, on first save. One subject of each kind, all typed twice,
/// closes the whole claim rather than one corner of it.
#[test]
fn owl_named_individual_survives_on_every_kind_of_subject() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram , owl:NamedIndividual ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:group d:civic ;
  hive:placement d:at-hall , d:at-annex ;
  hive:link d:corridor .

d:civic a hive:Group , owl:NamedIndividual ; hive:slug "civic" ; rdfs:label "Civic" .
d:hall a hive:Tile , owl:NamedIndividual ; hive:slug "hall" ; rdfs:label "Hall" .
d:annex a hive:Tile , owl:NamedIndividual ; hive:slug "annex" ; rdfs:label "Annex" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 ; hive:inGroup d:civic .
d:at-annex a hive:Placement ; hive:tile d:annex ; hive:col 1 ; hive:row 0 .
d:corridor a hive:Link , owl:NamedIndividual ; hive:slug "corridor" ;
  hive:from d:at-hall ; hive:to d:at-annex .
"#;
    let d = read_turtle(src, &ReadOpts::default()).expect("the document parses");

    let o = opts()
        .with_prefix("owl", "http://www.w3.org/2002/07/owl#")
        .unwrap();
    let once = write_turtle(&d, &o);
    for (subject, class) in [
        ("d:sheet", "hive:Diagram"),
        ("d:civic", "hive:Group"),
        ("d:hall", "hive:Tile"),
        ("d:corridor", "hive:Link"),
    ] {
        assert!(
            once.contains(&format!("{subject} a {class} , owl:NamedIndividual")),
            "{subject}'s owl:NamedIndividual did not survive next to its {class}: {once}"
        );
    }

    let reread = read_turtle(&once, &ReadOpts::default()).expect("the writer's own output parses");
    assert_eq!(
        write_turtle(&reread, &o),
        once,
        "a document where every subject carries an extra type is not a fixed point"
    );
}

/// A KNOWN, DELIBERATE LIMIT, PINNED SO IT CANNOT DRIFT INTO A SILENT
/// CORRUPTION LATER. With two diagrams in one file there is no principled
/// diagram to attribute a stray subject to; `read_turtle_all`'s own doc argues
/// that guessing "the first one" is worse than the status quo, because a
/// corpus that re-exports each diagram separately (as this codebase's own
/// `one_document_may_hold_one_diagram_of_each_mode` does) would then either
/// duplicate the stray subject across both exports or silently drop it from
/// one of them — the exact defect this file exists to close, reintroduced one
/// level up. This test is not pinning new behaviour; it is pinning that the
/// safe choice was made on purpose and stays made.
#[test]
fn a_stray_subject_is_not_attached_to_either_diagram_of_a_multi_diagram_document() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .
@prefix s:    <https://example.org/s/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall .
d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .

s:sketch a hive:Diagram ;
  hive:slug "sketch" ; rdfs:label "Sketch" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement s:at-left .
s:left a hive:Tile ; hive:slug "left" ; rdfs:label "Left" .
s:at-left a hive:Placement ; hive:tile s:left ; hive:col 0 ; hive:row 0 .

d:stray a hive:Tile ; hive:slug "stray" ; rdfs:label "Stray" .
"#;
    let all = read_turtle_all(src, &ReadOpts::default()).expect("both diagrams parse");
    assert_eq!(all.len(), 2, "the fixture must hold both diagrams");
    for d in &all {
        assert!(
            d.unreached().is_empty(),
            "{:?} was handed a stray subject it has no principled claim to; a document with two \
             diagrams must leave unreached subjects unattached rather than guess",
            d.slug()
        );
    }
}
