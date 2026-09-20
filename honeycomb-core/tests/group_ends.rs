//! A link's endpoint may be a GROUP, and everything the document layer owes
//! once it may.
//!
//! WHAT BREAKS WITHOUT THIS FILE. `Link` used to hold two `TileId`s, so "the
//! platform talks to the catalogue" could only be drawn as a line between two
//! arbitrarily chosen hexagons — one member standing in for a whole region,
//! with nothing in the document saying it was standing in for anything. The
//! widening is small in the type and large everywhere else: the reader has to
//! tell a placement IRI from a group IRI, the writer has to stop minting `at-`
//! for one of them, and `Diagram::try_new` gains two refusals that did not
//! exist because the state they refuse was not expressible.
//!
//! THE ONE PROPERTY EVERY ASSERTION HERE SERVES: the board never reaches a
//! state where a link exists that cannot be drawn. That is why an EMPTY group
//! may not be an endpoint — `hsh:LinkShape` already argues, in its own
//! `sh:message`, that a line the renderer has no cell to draw from is
//! indistinguishable from a link that was never there — and why a group may not
//! link to one of its own members, which containment already says and a line
//! cannot add to.
//!
//! THE ANCHOR IS CHECKED HERE AND NOT IN THE VIEW. Which cell a line meets a
//! group at is a fact about the document, in the way `components` and `holes`
//! are: two hosts drawing one file must meet the region at the same hexagon or
//! they are drawing different diagrams. The painting is somebody else's.

use std::collections::{BTreeMap, BTreeSet};

use honeycomb_core::{
    Cell, Command, Content, Diagram, DiagramSpec, Endpoint, Group, GroupId, LatticeConvention,
    Link, LinkId, ModelError, OwnTile, ReadOpts, Routing, Slug, Text, TileId, WriteOpts, anchors,
    read_turtle, write_turtle,
};

const BASE: &str = "https://example.org/honeycomb/v0-4-0/";

fn slug(s: &str) -> Slug {
    Slug::parse(s).unwrap_or_else(|e| panic!("the fixture slug {s:?} is not kebab-case ({e:?})"))
}

fn tid(s: &str) -> TileId {
    TileId(slug(s))
}

fn gid(s: &str) -> GroupId {
    GroupId(slug(s))
}

fn lid(s: &str) -> LinkId {
    LinkId(slug(s))
}

fn cell(col: i32, row: i32) -> Cell {
    Cell { col, row }
}

fn own(label: &str, group: Option<&str>) -> OwnTile {
    OwnTile {
        group: group.map(gid),
        label: Text::plain(label),
        comment: None,
        style_key: None,
        extra: Vec::new(),
    }
}

fn group(label: &str) -> Group {
    Group {
        label: Text::plain(label),
        style_key: None,
        note: None,
        extra: Vec::new(),
    }
}

fn link(from: Endpoint, to: Endpoint) -> Link {
    Link {
        from,
        to,
        label: None,
        routing: Routing::Straight,
        style_key: None,
        extra: Vec::new(),
    }
}

fn opts() -> WriteOpts {
    WriteOpts::new(BASE, "d", BASE).expect("well-formed write options")
}

fn fixture() -> String {
    let path = format!(
        "{}/fixtures/group-ended-links.ttl",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("group-ended-links.ttl could not be read at {path}: {e}"))
}

