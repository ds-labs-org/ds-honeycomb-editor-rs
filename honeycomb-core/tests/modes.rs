//! The two modes, and the line between them that nothing else in this crate draws.
//!
//! WHAT BREAKS WITHOUT THIS FILE. `hive:pinned` promises that a saved layout cannot go stale, and
//! it keeps that promise only because a pinned placement has nowhere to put content: no field on
//! `PinnedTile`, no editable handle out of `Diagram`, and no reader that will accept one. Each of
//! those is a separate decision a later change can quietly relax, and relaxing any of them does not
//! crash anything — it produces a diagram that draws last year's name for as long as nobody checks.
//! `hive:standalone` makes the opposite promise, that nothing outside the document is read, and it
//! fails the same silent way: a document that points outside itself looks regenerable to a reader
//! while no regeneration will ever touch it.
//!
//! So most of what is asserted here is a REFUSAL, and every refused document is the writer's own
//! output with exactly one predicate injected into it. A hand-typed fixture would leave open whether
//! the reader refused the defect or refused the way the fixture was typed; a one-predicate mutation
//! of bytes this crate itself produced cannot be about anything else. Each mutation test therefore
//! reads the clean bytes first and asserts they are accepted — a refusal test whose control is
//! already broken proves nothing at all.
//!
//! The SHACL shapes state the same contract for graph consumers, but they run in somebody else's
//! gate one repository away, over files that arrived there. These run here, before a file exists.
//!
//! The round-trips live in this file rather than with the other serialisation tests because what
//! they check is mode-carried: a pinned document must come back still pinned, still naming the
//! source it lays out, and a standalone document must come back carrying all of its own content,
//! because there is nowhere else for that content to come from.

use std::collections::BTreeMap;

use honeycomb_core::{
    Cell, Content, Diagram, DiagramSpec, Group, GroupId, Iri, LatticeConvention, Mode, OwnTile,
    PinnedTile, ReadError, ReadOpts, Slug, Text, TileId, Timestamp, WriteOpts, read_turtle,
    read_turtle_all, write_turtle,
};

const BASE: &str = "https://example.org/data/honeycomb/";
/// Two subject namespaces, one per fixture, so the two documents can be concatenated into one file
/// without their placement subjects colliding. A collision there is not a clash this crate would
/// report; it is one placement named by two diagrams, which is a different bug entirely.
const PINNED_NS: &str = "https://example.org/data/honeycomb/diagram/";
const STANDALONE_NS: &str = "https://example.org/data/honeycomb/sketch/";

const SOURCE: &str = "https://example.org/catalogue/release-2026-01";
const REVISION: &str = "0f1e2d3c4b5a6978";
const WHEN: &str = "2026-01-31T09:12:44Z";

fn slug(s: &str) -> Slug {
    Slug::parse(s).unwrap_or_else(|e| panic!("the fixture slug {s:?} is not kebab-case ({e:?})"))
}

fn tile_id(s: &str) -> TileId {
    TileId(slug(s))
}

fn group_id(s: &str) -> GroupId {
    GroupId(slug(s))
}

fn iri(s: &str) -> Iri {
    Iri(s.to_string())
}

fn when() -> Timestamp {
    Timestamp::parse(WHEN)
        .unwrap_or_else(|e| panic!("the fixture timestamp {WHEN} carries no usable zone ({e:?})"))
}

/// `base` is left unset on purpose. The writer emits exactly one `@base`, so a fallback supplied
/// here would quietly cover for a writer that stopped emitting one, and every subject would then
/// resolve against whatever this test happened to pass.
fn read_opts() -> ReadOpts {
    ReadOpts {
        base: None,
        slug: None,
    }
}

