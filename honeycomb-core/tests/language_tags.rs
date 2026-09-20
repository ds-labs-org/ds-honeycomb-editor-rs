//! `rdfs:label "Mairie"@fr` MUST COME BACK OUT STILL SAYING `@fr`.
//!
//! `ttl/read.rs`'s `as_string` reads a literal's value and drops everything
//! else about it, so every modelled string property in this crate — the
//! diagram's label, a group's, a tile's and its comment, a link's — arrived in
//! the model as bare text. `label_of`'s own doc has admitted the loss in
//! writing since it was written: "the model stores a label as a plain `String`
//! and has nowhere to keep one".
//!
//! WHAT MAKES IT WORSE THAN A MISSING FEATURE IS THE ASYMMETRY. The same
//! `@fr` on a predicate this vocabulary does NOT define survives untouched,
//! because `extras()` keeps the whole statement — tag, datatype and all. So
//! whether an author's language tag came back depended on which predicate they
//! had happened to put it on, with nothing anywhere saying so.
//!
//! WHERE A TAG IS AND IS NOT ALLOWED, because this file pins both halves and
//! they are not symmetrical. `rdfs:label` is constrained in `shapes.ttl` with
//! `sh:minLength` and no `sh:datatype`, and `rdfs:comment` carries no property
//! shape at all: a language-tagged literal conforms, so this crate must keep
//! it. `hive:note`, `hive:slug`, `hive:pinnedRevision`, `hive:generator`,
//! `hive:formatVersion` and `hive:generatedAt` are each pinned to an explicit
//! `sh:datatype` — `xsd:string` or `xsd:dateTime` — and a language-tagged
//! literal is `rdf:langString`, which is neither. A tag there is a document
//! the shipped shapes already reject, so the reader REFUSES it by name
//! instead of flattening it: reading it and writing it back without the tag
//! produces a file the author did not write, which is the one repair this
//! parser's own header says it will never make.
//!
//! THE FIXED-POINT ASSERTION IS NOT DECORATION. This codebase has been bitten
//! twice by a term that was parsed and then left out of a `consumed` list, and
//! written a second time on the next export — see `ttl/read.rs`'s comments on
//! `hive:link` and on a link's own `hive:slug`. A tag that came back on the
//! first export and doubled on the second would be the same defect wearing a
//! different hat.

use honeycomb_core::{ReadError, ReadOpts, WriteOpts, read_turtle, write_turtle};

const BASE: &str = "https://example.org/honeycomb/langue/";

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} could not be read at {path}: {e}"))
}

fn opts() -> WriteOpts {
    WriteOpts::new(BASE, "d", BASE)
        .unwrap_or_else(|e| panic!("the fixture write options were refused ({e:?})"))
}

