//! `hive:generator` was the only version marker a saved file ever carried, and
//! it is free text nothing here ever inspected, overwritten by every writer
//! that re-saves the file. Verified against the 0.1.0 code at commit
//! 5818508: that build's `model.rs` had no `links` field at all, so a 0.1.0
//! EDITOR OPENING A 0.2.0 FILE DROPS EVERY LINK ON SAVE, SILENTLY — the file
//! still "loads", the board still draws, and the one thing missing is a line
//! nobody asked to remove.
//!
//! `hive:formatVersion` is this crate's answer, and this file is two things at
//! once: the pin that `honeycomb-core/fixtures/v0.1.0-document.ttl` and
//! `v0.2.0-document.ttl` — neither of which existed before this decision,
//! which is exactly why "an older file still reads" had never actually been
//! verified — still read correctly, and the pin that a document naming a
//! version this crate does not implement is refused or accepted on the terms
//! the vocabulary now states.
//!
//! WHAT THIS CANNOT DO, said once rather than implied: no scheme here
//! retroactively fixes the 0.1.0 and 0.2.0 readers already in the wild. This
//! is forward-looking only — every reader from this release on knows what a
//! later file is claiming; nothing changes for the readers that shipped
//! before it existed.

use honeycomb_core::{ReadError, ReadOpts, Version, read_turtle, write_turtle};

fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} could not be read at {path}: {e}"))
}

/// THE GAP THIS DECISION CLOSES, NAMED DIRECTLY: v0.1.0-document.ttl has no
/// `hive:link` at all (the property did not exist when it was written) and
/// v0.2.0-document.ttl has one (`d:corridor`) but no `hive:formatVersion`
/// (that property did not exist yet either). Both must still read cleanly
/// under this crate's CURRENT reader, tile-for-tile and link-for-link — the
/// property this test exists to pin, because until now nothing in this
/// repository ever read a file an earlier release actually produced.
#[test]
fn both_earlier_fixtures_read_correctly_under_the_current_reader() {
    let v1 = read_turtle(&fixture("v0.1.0-document.ttl"), &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the 0.1.0-shaped fixture was refused: {e:?}"));
    assert_eq!(
        v1.cells().count(),
        2,
        "the 0.1.0 fixture should place two tiles"
    );
    assert_eq!(
        v1.links().len(),
        0,
        "0.1.0 could not have written a hive:link at all"
    );
    assert!(
        v1.format_version().is_none(),
        "0.1.0 could not have written hive:formatVersion; a reader that invents one out of its \
         absence is guessing"
    );

    let v2 = read_turtle(&fixture("v0.2.0-document.ttl"), &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the 0.2.0-shaped fixture was refused: {e:?}"));
    assert_eq!(
        v2.cells().count(),
        2,
        "the 0.2.0 fixture should place two tiles"
    );
    assert_eq!(
        v2.links().len(),
        1,
        "the 0.2.0 fixture's one hive:link (d:corridor) went missing on read — the exact \
         failure a 0.1.0 reader has for every link in every file, reproduced here one release \
         later if this regresses"
    );
    assert!(
        v2.format_version().is_none(),
        "0.2.0 could not have written hive:formatVersion either"
    );
}

/// A LATER MAJOR IS REFUSED, BY NAME, BEFORE A `Diagram` EXISTS AT ALL. A
/// major bump is this crate's own promise that a term may have been removed
/// or reshaped; reading such a file anyway is reading it under rules the file
/// itself says do not apply.
#[test]
fn a_document_naming_a_later_major_format_version_is_refused() {
    let current = Version::current();
    let future_major = format!("{}.0.0", current.major + 1);
    let src = format!(
        r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:formatVersion "{future_major}" ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#
    );
    match read_turtle(&src, &ReadOpts::default()) {
        Err(ReadError::UnsupportedFormatVersion { found, supported }) => {
            assert_eq!(found, Version::parse(&future_major).unwrap());
            assert_eq!(supported, current);
        }
        other => panic!(
            "a document naming format version {future_major} against this build's {current} was \
             not refused as UnsupportedFormatVersion: {other:?}"
        ),
    }
}

/// A LATER MINOR IS ACCEPTED, AND SAYS SO ON THE DIAGRAM. A minor bump only
/// ever adds an optional term, so an older reader still reads the document
/// correctly — it just cannot offer whatever the new term enables, which is
/// worth a host being ABLE to notice without refusing the file over it.
#[test]
fn a_document_naming_a_later_minor_format_version_is_accepted_and_flagged() {
    let current = Version::current();
    let future_minor = format!("{}.{}.0", current.major, current.minor + 1);
    let src = format!(
        r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:formatVersion "{future_minor}" ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#
    );
    let d = read_turtle(&src, &ReadOpts::default())
        .unwrap_or_else(|e| panic!("a later MINOR must be accepted, not refused: {e:?}"));
    let found = d
        .format_version()
        .expect("the document's own hive:formatVersion did not survive the read");
    assert_eq!(found, Version::parse(&future_minor).unwrap());
    assert!(
        found.is_newer_minor_than(current),
        "{found} against this build's {current} should read as a newer minor, so a host asking \
         has something to warn about"
    );
}

/// EVERYTHING ELSE — OLDER, OR EXACTLY THIS BUILD'S OWN VERSION — IS ACCEPTED
/// WITHOUT COMMENT. Neither is a reason to say anything: an older file is the
/// ordinary case this whole decision exists to keep working, and a file
/// stating exactly this build's version is simply correct.
#[test]
fn an_older_or_equal_format_version_is_accepted_and_never_flagged_as_newer() {
    let current = Version::current();
    for stated in [current.to_string(), "0.0.1".to_string()] {
        let src = format!(
            r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:formatVersion "{stated}" ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#
        );
        let d = read_turtle(&src, &ReadOpts::default())
            .unwrap_or_else(|e| panic!("hive:formatVersion {stated:?} must be accepted: {e:?}"));
        assert!(
            !d.format_version()
                .expect("the stated version did not survive the read")
                .is_newer_minor_than(current),
            "{stated:?} against this build's {current} was flagged as a newer minor, which it is \
             not"
        );
    }
}

/// THE WRITER ALWAYS STATES ITS OWN VERSION, NEVER WHAT IT READ — the same
/// rule `hive:generator` already follows, for the same reason: a file this
/// build is about to save is a file THIS build is now answering for.
#[test]
fn the_writer_always_states_its_own_version_never_the_one_it_read() {
    let d = read_turtle(&fixture("v0.1.0-document.ttl"), &ReadOpts::default())
        .expect("the fixture reads");
    assert!(
        d.format_version().is_none(),
        "the fixture states no version to begin with"
    );

    let o = honeycomb_core::WriteOpts::new(
        "https://example.org/honeycomb/v0-1-0/",
        "d",
        "https://example.org/honeycomb/v0-1-0/",
    )
    .unwrap();
    let once = write_turtle(&d, &o);
    let current = Version::current();
    assert!(
        once.contains(&format!("hive:formatVersion \"{current}\"")),
        "the writer did not state its own version on a document that had none at all: {once}"
    );

    // AND THE FIXED POINT: reading this back states the SAME version this
    // build just wrote, so a second save is not a second overwrite of
    // anything.
    let reread = read_turtle(&once, &ReadOpts::default()).expect("the writer's own output reads");
    assert_eq!(reread.format_version(), Some(current));
    assert_eq!(
        write_turtle(&reread, &o),
        once,
        "a document now stating this build's own format version is not a fixed point"
    );
}