/// Four tiles, two groups, and whichever links the caller asks for.
///
/// `north` holds hall and library, `south` holds depot alone, and annex is in
/// no group — which is the smallest arrangement in which all three of the new
/// combinations have somewhere to happen.
fn quarter(links: BTreeMap<LinkId, Link>) -> Result<Diagram, ModelError> {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    tiles.insert(tid("hall"), own("Town Hall", Some("north")));
    tiles.insert(tid("library"), own("Library", Some("north")));
    tiles.insert(tid("annex"), own("Annex", None));
    tiles.insert(tid("depot"), own("Depot", Some("south")));

    let mut cells: BTreeMap<TileId, Cell> = BTreeMap::new();
    cells.insert(tid("hall"), cell(0, 0));
    cells.insert(tid("library"), cell(1, 0));
    cells.insert(tid("annex"), cell(4, 0));
    cells.insert(tid("depot"), cell(3, 3));

    let mut groups: BTreeMap<GroupId, Group> = BTreeMap::new();
    groups.insert(gid("north"), group("North side"));
    groups.insert(gid("south"), group("South side"));

    Diagram::try_new(DiagramSpec {
        slug: slug("quarter"),
        label: Text::plain("Quarter"),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups,
        content: Content::Standalone { tiles },
        cells,
        links,
        extra: Vec::new(),
    })
}

fn one(id: &str, l: Link) -> BTreeMap<LinkId, Link> {
    let mut m = BTreeMap::new();
    m.insert(lid(id), l);
    m
}

// ------------------------------------------------------- the type itself

/// An `Endpoint` answers which kind it is WITHOUT the caller matching on it, so
/// a host asking "is this end a group?" does not have to import the variants to
/// find out — and, more to the point, cannot answer the question a second way
/// that disagrees with this one.
#[test]
fn an_endpoint_names_exactly_one_of_a_tile_and_a_group() {
    let t = Endpoint::Tile(tid("hall"));
    assert_eq!(t.tile(), Some(&tid("hall")));
    assert_eq!(t.group(), None);
    assert_eq!(t.slug().as_str(), "hall");

    let g = Endpoint::Group(gid("north"));
    assert_eq!(g.group(), Some(&gid("north")));
    assert_eq!(g.tile(), None);
    assert_eq!(g.slug().as_str(), "north");
}

// ----------------------------------------------------------- construction

/// All three new combinations construct. This is the feature, stated once at
/// the layer that is allowed to refuse things.
#[test]
fn a_group_may_be_either_end_of_a_link_or_both() {
    let mut links = BTreeMap::new();
    links.insert(
        lid("spine"),
        link(Endpoint::Group(gid("north")), Endpoint::Group(gid("south"))),
    );
    links.insert(
        lid("feeder"),
        link(Endpoint::Group(gid("north")), Endpoint::Tile(tid("annex"))),
    );
    links.insert(
        lid("spur"),
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    );
    let d = quarter(links).expect("group-to-group, group-to-cell and cell-to-group all draw");
    assert_eq!(d.links().len(), 3);
}

/// DECISION 2, AT CONSTRUCTION. A group with no members has no cell, so a line
/// to it is a line the renderer cannot draw — the exact argument `hsh:LinkShape`
/// already makes about a non-placement end, which stays true for an empty group
/// and stops being true for a populated one.
///
/// This is live rather than hypothetical: production declares 14 groups and
/// draws 8, so six of them are empty right now.
#[test]
fn a_link_to_a_group_with_no_members_is_refused_at_construction() {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    tiles.insert(tid("hall"), own("Town Hall", Some("north")));
    let mut cells: BTreeMap<TileId, Cell> = BTreeMap::new();
    cells.insert(tid("hall"), cell(0, 0));
    let mut groups: BTreeMap<GroupId, Group> = BTreeMap::new();
    groups.insert(gid("north"), group("North side"));
    // DECLARED AND EMPTY. Nothing in the vocabulary or the shapes requires a
    // group to have members, and `Command::Add` relies on that — a host
    // declares a group empty so that tiles have somewhere to be dropped into.
    groups.insert(gid("nobody"), group("Nobody"));

    let refused = Diagram::try_new(DiagramSpec {
        slug: slug("quarter"),
        label: Text::plain("Quarter"),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups,
        content: Content::Standalone { tiles },
        cells,
        links: one(
            "spine",
            link(Endpoint::Tile(tid("hall")), Endpoint::Group(gid("nobody"))),
        ),
        extra: Vec::new(),
    });

    match refused {
        Err(ModelError::LinkToEmptyGroup { link, group }) => {
            assert_eq!(link.0.as_str(), "spine");
            assert_eq!(group.0.as_str(), "nobody");
        }
        other => panic!(
            "a link to an empty group was accepted. Nothing can draw it, and a link nothing \
             draws is indistinguishable from a link that was never there: {other:?}"
        ),
    }
}

