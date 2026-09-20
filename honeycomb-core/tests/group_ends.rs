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
    Cell, Command, Content, Diagram, DiagramSpec, Endpoint, Group, GroupId, History,
    LatticeConvention, Link, LinkId, ModelError, OwnTile, ReadOpts, Rejection, Routing, Slug, Text,
    TileId, WriteOpts, anchors, read_turtle, write_turtle,
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

// ------------------------------------------------------------------ rules
//
// THE LIVE-EDIT HALF OF THE SAME FOUR DECISIONS. `Diagram::try_new` refuses a
// DOCUMENT that asserts an undrawable link; these refuse the EDIT that would
// produce one. Both are needed and neither substitutes for the other:
// `Diagram::apply` mutates in place and never reconstructs through `try_new`,
// so a `check` that let one of these through would build a live diagram whose
// own constructor would have rejected it — and the next export writes that
// diagram to a file.
//
// They live in this file rather than in `tests/rules.rs` because every one of
// them needs the same four tiles and two groups `quarter` already builds, and
// because what is on trial is the endpoint rather than the command.

/// DECISION 2, AT THE DOORWAY. `Command::Connect` refuses an empty group for
/// the reason `try_new` does — and it is the doorway that matters in practice,
/// because a picker offers every declared group and six of production's
/// fourteen have nothing in them.
#[test]
fn connecting_to_a_group_with_no_members_is_refused() {
    let mut d = quarter(BTreeMap::new()).expect("a diagram with no links yet");
    d.apply(Command::DeclareGroup {
        id: gid("nobody"),
        group: group("Nobody"),
    })
    .expect("a group with no members may be declared — that is what makes it joinable");

    let refused = d.check(&Command::Connect {
        id: lid("spine"),
        link: link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("nobody"))),
    });
    match refused {
        Err(Rejection::EmptyGroupEnd { link, group }) => {
            assert_eq!(link.0.as_str(), "spine");
            assert_eq!(group.0.as_str(), "nobody");
        }
        other => panic!("a line to a group with no cells was allowed: {other:?}"),
    }
}

/// DECISION 3, AT THE DOORWAY, and the mis-drag it is really about: the user
/// pressed on a ground and released on one of that ground's own hexagons.
#[test]
fn connecting_a_group_to_its_own_member_is_refused_either_way_round() {
    let d = quarter(BTreeMap::new()).expect("a diagram with no links yet");

    for (from, to) in [
        (
            Endpoint::Group(gid("north")),
            Endpoint::Tile(tid("library")),
        ),
        (
            Endpoint::Tile(tid("library")),
            Endpoint::Group(gid("north")),
        ),
    ] {
        match d.check(&Command::Connect {
            id: lid("inward"),
            link: link(from.clone(), to.clone()),
        }) {
            Err(Rejection::LinkToOwnMember { link, group, tile }) => {
                assert_eq!(link.0.as_str(), "inward");
                assert_eq!(group.0.as_str(), "north");
                assert_eq!(tile.0.as_str(), "library");
            }
            other => panic!("{from:?} -> {to:?} was allowed: {other:?}"),
        }
    }
}

/// DECISION 3 REACHED THE OTHER WAY. The link is drawn first and the
/// containment second: attaching a tile to a group it is already linked to
/// produces exactly the state the rule above refuses, so `Attach` has to refuse
/// it too or the rule is a rule the user walks around.
#[test]
fn attaching_a_tile_to_a_group_it_is_already_linked_to_is_refused() {
    let d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    match d.check(&Command::Attach {
        tile: tid("annex"),
        group: gid("south"),
    }) {
        Err(Rejection::LinkToOwnMember { link, group, tile }) => {
            assert_eq!(link.0.as_str(), "spur");
            assert_eq!(group.0.as_str(), "south");
            assert_eq!(tile.0.as_str(), "annex");
        }
        other => panic!(
            "annex joined the very group it is linked to, so the board now holds a link \
             containment already states: {other:?}"
        ),
    }
}