/// A layout over an external source: two placements, negative coordinates, and one group.
///
/// The group is here deliberately. A pinned diagram may still carry its groups' own labels — the
/// closed shape is on the placement, not on the group — and an implementation that strips every
/// label in pinned mode passes a test that only inspects placements while silently dropping the
/// region headings that say why those cells are together.
fn pinned() -> Diagram {
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("vault"),
        PinnedTile {
            group: Some(group_id("platform")),
            represents: iri("https://example.org/catalogue/vault"),
        },
    );
    tiles.insert(
        tile_id("identity-hub"),
        PinnedTile {
            group: Some(group_id("platform")),
            represents: iri("https://example.org/catalogue/identity-hub"),
        },
    );

    // Row -1 and column -2 are ordinary, not exotic: the origin is wherever the first tile landed.
    // A fixture that stays in the positive quadrant certifies an implementation that loses cells.
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("vault"), Cell { col: 0, row: 0 });
    cells.insert(tile_id("identity-hub"), Cell { col: -2, row: -1 });

    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("platform"),
        Group {
            label: "Platform".into(),
            style_key: Some(iri("https://example.org/style/ground-shared")),
            note: None,
            extra: Vec::new(),
        },
    );

    Diagram::try_new(DiagramSpec {
        slug: slug("site-layout"),
        label: "Site layout".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        // Set here AND on the WriteOpts below, to the same value, so that no test in this file
        // silently depends on which of the two the writer reads. That choice belongs to the
        // writer's own tests; what is asserted here is only that the mode survives.
        generator: None,
        generated_at: Some(when()),
        groups,
        content: Content::Pinned {
            source: iri(SOURCE),
            revision: Some(REVISION.to_string()),
            tiles,
        },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .unwrap_or_else(|e| {
        panic!("the pinned fixture was refused by the model ({e:?}), so every test below would be testing the fixture rather than the mode")
    })
}

/// A self-contained drawing: the only place its text exists is the document itself.
fn standalone() -> Diagram {
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("sketch-left"),
        OwnTile {
            group: None,
            label: "Sketch left".into(),
            comment: Some("The half nothing regenerates.".into()),
            style_key: Some(iri("https://example.org/style/ground-shared")),
            extra: Vec::new(),
        },
    );
    tiles.insert(
        tile_id("sketch-right"),
        OwnTile {
            group: None,
            label: "Sketch right".into(),
            comment: None,
            style_key: None,
            extra: Vec::new(),
        },
    );

    let mut cells = BTreeMap::new();
    cells.insert(tile_id("sketch-left"), Cell { col: 0, row: 0 });
    cells.insert(tile_id("sketch-right"), Cell { col: 1, row: -1 });

    Diagram::try_new(DiagramSpec {
        slug: slug("notes-sketch"),
        label: "Notes sketch".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: Some(when()),
        groups: BTreeMap::new(),
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .unwrap_or_else(|e| {
        panic!("the standalone fixture was refused by the model ({e:?}), so every test below would be testing the fixture rather than the mode")
    })
}

fn opts(prefix: &str, subject_ns: &str) -> WriteOpts {
    WriteOpts::new(BASE, prefix, subject_ns)
        .unwrap_or_else(|e| panic!("the fixture write options were refused ({e:?})"))
        .with_generated_at(when())
}

/// Serialise, parse, serialise. Returns the parsed diagram as well as both strings, because bytes
/// that differ and a diagram that came back in the wrong mode are different bugs with one symptom.
fn write_read_write(d: &Diagram, o: &WriteOpts) -> (String, Diagram, String) {
    let once = write_turtle(d, o);
    assert!(
        !once.is_empty(),
        "the writer produced no bytes at all, so the byte-identity assertion below would compare an empty string to an empty string and report success while nothing could be exported"
    );

    let reread = read_turtle(&once, &read_opts()).unwrap_or_else(|e| {
        panic!("the reader refused the writer's own output ({e:?}); an editor that cannot load the file it has just written loses every arrangement the moment the page reloads")
    });

    let twice = write_turtle(&reread, o);
    (once, reread, twice)
}

#[test]
fn a_pinned_placement_has_nowhere_to_put_a_label_and_a_standalone_tile_does() {
    let mut layout = pinned();

    let ids: Vec<TileId> = layout.content().ids().cloned().collect();
    assert!(
        !ids.is_empty(),
        "the pinned fixture reports no tiles, so the loop below would inspect nothing and this test would pass over an empty diagram"
    );

    for id in &ids {
        assert!(
            layout.own_tile_mut(id).is_none(),
            "editable content was handed out for {id:?} in a pinned diagram; the moment a user can type there the file caches a label that its source will change without telling anyone, and nothing between the keystroke and the next reader ever invalidates it"
        );
    }

    // The same call in the other mode. Without this half, the loop above would also pass against a
    // method that returns None to everybody, which would take the editor's only content verb away
    // from the one mode that is supposed to have it.
    let mut drawing = standalone();
    let edited = tile_id("sketch-left");
    let held = drawing.own_tile_mut(&edited).unwrap_or_else(|| {
        panic!("standalone content could not be edited, which leaves the editor unable to change the only text a self-contained drawing has")
    });
    held.label = "Renamed".into();

    let stored = match drawing.content() {
        Content::Standalone { tiles } => tiles.get(&edited).map(|t| t.label.clone()),
        Content::Pinned { .. } => panic!(
            "the standalone fixture reports pinned content, so the mode and the content it is read off have already disagreed"
        ),
    };
    assert_eq!(
        stored.as_ref().map(Text::as_str),
        Some("Renamed"),
        "the edit did not reach the stored tile, so a rename made in the editor is gone by the time the document is exported"
    );
}

#[test]
fn the_mode_is_read_off_the_content_and_cannot_disagree_with_it() {
    let layout = pinned();
    assert_eq!(
        layout.mode(),
        Mode::Pinned,
        "a diagram holding pinned content reports the other mode, so a loader would enforce standalone rules over it and let a user type a label into a layout"
    );
    assert!(
        matches!(layout.content(), Content::Pinned { .. }),
        "the pinned fixture's content is not Pinned, so the mode above is being stored beside the content rather than read off it — and two copies of one fact disagree the first time only one is updated"
    );
    assert_eq!(
        layout.source(),
        Some(&iri(SOURCE)),
        "the pinned diagram does not name the source it lays out; loaded against the wrong source it drops every placement whose subject is absent there and still looks like a diagram, one cell short and silent about which cell"
    );
    assert_eq!(
        layout.revision(),
        Some(REVISION),
        "the pinned revision was lost, and it is the only thing that can tell a reader this layout predates the data it lays out"
    );

    let drawing = standalone();
    assert_eq!(
        drawing.mode(),
        Mode::Standalone,
        "a diagram holding its own tiles reports the other mode, so a loader would refuse to let a user edit the only content this document has"
    );
    assert!(
        drawing.source().is_none(),
        "a standalone diagram names an external source; nothing regenerates a standalone document, so a source named here is a promise no code anywhere will keep"
    );
    assert!(
        drawing.revision().is_none(),
        "a standalone diagram carries a pinned revision, which claims a drift that cannot happen and sends a reader looking for a source that does not exist"
    );
}

#[test]
fn a_pinned_document_whose_placement_caches_a_label_is_refused() {
    let clean = write_turtle(&pinned(), &opts("d", PINNED_NS));

    const ANCHOR: &str = "hive:represents";
    assert!(
        clean.contains(ANCHOR),
        "the writer emitted no {ANCHOR} for a pinned placement, so the injection below would change nothing and this test would be asserting the refusal of a document with no defect in it"
    );
    assert!(
        read_turtle(&clean, &read_opts()).is_ok(),
        "the reader refuses the clean pinned document, so refusing the mutated one proves nothing about the cached label"
    );

    // rdfs:label as a full IRI ref rather than a prefixed name: which prefixes the writer binds is
    // the writer's business, and this test must not go red the day it stops binding rdfs:.
    let cached = clean.replacen(
        ANCHOR,
        "<http://www.w3.org/2000/01/rdf-schema#label> \"Vault\" ;\n    hive:represents",
        1,
    );

    match read_turtle(&cached, &read_opts()) {
        Ok(loaded) => panic!(
            "a pinned placement carrying a label was accepted into {:?}; the label is content cached beside a layout with nothing anywhere to invalidate it, and the model has no field to hold it, so it was either dropped in silence or smuggled somewhere it will be written back out",
            loaded.slug()
        ),
        Err(ReadError::Syntax {
            line,
            col,
            found,
            expected,
        }) => panic!(
            "the cached label was reported as a syntax error at {line}:{col} (found {found:?}, expected {expected}); the document is well-formed Turtle, so the contributor sent there will go looking at their punctuation instead of deleting the predicate that is actually wrong"
        ),
        Err(_) => {}
    }
}

#[test]
fn a_standalone_document_that_points_outside_itself_is_refused() {
    let clean = write_turtle(&standalone(), &opts("s", STANDALONE_NS));
    assert!(
        read_turtle(&clean, &read_opts()).is_ok(),
        "the reader refuses the clean standalone document, so neither refusal below proves anything about pointing outside it"
    );

    // Leg one: a placement naming a subject instead of a tile. That the IRI it names happens to be
    // local is beside the point — the predicate is the claim, and hive:represents claims content is
    // read from somewhere this document does not contain.
    const TILE: &str = "hive:tile";
    assert!(
        clean.contains(TILE),
        "the writer emitted no {TILE} for a standalone placement, so the mutation below would change nothing and the refusal it asserts would be about an unmodified file"
    );
    let outward = clean.replacen(TILE, "hive:represents", 1);
    match read_turtle(&outward, &read_opts()) {
        Ok(loaded) => panic!(
            "a standalone diagram whose placement names an external subject was accepted into {:?}; the document now reads as regenerable while nothing will ever regenerate it, so the cell it draws is frozen and nothing on the page says which cell that is",
            loaded.slug()
        ),
        Err(ReadError::ModeDisagreesWithContent {
            declared, subject, ..
        }) => {
            assert_eq!(
                declared,
                Mode::Standalone,
                "the refusal names the wrong declared mode, which sends whoever reads it to convert the half of the file that is already correct"
            );
            assert!(
                !subject.is_empty(),
                "the refusal names no subject; in a document of forty placements, 'a placement points outside' is not something anybody can act on"
            );
        }
        Err(other) => panic!(
            "a standalone placement naming an external subject was refused as {other:?} rather than as a disagreement between the declared mode and the content, so the message a contributor is shown will not be about the mode at all"
        ),
    }

    // Leg two: the diagram itself naming a source. The assertion is deliberately weaker than leg
    // one's — the designed error set names one variant for a placement disagreeing with its
    // diagram, and whether a diagram-level hive:pinnedTo reports as that same variant is a choice
    // this test does not own. What it does own is that the document must not load.
    const PLACEMENT: &str = "hive:placement";
    assert!(
        clean.contains(PLACEMENT),
        "the writer emitted no {PLACEMENT}, so the mutation below would change nothing and the refusal it asserts would be about an unmodified file"
    );
    let sourced = clean.replacen(
        PLACEMENT,
        &format!("hive:pinnedTo <{SOURCE}> ;\n    hive:placement"),
        1,
    );
    match read_turtle(&sourced, &read_opts()) {
        Ok(loaded) => panic!(
            "a standalone diagram naming an external source was accepted into {:?}; a reader will be told this drawing tracks something, and will go looking for the drift between them in a document nothing has ever regenerated",
            loaded.slug()
        ),
        Err(ReadError::Syntax { line, col, .. }) => panic!(
            "hive:pinnedTo on a standalone diagram was reported as a syntax error at {line}:{col}; the document parses, and the contributor sent there will not find the predicate that has to go"
        ),
        Err(_) => {}
    }
}

#[test]
fn a_diagram_declaring_both_modes_is_refused() {
    let clean = write_turtle(&standalone(), &opts("s", STANDALONE_NS));

    const MODE: &str = "hive:standalone";
    assert!(
        clean.contains(MODE),
        "the writer emitted no {MODE}, so the second mode could not be added below and this test would assert the refusal of a file that declares one mode"
    );
    assert!(
        read_turtle(&clean, &read_opts()).is_ok(),
        "the reader refuses the clean one-mode document, so refusing the two-mode one proves nothing about the second mode"
    );

    let both = clean.replacen(MODE, "hive:standalone , hive:pinned", 1);

    match read_turtle(&both, &read_opts()) {
        Ok(loaded) => panic!(
            "a diagram declaring both hive:pinned and hive:standalone was accepted into {:?}, reporting mode {:?}; one of the two values was picked by statement order, so the same bytes open as a layout in one session and as a drawing in the next, and the editor lets a user type a label into whichever of them it guessed",
            loaded.slug(),
            loaded.mode()
        ),
        Err(ReadError::Syntax { line, col, .. }) => panic!(
            "two hive:mode values were reported as a syntax error at {line}:{col}; the document parses, and a contributor told to check their punctuation will never find the second mode"
        ),
        Err(_) => {}
    }
}

#[test]
fn a_pinned_diagram_survives_write_read_write_byte_for_byte() {
    let original = pinned();
    let o = opts("d", PINNED_NS);
    let (once, reread, twice) = write_read_write(&original, &o);

    assert!(
        once.contains("site-layout") && once.contains("vault") && once.contains("identity-hub"),
        "the exported bytes name neither the diagram nor its tiles, so the byte comparison below would be comparing two documents that are equally empty of content"
    );
    assert!(
        once.contains(SOURCE),
        "the exported pinned bytes do not name the source they lay out; a layout of an unnamed source is a layout of nothing in particular, and loading it against the wrong one drops placements in silence"
    );
    assert!(
        once.contains(REVISION),
        "the exported pinned bytes carry no revision, so no reader can ever be told that this layout predates the data it draws"
    );

    assert_eq!(
        reread.mode(),
        Mode::Pinned,
        "a pinned document came back in the other mode, which is an editor that will let a user type content into a layout that must never carry any"
    );
    assert_eq!(
        reread.source(),
        Some(&iri(SOURCE)),
        "the source survived the write but not the read, so the reloaded layout lays out nothing in particular"
    );
    assert_eq!(
        reread.revision(),
        Some(REVISION),
        "the pinned revision was lost on the way back in, and drift silently becomes undetectable rather than absent"
    );
    assert_eq!(
        reread.cell_of(&tile_id("identity-hub")),
        Some(Cell { col: -2, row: -1 }),
        "a placement at a negative column and an odd negative row did not come back to its own cell; an implementation that indexes from a corner loses exactly these and says nothing"
    );
    assert_eq!(
        reread
            .group(&group_id("platform"))
            .map(|g| g.label.as_str()),
        Some("Platform"),
        "the group's label did not survive a pinned round trip; the region is still drawn, so the reader gets a coloured blob that asserts a relationship and never names it"
    );

    assert_eq!(
        once, twice,
        "a pinned diagram serialises differently after a round trip, so every export rewrites lines nothing changed — and a diagram whose git history is all churn is a diagram nobody can review"
    );
}

#[test]
fn a_standalone_diagram_survives_write_read_write_byte_for_byte() {
    let original = standalone();
    let o = opts("s", STANDALONE_NS);
    let (once, reread, twice) = write_read_write(&original, &o);

    assert!(
        once.contains("Sketch left") && once.contains("Sketch right"),
        "the exported standalone bytes do not carry their own tile labels, and nothing outside the document is read, so the file draws two empty hexagons that still occupy their cells"
    );

    assert_eq!(
        reread.mode(),
        Mode::Standalone,
        "a standalone document came back in the other mode, which is an editor that refuses to edit the only content this document has"
    );
    assert!(
        reread.source().is_none(),
        "the round trip gave a standalone diagram a source; it now claims to track something that will never regenerate it"
    );

    let left = match reread.content() {
        Content::Standalone { tiles } => tiles.get(&tile_id("sketch-left")).cloned(),
        Content::Pinned { .. } => panic!(
            "a standalone document was read back as pinned content, so its tiles were dropped and every cell now points at an external subject this document never named"
        ),
    };
    let left = left.unwrap_or_else(|| {
        panic!(
            "a tile disappeared across the round trip, and with it the only text its hexagon had"
        )
    });
    assert_eq!(
        left.label.as_str(),
        "Sketch left",
        "a standalone tile's label changed across a round trip, which is the one piece of content this mode exists to hold"
    );
    assert_eq!(
        left.comment.as_ref().map(Text::as_str),
        Some("The half nothing regenerates."),
        "the tile's comment was dropped on the way back in; a round trip that loses what it does not display is how a host's content disappears one save at a time"
    );
    assert_eq!(
        left.style_key,
        Some(iri("https://example.org/style/ground-shared")),
        "the opaque style key was lost, so the tile now draws as the host's default and asserts a claim about the diagram that nobody made"
    );

    assert_eq!(
        once, twice,
        "a standalone diagram serialises differently after a round trip, so every export rewrites lines nothing changed and the diff stops being reviewable"
    );
}

#[test]
fn one_document_may_hold_one_diagram_of_each_mode() {
    // Mixing the modes WITHIN a diagram is refused above; mixing them between diagrams in one file
    // is ordinary and must stay so. A consumer's shapes file needs a focus node for both modes, and
    // a shape that selects nothing is a rule nobody is enforcing — so a reader that refused this
    // file would quietly disarm half of the contract this vocabulary ships with.
    //
    // Re-declaring @base and @prefix mid-document is legal Turtle, and both halves declare the same
    // values here, so the concatenation below means exactly what the two halves mean apart.
    let layout = write_turtle(&pinned(), &opts("d", PINNED_NS));
    let drawing = write_turtle(&standalone(), &opts("s", STANDALONE_NS));
    let corpus = format!("{layout}\n{drawing}");

    let all = read_turtle_all(&corpus, &read_opts()).unwrap_or_else(|e| {
        panic!("a file holding one diagram of each mode was refused ({e:?}); a corpus ships both seeds in one file precisely so that every shape has something to check, and refusing it leaves half of them inert")
    });
    assert_eq!(
        all.len(),
        2,
        "reading a two-diagram file returned {} diagram(s); a reader that stops at the first one drops the second silently, and the file still looks like it loaded",
        all.len()
    );
    let modes: Vec<Mode> = all.iter().map(|d| d.mode()).collect();
    assert!(
        modes.contains(&Mode::Pinned) && modes.contains(&Mode::Standalone),
        "the two diagrams came back as {modes:?} rather than one of each, so one of them was read under the other's rules"
    );

    match read_turtle(&corpus, &read_opts()) {
        Err(ReadError::AmbiguousDocument { slugs }) => {
            assert!(
                slugs.contains(&slug("site-layout")) && slugs.contains(&slug("notes-sketch")),
                "the ambiguity error lists {slugs:?} and not both diagrams, so the caller is not told which names it may ask for and has to guess a second time"
            );
        }
        Err(other) => panic!(
            "asking for one unnamed diagram out of two was refused as {other:?}; the caller needs to be told that the document holds several and what they are called, not that something else went wrong"
        ),
        Ok(picked) => panic!(
            "read_turtle silently picked {:?} out of a two-diagram document; whichever one it guessed, the other is now invisible to a caller who never learns it exists",
            picked.slug()
        ),
    }

    let named = read_turtle(
        &corpus,
        &ReadOpts {
            base: None,
            slug: Some(slug("site-layout")),
        },
    )
    .unwrap_or_else(|e| {
        panic!("naming one of the two diagrams did not resolve the ambiguity ({e:?}), which leaves a multi-diagram file unreadable by any route")
    });
    assert_eq!(
        named.mode(),
        Mode::Pinned,
        "asking for the pinned diagram by slug returned the standalone one, so a host loading a layout gets a drawing and enforces the wrong rules over it"
    );
}

/// A GROUP NOBODY IS IN, THROUGH THE WHOLE PIPELINE.
///
/// This is the state a host ships when it wants a group to be JOINABLE before
/// anything is in it — which is what a palette needs: `Command::Add` refuses a
/// group the diagram has not declared, so a host that only declares groups with
/// members can never offer the first tile of one.
///
/// Nothing in the vocabulary or the shapes requires a group to have members —
/// `hive:Group` says its geometry is derived from its members' cells and says
/// nothing about there being any — but "nothing forbids it" and "the writer, the
/// reader and the byte-identity all survive it" are different claims, and only
/// the second one is worth relying on.
#[test]
fn a_declared_group_with_no_members_survives_write_read_write() {
    // Built rather than mutated: `Diagram` has no way to declare a group after
    // construction, which is itself the reason a host must declare its joinable
    // groups up front.
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("vault"),
        PinnedTile {
            group: Some(group_id("platform")),
            represents: iri("https://example.org/catalogue/vault"),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("vault"), Cell { col: 0, row: 0 });

    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("platform"),
        Group {
            label: "Platform".into(),
            style_key: Some(iri("https://example.org/style/ground-shared")),
            note: None,
            extra: Vec::new(),
        },
    );
    // Both kinds: one carrying a style key, one bare, because the writer emits
    // an optional property and a group with none is the shorter branch.
    for (key, style) in [
        (
            "empty-quarter",
            Some(iri("https://example.org/style/ground-shared")),
        ),
        ("bare-quarter", None),
    ] {
        groups.insert(
            group_id(key),
            Group {
                label: format!("The {key}").into(),
                style_key: style,
                note: None,
                extra: Vec::new(),
            },
        );
    }

    let original = Diagram::try_new(DiagramSpec {
        slug: slug("site-layout"),
        label: "Site layout".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: Some(when()),
        groups,
        content: Content::Pinned {
            source: iri(SOURCE),
            revision: Some(REVISION.to_string()),
            tiles,
        },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .expect("a diagram may declare a group nothing is in");
    let before = original.groups().len();
    assert_eq!(
        before, 3,
        "the fixture must actually carry the empty groups"
    );

    let o = opts("d", PINNED_NS);
    let (once, reread, twice) = write_read_write(&original, &o);

    assert!(
        once.contains("empty-quarter") && once.contains("bare-quarter"),
        "the writer dropped a group with no members, so a host cannot ship a joinable empty group at all: {once}"
    );
    assert_eq!(
        reread.groups().len(),
        before,
        "the reader lost a group with no members; a palette offering it would be refused with UnknownGroup on a document that declares it"
    );
    for key in ["empty-quarter", "bare-quarter"] {
        assert!(
            reread.has_group(&group_id(key)),
            "{key} did not survive the round trip, so Command::Add into it would be refused"
        );
        assert!(
            reread.members(&group_id(key)).is_empty(),
            "{key} came back with members it never had"
        );
    }
    assert_eq!(
        once, twice,
        "an empty group is not stable across a round trip, so the file a reviewer sees changes every time it is re-exported"
    );
}

/// A `Diagram` THAT EXISTS SERIALISES TO A DOCUMENT THAT VALIDATES, which is the
/// promise `model.rs`'s header makes and did not keep.
///
/// The writer mints a placement's subject as `at-{slug}`. Nothing checked that
/// against the other locals, so a tile called `at-hall` and a tile called `hall`
/// produced `d:at-hall` TWICE — once as a `hive:Tile` with a label and a slug,
/// once as a `hive:Placement`, which is the one closed shape in the contract.
/// The result is refused by this crate's own reader and by SHACL, from a diagram
/// `try_new` had accepted.
#[test]
fn two_subjects_that_would_share_one_iri_are_refused_at_construction() {
    // (a) A tile whose slug collides with another tile's placement subject.
    let mut tiles = BTreeMap::new();
    let mut cells = BTreeMap::new();
    for (name, col) in [("hall", 0), ("at-hall", 1)] {
        tiles.insert(
            tile_id(name),
            OwnTile {
                group: None,
                label: format!("Label {name}").into(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
        cells.insert(tile_id(name), Cell { col, row: 0 });
    }
    let spec = DiagramSpec {
        slug: slug("sheet"),
        label: "Sheet".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups: BTreeMap::new(),
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    };
    match Diagram::try_new(spec) {
        Err(honeycomb_core::ModelError::SubjectCollision { local, .. }) => {
            assert_eq!(local, "at-hall")
        }
        other => panic!(
            "a tile colliding with another tile's placement subject was accepted, so the \
             writer would emit d:at-hall twice and the reader would refuse its own output: \
             {other:?}"
        ),
    }

    // (b) A tile and a group with one slug, in standalone mode.
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("civic"),
        OwnTile {
            group: Some(group_id("civic")),
            label: "Civic".into(),
            comment: None,
            style_key: None,
            extra: Vec::new(),
        },
    );
    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("civic"),
        Group {
            label: "Civic Quarter".into(),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("civic"), Cell { col: 0, row: 0 });
    assert!(
        matches!(
            Diagram::try_new(DiagramSpec {
                slug: slug("sheet"),
                label: "Sheet".into(),
                note: None,
                convention: LatticeConvention::OddRPointyTop,
                generator: None,
                generated_at: None,
                groups,
                content: Content::Standalone { tiles },
                cells,
                extra: Vec::new(),
                links: BTreeMap::new(),
            }),
            Err(honeycomb_core::ModelError::SubjectCollision { .. })
        ),
        "a tile and a group with one slug share a subject and produce two rdfs:labels on it"
    );

    // (c) The SAME pair is legal in PINNED mode, and must stay legal: a pinned
    // placement names no tile subject at all, so there is nothing to collide.
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("civic"),
        PinnedTile {
            group: Some(group_id("civic")),
            represents: iri("https://example.org/catalogue/civic"),
        },
    );
    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("civic"),
        Group {
            label: "Civic Quarter".into(),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("civic"), Cell { col: 0, row: 0 });
    assert!(
        Diagram::try_new(DiagramSpec {
            slug: slug("sheet"),
            label: "Sheet".into(),
            note: None,
            convention: LatticeConvention::OddRPointyTop,
            generator: None,
            generated_at: None,
            groups,
            content: Content::Pinned {
                source: iri(SOURCE),
                revision: None,
                tiles,
            },
            cells,
            extra: Vec::new(),
            links: BTreeMap::new(),
        })
        .is_ok(),
        "a pinned diagram has no tile subjects, so a tile and a group may share a slug"
    );
}

/// The writer escapes what an `IRIREF` may not contain, so its output reads back.
#[test]
fn an_iri_that_would_end_its_own_token_is_escaped() {
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("sketch"),
        OwnTile {
            group: None,
            label: "Sketch".into(),
            comment: None,
            // `>` ends an IRIREF; a space is legal for this reader and rejected
            // by a strict one. Both used to be written raw.
            style_key: Some(iri("https://example.org/style#a>b c")),
            extra: Vec::new(),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("sketch"), Cell { col: 0, row: 0 });
    let d = Diagram::try_new(DiagramSpec {
        slug: slug("sheet"),
        label: "Sheet".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups: BTreeMap::new(),
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .unwrap();

    let o = opts("d", "https://example.org/d/");
    let (once, reread, twice) = write_read_write(&d, &o);
    assert!(
        !once.contains("#a>b"),
        "the writer emitted a raw `>` inside an IRI ref, which ends the token: {once}"
    );
    let back = match reread.content() {
        Content::Standalone { tiles } => tiles[&tile_id("sketch")].style_key.clone(),
        Content::Pinned { .. } => unreachable!(),
    };
    assert_eq!(
        back,
        Some(iri("https://example.org/style#a>b c")),
        "the escaped IRI did not round-trip to the value it started as"
    );
    assert_eq!(once, twice);
}

/// A HOST'S OWN PREDICATES ON THE DIAGRAM SUBJECT SURVIVE A ROUND TRIP.
///
/// `hsh:DiagramShape` is deliberately not closed, and says why: "a host hangs
/// its own predicates on a diagram". This crate read them and threw them away
/// for its whole first life — an open shape with a closed implementation, which
/// is exactly the defect `OwnTile::extra`'s doc exists to prevent one level
/// down. A host storing a sheet number on the diagram lost it the first time
/// anybody dragged a hexagon, with no error anywhere.
#[test]
fn a_hosts_own_predicates_on_the_diagram_subject_survive_a_round_trip() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .
@prefix d:    <https://example.org/d/> .
@prefix ex:   <https://example.org/planning#> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ;
  rdfs:label "Sheet" ;
  hive:mode hive:standalone ;
  hive:lattice hive:oddRPointyTop ;
  ex:owner "planning-office" ;
  ex:sheetNumber 4 ;
  hive:placement d:at-hall .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
"#;
    let d = read_turtle(src, &ReadOpts::default()).expect("the document parses");
    assert_eq!(
        d.extra().len(),
        2,
        "the host's two predicates were dropped on the way in: {:?}",
        d.extra()
    );

    let o = opts("d", "https://example.org/d/")
        .with_prefix("ex", "https://example.org/planning#")
        .unwrap();
    let once = write_turtle(&d, &o);
    assert!(
        once.contains("planning-office"),
        "the host's value was dropped on the way out: {once}"
    );
    let reread = read_turtle(&once, &ReadOpts::default()).expect("it reads back");
    assert_eq!(reread.extra(), d.extra(), "the predicates changed shape");
    assert_eq!(
        write_turtle(&reread, &o),
        once,
        "a document carrying host predicates is not a fixed point"
    );
}

/// AN UNNAMED GROUP IS OMITTED, NOT WRITTEN EMPTY — and it validates.
///
/// `rdfs:label ""` claims the name IS the empty string; no predicate at all says
/// there is none, and a reader cannot tell those apart from the value. The shape
/// says the same: `sh:maxCount 1` with `sh:minLength 1`, and no `sh:minCount`.
#[test]
fn a_group_with_no_label_omits_the_predicate_and_reads_back_unnamed() {
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("vault"),
        PinnedTile {
            group: Some(group_id("quiet")),
            represents: iri("https://example.org/catalogue/vault"),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("vault"), Cell { col: 0, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("quiet"),
        Group {
            label: Text::default(),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    let original = Diagram::try_new(DiagramSpec {
        slug: slug("site-layout"),
        label: "Site layout".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: Some(when()),
        groups,
        content: Content::Pinned {
            source: iri(SOURCE),
            revision: None,
            tiles,
        },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .expect("a group may have no label");

    let o = opts("d", PINNED_NS);
    let (once, reread, twice) = write_read_write(&original, &o);
    assert!(
        !once.contains(r#"rdfs:label "" "#),
        "the writer emitted an empty label instead of omitting it: {once}"
    );
    assert!(
        reread.has_group(&group_id("quiet")),
        "the unnamed group did not survive the round trip"
    );
    assert_eq!(
        reread.group(&group_id("quiet")).map(|g| g.label.as_str()),
        Some(""),
        "an absent label must read back as no name"
    );
    assert_eq!(once, twice, "an unnamed group is not a fixed point");
}

/// LINKS SURVIVE WRITE, READ AND WRITE AGAIN — all three routings, a label, and
/// a host's own predicate on a link.
///
/// The ends are PLACEMENT subjects in the file and TILE ids in the model, so
/// every link crosses that indirection twice on a round trip. It is the kind of
/// mapping that works for the default case and loses the others.
#[test]
fn links_survive_write_read_write_with_every_routing() {
    use honeycomb_core::{Link, LinkId, Routing};

    let mut tiles = BTreeMap::new();
    let mut cells = BTreeMap::new();
    for (i, name) in ["vault", "identity-hub", "catalogue"].iter().enumerate() {
        tiles.insert(
            tile_id(name),
            PinnedTile {
                group: None,
                represents: iri(&format!("https://example.org/catalogue/{name}")),
            },
        );
        cells.insert(
            tile_id(name),
            Cell {
                col: i as i32,
                row: 0,
            },
        );
    }
    let mut links = BTreeMap::new();
    for (id, from, to, routing, label) in [
        ("plain", "vault", "identity-hub", Routing::Straight, None),
        (
            "routed",
            "identity-hub",
            "catalogue",
            Routing::LatticePath,
            Some("registers offers"),
        ),
        ("bowed", "vault", "catalogue", Routing::Arc, None),
    ] {
        links.insert(
            LinkId(slug(id)),
            Link {
                from: tile_id(from),
                to: tile_id(to),
                label: label.map(Text::plain),
                routing,
                style_key: Some(iri("https://example.org/style/flow")),
                extra: vec![honeycomb_core::Statement {
                    predicate: iri("https://example.org/planning#protocol"),
                    object: honeycomb_core::Term::Literal {
                        value: "https".into(),
                        datatype: None,
                        lang: None,
                    },
                }],
            },
        );
    }

    let original = Diagram::try_new(DiagramSpec {
        slug: slug("site-layout"),
        label: "Site layout".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: Some(when()),
        groups: BTreeMap::new(),
        content: Content::Pinned {
            source: iri(SOURCE),
            revision: None,
            tiles,
        },
        cells,
        links,
        extra: Vec::new(),
    })
    .expect("three placed tiles and three links between them");

    let o = opts("d", PINNED_NS)
        .with_prefix("ex", "https://example.org/planning#")
        .unwrap();
    let (once, reread, twice) = write_read_write(&original, &o);

    // The ends are placement subjects, not tile subjects.
    assert!(
        once.contains("hive:from d:at-vault"),
        "a link's end was not written as a placement subject: {once}"
    );
    // ABSENT MEANS STRAIGHT, so the default costs no statement.
    assert!(
        !once.contains("hive:straight"),
        "the writer said the default out loud: {once}"
    );
    assert!(once.contains("hive:latticePath") && once.contains("hive:arc"));

    assert_eq!(reread.links().len(), 3, "a link was lost on the way back");
    for (id, want) in original.links() {
        let got = reread.link(id).unwrap_or_else(|| panic!("{id:?} is gone"));
        assert_eq!(got, want, "{id:?} changed shape on the round trip");
    }
    assert_eq!(once, twice, "a diagram with links is not a fixed point");
}

/// A document whose link reaches a tile the diagram does not place is refused,
/// rather than read into a diagram that draws a line to nowhere.
///
/// THE REFUSAL MOVED, AND THE OLD ONE IS WHY. This used to expect
/// `ModelError::LinkToNowhere { end: TileId("missing") }` — and `missing` is a
/// name that appears NOWHERE in the document below. The reader had taken
/// `d:at-missing`, stripped the `at-`, and invented a tile identity out of the
/// remainder; the refusal that followed came from `Diagram::try_new` noticing
/// that the invented tile was not placed. Right answer, wrong reason, and the
/// reason is what stops being safe the moment an end is allowed to name
/// something that is not a placement. `ReadError::UnresolvedLinkEnd` is the
/// same refusal made at the point of resolution, naming the IRI the document
/// actually contains instead of a tile it does not. See
/// `honeycomb-core/tests/link_ends.rs` for the silent-rewrite case the same
/// guess produced.
#[test]
fn a_link_to_an_absent_placement_is_refused_on_the_way_in() {
    let src = r#"
@prefix hive: <https://semantic.ds-labs.org/vocab/honeycomb#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix d:    <https://example.org/d/> .

d:sheet a hive:Diagram ;
  hive:slug "sheet" ; rdfs:label "Sheet" ;
  hive:mode hive:standalone ; hive:lattice hive:oddRPointyTop ;
  hive:placement d:at-hall ; hive:link d:nowhere .

d:hall a hive:Tile ; hive:slug "hall" ; rdfs:label "Hall" .
d:at-hall a hive:Placement ; hive:tile d:hall ; hive:col 0 ; hive:row 0 .
d:nowhere a hive:Link ; hive:slug "nowhere" ;
  hive:from d:at-hall ; hive:to d:at-missing .
"#;
    match read_turtle(src, &ReadOpts::default()) {
        Err(ReadError::UnresolvedLinkEnd {
            link,
            predicate,
            iri,
        }) => {
            assert!(link.ends_with("nowhere"), "{link}");
            assert_eq!(predicate, "hive:to");
            assert_eq!(iri, "https://example.org/d/at-missing");
        }
        other => panic!("a link to nowhere was accepted: {other:?}"),
    }
}

/// VERIFICATION, NOT A FIX — checked here rather than assumed because the
/// decision that a group's note must be DRAWN (the component and its demo
/// host, not this crate) starts from the premise that this crate's own
/// writer and reader already carry the value correctly, and that premise had
/// never actually been pinned by a test. `write_turtle` already emits
/// `hive:note` for a group exactly the way it does for a diagram (see the
/// `if let Some(n) = &g.note` beside `if let Some(n) = d.note()`), and
/// `read.rs`'s `g_note` already parses it back — this test exists to make
/// that a checked fact instead of something a reader of this crate's source
/// has to take on faith, and to catch it going quiet again if it ever does.
#[test]
fn a_groups_note_survives_write_read_write() {
    let mut tiles = BTreeMap::new();
    tiles.insert(
        tile_id("vault"),
        PinnedTile {
            group: Some(group_id("platform")),
            represents: iri("https://example.org/catalogue/vault"),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tile_id("vault"), Cell { col: 0, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        group_id("platform"),
        Group {
            label: "Platform".into(),
            style_key: None,
            note: Some("deployed once per participant — ×7 here".to_string()),
            extra: Vec::new(),
        },
    );
    let original = Diagram::try_new(DiagramSpec {
        slug: slug("site-layout"),
        label: "Site layout".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: Some(when()),
        groups,
        content: Content::Pinned {
            source: iri(SOURCE),
            revision: None,
            tiles,
        },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .expect("a group may carry a note");

    let o = opts("d", PINNED_NS);
    let (once, reread, twice) = write_read_write(&original, &o);
    assert!(
        once.contains("deployed once per participant"),
        "the writer dropped a group's note: {once}"
    );
    assert_eq!(
        reread
            .group(&group_id("platform"))
            .and_then(|g| g.note.as_deref()),
        Some("deployed once per participant — ×7 here"),
        "the group's note did not survive the round trip — everything downstream of the model, \
         including the component that is supposed to draw it, would be drawing nothing"
    );
    assert_eq!(once, twice, "a group's note is not a fixed point");
}