/// DECISION 3, AT CONSTRUCTION. Containment already says the member is in the
/// group; a line from a whole to its own part adds nothing to the drawing, and
/// it is the likeliest mis-drag there is — start on a ground, release on one of
/// its own hexagons.
#[test]
fn a_link_from_a_group_to_one_of_its_own_members_is_refused_at_construction() {
    let refused = quarter(one(
        "inward",
        link(Endpoint::Group(gid("north")), Endpoint::Tile(tid("hall"))),
    ));
    match refused {
        Err(ModelError::LinkToOwnMember { link, group, tile }) => {
            assert_eq!(link.0.as_str(), "inward");
            assert_eq!(group.0.as_str(), "north");
            assert_eq!(tile.0.as_str(), "hall");
        }
        other => panic!("a group was linked to its own member: {other:?}"),
    }
}

/// The same refusal with the ends the other way round. A link is directed, and
/// a rule that held in one direction only would be a rule half the mis-drags
/// walk straight past.
#[test]
fn a_link_from_a_member_to_its_own_group_is_refused_too() {
    let refused = quarter(one(
        "outward",
        link(
            Endpoint::Tile(tid("library")),
            Endpoint::Group(gid("north")),
        ),
    ));
    assert!(
        matches!(refused, Err(ModelError::LinkToOwnMember { .. })),
        "containment is not a direction: {refused:?}"
    );
}

/// A group end the diagram does not declare is the group-shaped half of
/// `LinkToNowhere`, and it reports the same way — the end, whichever kind it is.
#[test]
fn a_link_to_an_undeclared_group_is_a_line_to_nowhere() {
    let refused = quarter(one(
        "stray",
        link(
            Endpoint::Tile(tid("annex")),
            Endpoint::Group(gid("elsewhere")),
        ),
    ));
    match refused {
        Err(ModelError::LinkToNowhere { link, end }) => {
            assert_eq!(link.0.as_str(), "stray");
            assert_eq!(end, Endpoint::Group(gid("elsewhere")));
        }
        other => panic!("a link to an undeclared group was accepted: {other:?}"),
    }
}

/// A group linked to ITSELF is still `LinkToItself`: no direction, no length,
/// nothing to draw. The widening does not give the degenerate case a new
/// meaning.
#[test]
fn a_group_linked_to_itself_is_still_nothing_to_draw() {
    let refused = quarter(one(
        "loop",
        link(Endpoint::Group(gid("north")), Endpoint::Group(gid("north"))),
    ));
    assert!(
        matches!(refused, Err(ModelError::LinkToItself(_))),
        "{refused:?}"
    );
}

// ---------------------------------------------------------------- anchors

/// THE ANCHOR RULE, STATED ONCE: a line meets a group at whichever of its
/// member cells is closest to the other end, so it touches the region's edge
/// facing what it connects to.
///
/// north holds hall at (0,0) and library at (1,0); annex sits at (4,0). library
/// is three cells from annex and hall is four, so the line leaves north from
/// library — not from hall, which is the map's first key and therefore the cell
/// a resolver that did not measure anything would pick.
#[test]
fn a_group_end_anchors_at_its_member_nearest_the_other_end() {
    let d = quarter(one(
        "feeder",
        link(Endpoint::Group(gid("north")), Endpoint::Tile(tid("annex"))),
    ))
    .expect("a group linked to an ungrouped tile");

    let l = d.link(&lid("feeder")).expect("the link just built");
    assert_eq!(
        d.link_anchors(l),
        Some((cell(1, 0), cell(4, 0))),
        "the line should leave north at library (1,0), the member facing annex"
    );
}