/// DECISION 4. `south` has one member and a line reaching it; taking that
/// member off the board would leave the line with no cell to meet.
///
/// IT NAMES THE LINKS, which is what makes the refusal actionable rather than a
/// dead end: links can be removed, so "disconnect spur first" is advice the
/// user can follow. `Rejection::StillLinked` already works this way for a tile
/// a link touches directly.
#[test]
fn removing_the_last_member_of_a_linked_group_is_refused_and_names_the_links() {
    let mut d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    match d.check(&Command::Remove { tile: tid("depot") }) {
        Err(Rejection::LastMemberStillLinked { tile, group, links }) => {
            assert_eq!(tile.0.as_str(), "depot");
            assert_eq!(group.0.as_str(), "south");
            assert_eq!(
                links.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
                vec!["spur"]
            );
        }
        other => panic!("the south side was emptied out from under its own link: {other:?}"),
    }

    // AND THE ADVICE WORKS. A refusal that names a fix nobody can carry out is
    // worse than one that says nothing, so the fix is exercised here rather
    // than asserted in prose.
    d.apply(Command::Disconnect { id: lid("spur") })
        .expect("a link can always be removed");
    d.apply(Command::Remove { tile: tid("depot") })
        .expect("with the line gone, the last member may leave");
}

/// THE OTHER SIDE OF DECISION 4, which is the half that keeps it from being a
/// blanket ban on editing a linked group. `north` has two members and a line
/// reaching it; removing one leaves the other, so the line still has a cell and
/// nothing is refused.
#[test]
fn removing_a_member_that_is_not_the_last_one_is_allowed() {
    let mut d = quarter(one(
        "feeder",
        link(Endpoint::Group(gid("north")), Endpoint::Tile(tid("annex"))),
    ))
    .expect("the north side linked to annex");

    d.apply(Command::Remove { tile: tid("hall") })
        .expect("library is still in north, so the line still has somewhere to meet it");
    let l = d.link(&lid("feeder")).expect("the link is untouched");
    assert_eq!(
        d.link_anchors(l),
        Some((cell(1, 0), cell(4, 0))),
        "the surviving member is where the line meets north now"
    );
}

/// A MEMBER CAN LEAVE A GROUP WITHOUT LEAVING THE BOARD, and that empties the
/// group just as completely. `Detach` is the second of the three ways to take
/// the last member out, so it refuses on the same terms — the through-line is
/// about the STATE, not about which verb reached it.
#[test]
fn detaching_the_last_member_of_a_linked_group_is_refused() {
    let d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    match d.check(&Command::Detach { tile: tid("depot") }) {
        Err(Rejection::LastMemberStillLinked { group, links, .. }) => {
            assert_eq!(group.0.as_str(), "south");
            assert_eq!(
                links.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
                vec!["spur"]
            );
        }
        other => panic!("detaching emptied a linked group: {other:?}"),
    }
}

/// And the third way: moving the last member into ANOTHER group. The tile stays
/// on the board and `south` still ends up with nothing in it.
#[test]
fn attaching_the_last_member_of_a_linked_group_elsewhere_is_refused() {
    let d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    match d.check(&Command::Attach {
        tile: tid("depot"),
        group: gid("north"),
    }) {
        Err(Rejection::LastMemberStillLinked { group, .. }) => {
            assert_eq!(group.0.as_str(), "south");
        }
        other => {
            panic!("the last member walked into another group and left a link behind: {other:?}")
        }
    }
}

/// A LINKED GROUP MUST NOT VANISH. `RemoveGroup` already refuses a group with
/// members, and a group with a link always has one — decisions 2, 3 and 4
/// between them see to that — so this check is reachable only because it is
/// asked FIRST.
///
/// THE ORDER IS THE POINT. `Rejection::GroupInUse` tells the user to take the
/// members out of the group first; for the LAST member that advice is itself
/// refused, by the test above. Reporting the link first means the user is never
/// told to do something that will not work.
#[test]
fn removing_a_linked_group_is_refused_naming_the_links_before_the_members() {
    let d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    match d.check(&Command::RemoveGroup { id: gid("south") }) {
        Err(Rejection::GroupStillLinked { group, links }) => {
            assert_eq!(group.0.as_str(), "south");
            assert_eq!(
                links.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
                vec!["spur"]
            );
        }
        other => panic!(
            "a group with a line reaching it was undeclared, leaving the link naming a group \
             the diagram no longer has: {other:?}"
        ),
    }
}