/// THE WHOLE DEFECT, ON A DOCUMENT RATHER THAN ON A UNIT. Every modelled
/// string property the shapes allow a tag on is tagged in the fixture, so a
/// fix that carried the tag through the diagram's label and forgot a tile's
/// comment cannot pass this by accident.
#[test]
fn every_tag_the_shapes_allow_survives_a_save() {
    let src = fixture("language-tagged-document.ttl");
    let d = read_turtle(&src, &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the language-tagged fixture was refused: {e:?}"));

    let once = write_turtle(&d, &opts());
    for (what, expected) in [
        (
            "the diagram's own label",
            r#"rdfs:label "Plan de la mairie"@fr"#,
        ),
        ("a group's label", r#"rdfs:label "Centre administratif"@fr"#),
        ("a link's label", r#"rdfs:label "passage couvert"@fr"#),
        ("a tile's label", r#"rdfs:label "Mairie"@fr"#),
        (
            "a tile's comment",
            r#"rdfs:comment "Le bâtiment principal, ouvert au public"@fr"#,
        ),
        (
            "an unmodelled predicate (which never lost its tag)",
            r#""centre-ville"@fr"#,
        ),
    ] {
        assert!(
            once.contains(expected),
            "{what} lost its language tag on the way out: expected {expected} in\n{once}"
        );
    }

    assert!(
        once.contains(r#"rdfs:label "Library""#) && !once.contains(r#""Library"@"#),
        "an UNTAGGED label was stamped with a language it never carried, which invents a claim \
         the document never made:\n{once}"
    );
}

/// A SECOND EXPORT MUST BE THE FIRST ONE'S BYTES. See this file's header: a
/// tag parsed but left out of a `consumed` list comes back as a host predicate
/// and is written twice, and nothing but this comparison would catch it.
///
/// GREEN BEFORE THE FIX TOO, and worth saying rather than implying: a reader
/// that dropped every tag was trivially a fixed point, because there was
/// nothing left to double. This is the assertion that only becomes load-bearing
/// once the tag is carried, which is precisely when the trap opens.
#[test]
fn a_tagged_document_is_a_fixed_point() {
    let src = fixture("language-tagged-document.ttl");
    let d = read_turtle(&src, &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the language-tagged fixture was refused: {e:?}"));
    let o = opts();
    let once = write_turtle(&d, &o);

    let reread = read_turtle(&once, &ReadOpts::default()).unwrap_or_else(|e| {
        panic!("the writer's own tagged output cannot be read back by its own reader: {e:?}")
    });
    let twice = write_turtle(&reread, &o);
    assert_eq!(
        once, twice,
        "a document carrying language tags is not a fixed point: a tag written on the first \
         export and doubled, dropped or re-spelled on the second is the same 'parsed but not \
         consumed' trap this crate has already been bitten by twice"
    );
}

/// THE TAG IS NOT INVENTED WHERE THERE WAS NONE. A CHARACTERISATION TEST AND
/// SAID TO BE ONE: it passes before the change as well as after, because the
/// old reader could not have produced a tag at all. What it guards is the
/// other direction of the fix — a writer that defaulted an absent tag to `@en`
/// would make every existing document in the wild change on its next save.
#[test]
fn an_untagged_document_gains_no_tags() {
    let src = fixture("v0.2.0-document.ttl");
    let d = read_turtle(&src, &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the 0.2.0-shaped fixture was refused: {e:?}"));
    let out = write_turtle(
        &d,
        &WriteOpts::new(
            "https://example.org/honeycomb/v0-2-0/",
            "d",
            "https://example.org/honeycomb/v0-2-0/",
        )
        .expect("the fixture write options were refused"),
    );
    assert!(
        !out.contains("\"@"),
        "a document that carried no language tag came back with one, so every file already \
         written changes on its next save:\n{out}"
    );
}

/// The three properties whose shapes pin an explicit `sh:datatype`, each
/// carrying a tag their shape rejects. REFUSED BY NAME — the error has to say
/// which predicate to delete the tag from, because "this file is wrong" is not
/// something a contributor can act on.
#[test]
fn a_tag_on_a_datatyped_property_is_refused_and_names_the_predicate() {
    for (predicate, line) in [
        ("hive:note", r#"hive:note "relevé"@fr ;"#),
        ("hive:slug", r#"hive:slug "sheet"@fr ;"#),
        (
            "hive:generatedAt",
            r#"hive:generatedAt "2026-01-31T09:12:44Z"@fr ;"#,
        ),
    ] {
        // The offending line replaces the plain `hive:slug` the document would
        // otherwise carry, or is added beside it, so each case differs from the
        // next by exactly the tag under test.
        let src = format!(
            r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  {line}
  {slug}
  rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#,
            slug = if predicate == "hive:slug" {
                ""
            } else {
                r#"hive:slug "sheet" ;"#
            }
        );
        match read_turtle(&src, &ReadOpts::default()) {
            Err(ReadError::ContractViolated { predicate: p, .. }) => assert_eq!(
                p, predicate,
                "the refusal named the wrong predicate, so a contributor is sent to the wrong \
                 line of their own file"
            ),
            other => panic!(
                "a language tag on {predicate} is a document the shipped shapes reject \
                 (sh:datatype), and reading it back without the tag rewrites what the author \
                 wrote; it was not refused: {other:?}"
            ),
        }
    }
}

/// A TAG THIS CRATE'S OWN LEXER ACCEPTS AND A STRICT PARSER DOES NOT. Turtle's
/// LANGTAG is `[a-zA-Z]+ ('-' [a-zA-Z0-9]+)*`; the lexer here scans
/// `[A-Za-z0-9-]+` and is documented as deliberately tolerant. Carrying `fr-`
/// into the model would mean writing `"Mairie"@fr-` back out — a file this
/// crate can still read and nothing else can, which is exactly the failure
/// `iri_ref`'s own doc in `ttl/write.rs` records for an unescaped `>`.
#[test]
fn a_malformed_language_tag_is_refused_rather_than_written_back_out() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Plan"@fr- ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#;
    match read_turtle(src, &ReadOpts::default()) {
        Err(ReadError::ContractViolated { predicate, .. }) => assert_eq!(
            predicate, "rdfs:label",
            "the refusal named the wrong predicate"
        ),
        other => panic!(
            "`@fr-` is not a Turtle language tag, and accepting it here means writing a document \
             back out that only this crate can read: {other:?}"
        ),
    }
}