/// The same rule at both ends at once. Neither anchor can be computed without
/// the other, so group-to-group is the CLOSEST PAIR rather than two independent
/// nearest-member questions — which is also the only reading that is symmetric,
/// and a line whose two ends disagreed about which way round they were measured
/// would be drawn differently depending on which end a host started from.
#[test]
fn group_to_group_meets_at_the_closest_pair_of_members() {
    let d = quarter(one(
        "spine",
        link(Endpoint::Group(gid("north")), Endpoint::Group(gid("south"))),
    ))
    .expect("a group linked to another group");

    let l = d.link(&lid("spine")).expect("the link just built");
    assert_eq!(
        d.link_anchors(l),
        Some((cell(1, 0), cell(3, 3))),
        "south has one member, so the pair is decided by north's: library, not hall"
    );
}

/// THE ANCHOR IS RECOMPUTED, NEVER STORED, and this is what that buys. Drag the
/// far end past the group and the line changes which member it leaves from,
/// with nothing in the document edited and nothing to invalidate.
#[test]
fn moving_the_far_end_moves_which_member_the_line_leaves_from() {
    let mut d = quarter(one(
        "feeder",
        link(Endpoint::Group(gid("north")), Endpoint::Tile(tid("annex"))),
    ))
    .expect("a group linked to an ungrouped tile");

    let anchor_of = |d: &Diagram| {
        d.link_anchors(d.link(&lid("feeder")).expect("the link is still there"))
            .expect("both ends are drawable")
            .0
    };
    assert_eq!(
        anchor_of(&d),
        cell(1, 0),
        "annex is east, so library faces it"
    );

    // Annex to the far WEST of both members. `Restore` names the cell outright,
    // which is what a test wants here: a translate would have to be expressed
    // as a delta and the point is the destination.
    let mut to = BTreeMap::new();
    to.insert(tid("annex"), cell(-4, 0));
    d.apply(Command::Restore { cells: to })
        .expect("an empty cell four to the west");

    assert_eq!(
        anchor_of(&d),
        cell(0, 0),
        "annex moved west, so the line must now leave north at hall"
    );
}

/// The free function, over cells nobody has committed.
///
/// IT EXISTS FOR `components`' REASON, ONE FEATURE ALONG. While a pointer is
/// down the arrangement on screen is the one the drag is PROPOSING, not the one
/// the diagram holds, so a view that wants to draw the line the drop would
/// produce has to ask about previewed cells. The alternative is a second
/// nearest-member search in the view, free to disagree with this one.
#[test]
fn anchors_answers_over_cells_the_diagram_does_not_hold() {
    let from: BTreeSet<Cell> = [cell(0, 0), cell(1, 0)].into_iter().collect();
    let to: BTreeSet<Cell> = [cell(9, 9)].into_iter().collect();
    assert_eq!(anchors(&from, &to), Some((cell(1, 0), cell(9, 9))));

    // AN EMPTY SIDE HAS NO ANCHOR, and `None` is the honest answer rather than
    // a default cell: `Command::Connect` refuses an empty group precisely so a
    // caller never has to render this case, and a fabricated (0,0) would draw a
    // line to the origin instead of drawing nothing.
    assert_eq!(anchors(&from, &BTreeSet::new()), None);
    assert_eq!(anchors(&BTreeSet::new(), &to), None);
}

// -------------------------------------------------- reader, writer, bytes