// ------------------------------------------------------------- own_tile_mut

/// `Diagram::own_tile_mut` used to return `&mut OwnTile`, and `OwnTile.group`
/// was a `pub` field: a caller holding that reference could clear a linked
/// group's LAST member's `group` with a plain field write — no `Command`, no
/// `check`, no `Rejection` at all — and reach the exact state
/// `detaching_the_last_member_of_a_linked_group_is_refused` above proves
/// `Command::Detach` refuses. A test naming this function that ended
/// `.own_tile_mut(&tid("depot")).unwrap().group = None;` passed here, against
/// exactly this fixture, before the fix; the same line now fails to COMPILE
/// with `error[E0609]: no field `group` on type `OwnTileEdit<'_>``, because
/// `own_tile_mut` now returns [`honeycomb_core::OwnTileEdit`], which has no
/// such field — see `model.rs`'s `OwnTileEdit` and `own_tile_mut` for the
/// reasoning. Both the passing run and the compiler error are kept verbatim in
/// the commit that made this change, rather than in the tree, because a test
/// proven not to compile has nowhere left to run.
///
/// WHAT THIS TEST PINS INSTEAD: the fix removed ONE field from the view, not
/// the capability. The accessor still hands out every field a live edit
/// legitimately needs — even on the last member of a linked group, where the
/// old exploit lived.
#[test]
fn own_tile_mut_still_edits_content_on_the_last_member_of_a_linked_group() {
    let mut d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");

    let held = d
        .own_tile_mut(&tid("depot"))
        .expect("depot is a standalone tile with content to edit");
    *held.label = Text::plain("Depot Yard");

    let relabelled = match d.content() {
        Content::Standalone { tiles } => tiles
            .get(&tid("depot"))
            .expect("depot is still on the board")
            .label
            .clone(),
        Content::Pinned { .. } => panic!("the fixture is standalone"),
    };
    assert_eq!(
        relabelled,
        Text::plain("Depot Yard"),
        "the edit did not reach the stored tile"
    );

    // AND SOUTH IS UNTOUCHED. The edit above went through the narrowed view;
    // it had nowhere to put a group change even if it had tried.
    assert_eq!(
        d.members(&gid("south")),
        vec![tid("depot")],
        "editing depot's label emptied its group, which no field on OwnTileEdit can reach"
    );
}

// -------------------------------------------------------------------- undo

/// NO RECORDED INVERSE IS MADE UNAPPLYABLE BY THE NEW REFUSALS.
///
/// The hazard is the one `Command::Swap`'s own doc names: an inverse that can
/// be REFUSED later wedges the stack, because `History::undo` pushes a refusal
/// back and every later undo retries the same failure forever with nothing on
/// screen to say why. Decision 2 is the obvious way to get one — a `Connect`
/// recorded as the inverse of a `Disconnect`, replayed after the group at its
/// far end has been emptied.
///
/// WHAT MAKES IT SAFE IS NOT THE CHECK, IT IS THE ORDER. Emptying that group is
/// only possible once the link is gone, so every command that empties it is
/// recorded ABOVE the `Connect` on the undo stack and is undone BEFORE it. The
/// sequence below is that argument driven rather than asserted: disconnect,
/// then empty the group two different ways, then undo everything and require
/// every step to succeed.
#[test]
fn an_inverse_recorded_before_a_group_emptied_still_applies_when_it_comes_back() {
    let mut d = quarter(one(
        "spur",
        link(Endpoint::Tile(tid("annex")), Endpoint::Group(gid("south"))),
    ))
    .expect("annex linked to the south side");
    let start = d.clone();
    let mut h = History::default();

    for cmd in [
        // The link first, because nothing below is permitted while it is there.
        Command::Disconnect { id: lid("spur") },
        // Now empty `south`, and then take the group away entirely.
        Command::Detach { tile: tid("depot") },
        Command::RemoveGroup { id: gid("south") },
    ] {
        let inverse = d
            .apply(cmd.clone())
            .unwrap_or_else(|e| panic!("{cmd:?} was refused with the link already gone: {e:?}"));
        h.record(inverse);
    }

    let mut undone = 0;
    while h.can_undo() {
        assert!(
            h.undo(&mut d).is_some(),
            "an undo was refused and pushed back onto the stack, which wedges it: the inverse \
             recorded {undone} step(s) from the end is no longer applyable"
        );
        undone += 1;
    }
    assert_eq!(undone, 3);
    assert_eq!(d, start, "undoing everything did not restore the diagram");
}