/// The fixture reads, and every end comes back as the KIND the file wrote.
#[test]
fn the_fixture_reads_with_each_end_resolved_to_what_the_file_named() {
    let d = read_turtle(&fixture(), &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the 0.4.0 fixture was refused: {e:?}"));

    let ends = |id: &str| {
        let l = d
            .link(&lid(id))
            .unwrap_or_else(|| panic!("{id} went missing on read"));
        (l.from.clone(), l.to.clone())
    };

    assert_eq!(
        ends("spine"),
        (Endpoint::Group(gid("north")), Endpoint::Group(gid("south")))
    );
    assert_eq!(
        ends("feeder"),
        (Endpoint::Group(gid("north")), Endpoint::Tile(tid("annex")))
    );
    assert_eq!(
        ends("spur"),
        (Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south")))
    );
}

/// A GROUP END IS WRITTEN AS THE GROUP'S OWN SUBJECT, with no `at-`. The `at-`
/// prefix is how a placement is named and a group has a subject of its own
/// already — `hsh:SlugMatchesIri` ties it to the group's slug — so minting one
/// for a group would invent a second IRI for a subject that has one.
#[test]
fn a_group_end_is_written_as_the_groups_own_subject() {
    let d = quarter(one(
        "spine",
        link(Endpoint::Group(gid("north")), Endpoint::Group(gid("south"))),
    ))
    .expect("a group-to-group link");
    let out = write_turtle(&d, &opts());

    assert!(
        out.contains("hive:from d:north"),
        "a group end was not written as its own subject:\n{out}"
    );
    assert!(
        out.contains("hive:to d:south"),
        "a group end was not written as its own subject:\n{out}"
    );
    assert!(
        !out.contains("d:at-north"),
        "the writer minted a placement name for a group:\n{out}"
    );
}

/// ROUND TRIP AND FIXED POINT, over the fixture rather than over a diagram this
/// test built — so the bytes on trial are bytes this crate did not choose.
///
/// THE FIXED POINT IS THE HALF THAT CATCHES THE REAL BUG. A term the reader
/// parses and forgets to name in its `consumed` list comes back as a host
/// predicate and is written a SECOND time on the next export; the document
/// grows a duplicate every round trip and nothing but this comparison notices.
/// `ttl/read.rs` carries two comments naming exactly that failure, one for
/// `hive:link` and one for a link's own `hive:slug`, because it has happened
/// twice.
#[test]
fn the_fixture_round_trips_and_a_second_export_is_byte_identical() {
    let first = read_turtle(&fixture(), &ReadOpts::default())
        .unwrap_or_else(|e| panic!("the 0.4.0 fixture was refused: {e:?}"));
    let o = opts();
    let once = write_turtle(&first, &o);

    let again = read_turtle(&once, &ReadOpts::default()).unwrap_or_else(|e| {
        panic!("the reader refused the writer's own group-ended output: {e:?}\n{once}")
    });
    let twice = write_turtle(&again, &o);

    assert_eq!(
        once, twice,
        "the second export differs from the first, which is what a term parsed and left out of \
         a `consumed` list looks like"
    );
    assert_eq!(first.links().len(), again.links().len());
    for (id, l) in first.links() {
        let back = again
            .link(id)
            .unwrap_or_else(|| panic!("{id:?} went missing"));
        assert_eq!((&l.from, &l.to), (&back.from, &back.to));
    }
}

/// A `hive:from` naming a subject that is neither a placement nor a group is
/// still refused. The widening admits ONE more kind of end, not anything at all
/// — see `ReadError::UnresolvedLinkEnd` and `tests/link_ends.rs` for what the
/// old guess did instead.
#[test]
fn widening_the_ends_did_not_make_the_resolver_permissive() {
    let bad = fixture().replace("hive:from d:north ;", "hive:from d:hall ;");
    assert!(
        bad.contains("hive:from d:hall"),
        "the mutation did not apply, so this test would be checking the clean fixture"
    );
    let refused = read_turtle(&bad, &ReadOpts::default());
    assert!(
        refused.is_err(),
        "`d:hall` is a hive:Tile — neither a placement nor a group — and was accepted"
    );
}