/// THE INVERSE OF A GROUP EDIT CANNOT RESURRECT A FORBIDDEN LINK, stated
/// directly rather than left to follow from the ordering argument above.
///
/// `RemoveGroup`'s inverse is a `DeclareGroup` carrying the group's whole value,
/// and a group that could be removed was EMPTY — so replaying it puts an empty
/// group back and touches no link at all. There is no recorded inverse anywhere
/// in this enum that adds a link and a group membership in one step, which is
/// what it would take to land in a decision-2 state by undoing.
#[test]
fn undoing_a_group_removal_puts_back_an_empty_group_and_no_link() {
    let mut d = quarter(BTreeMap::new()).expect("a diagram with no links yet");
    d.apply(Command::DeclareGroup {
        id: gid("nobody"),
        group: group("Nobody"),
    })
    .expect("an empty group may be declared");

    let inverse = d
        .apply(Command::RemoveGroup { id: gid("nobody") })
        .expect("an empty group with no link may be undeclared");
    d.apply(inverse).expect("and put back");

    assert!(d.has_group(&gid("nobody")));
    assert!(d.members(&gid("nobody")).is_empty());
    assert!(
        d.links().is_empty(),
        "undoing a group edit invented a link, which is the one thing it must never do"
    );
}

/// NOT A TEST OF BEHAVIOUR. An escape hatch, because the one thing this
/// workspace cannot check about `shapes.ttl` is whether it actually validates
/// anything: `honeycomb-core` has zero dependencies and keeps zero, so no
/// SHACL engine is linked here and `tests/vocabulary.rs` says in its own header
/// that a pass there is never "these files are valid SHACL".
///
/// What was run by hand against this dump, and what it established — recorded
/// because the next person to widen a shape will want to repeat it and there
/// is nothing in the repository to tell them how:
///
/// ```text
/// HONEYCOMB_DUMP=/tmp/export.ttl \
///   cargo test -p honeycomb-core --test group_ends -- dump_ --ignored
/// python3 -m venv /tmp/shaclenv && /tmp/shaclenv/bin/pip install pyshacl
/// /tmp/shaclenv/bin/pyshacl -s shapes.ttl -e ns.ttl -a /tmp/export.ttl
/// ```
///
/// The writer's own group-ended output CONFORMS, as do all four committed
/// fixtures and the demo's export. Four mutations of the fixture were each
/// caught by the shape that claims them, with the message written for it:
/// an end naming an empty group (`hsh:LinkEndIsDrawable`), a group linked to
/// its own member (`hsh:NoLinkToItself`), an end naming a group another
/// diagram declares (`hsh:LinkEndsBelongToItsDiagram`), and an end naming a
/// `hive:Tile` subject (`hsh:LinkShape`'s `sh:or`, reporting the OUTER message
/// and not either branch's — which is convention 2 at the head of `shapes.ttl`
/// confirmed rather than assumed).
#[test]
#[ignore = "writes a file; run by hand when checking the shapes with pyshacl"]
fn dump_the_group_ended_export() {
    let Ok(path) = std::env::var("HONEYCOMB_DUMP") else {
        return;
    };
    let d = read_turtle(&fixture(), &ReadOpts::default()).expect("the fixture reads");
    std::fs::write(path, write_turtle(&d, &opts())).expect("the dump path is writable");
}
