//! The drop rules, as pure functions: no pointer, no SVG, no browser anywhere.
//!
//! WHAT BREAKS WITHOUT THIS FILE. Everything below is invisible in the Turtle
//! the editor writes and unmistakable on screen, which is the worst order to
//! discover a bug in — the document reviews clean and the drawing is wrong.
//!
//!   * TWO TILES IN ONE CELL. In Turtle that is two well-formed placements; on
//!     screen one hexagon simply covers another, and which one wins depends on
//!     statement order, so the same file draws differently after an unrelated
//!     re-export. It has to be refused, and refused WITH ITS EVIDENCE: "you
//!     can't drop here" leaves the host nothing to outline, and a user who
//!     cannot see what is in the way just tries the same drop again.
//!
//!   * A GROUP MOVE APPLIED HALFWAY. The likeliest real bug in this feature is a
//!     loop that moves as it goes and stops at the first collision, leaving five
//!     tiles moved and one behind: a rearrangement nobody asked for, produced by
//!     a refusal.
//!
//!   * A GROUP THAT QUIETLY LOSES ITS ARRANGEMENT. Translating members in OFFSET
//!     coordinates looks right, and IS right for every delta whose row component
//!     is even. Half of all deltas break the shape. A suite that only tries even
//!     row deltas certifies the wrong implementation, so the odd ones are tried
//!     here exhaustively, and the broken alternative is asserted as broken.
//!
//!   * A GROUP REGION THAT STOPS FOLLOWING ITS MEMBERS. The region is derived
//!     from the members' cells on every render; an anchor stored beside them is
//!     a second source of truth that disagrees the moment one member is dragged.
//!
//! And one DECISION is encoded here, so that a later "fix" has to argue with a
//! test instead of deleting a paragraph: A FRACTURED GROUP IS REPORTED, NEVER
//! REFUSED. Pulling a member out and putting it back somewhere else necessarily
//! passes through a split state, so an editor that enforces contiguity at every
//! step makes half of all legal rearrangements impossible.
//!
//! Every fixture is a hive:pinned diagram. The rules under test are about cells
//! and membership, and a pinned placement carries nothing else — which is also
//! why no assertion here mentions a label.

use std::collections::{BTreeMap, BTreeSet};

use honeycomb_core::{
    Axial, Cell, Command, Content, Diagram, DiagramSpec, Group, GroupId, Iri, Lattice,
    LatticeConvention, ModelError, PinnedTile, Rejection, Slug, TileId,
};

// ---------------------------------------------------------------- fixtures

/// r = 46, gap = 1 are the numbers every measurement in the design was taken
/// at, so a figure printed by a failure here can be compared with one in the
/// design without converting it. Nothing in these tests depends on the size:
/// the radius is presentation and is never serialised.
const LATTICE: Lattice = Lattice::new(46.0, 1.0);

fn slug(s: &str) -> Slug {
    Slug::parse(s)
        .unwrap_or_else(|e| panic!("the fixture's own identifier {s:?} is not a slug: {e:?}"))
}

fn tile_id(s: &str) -> TileId {
    TileId(slug(s))
}

fn group_id(s: &str) -> GroupId {
    GroupId(slug(s))
}

fn cell(col: i32, row: i32) -> Cell {
    Cell { col, row }
}

fn axial(q: i32, r: i32) -> Axial {
    Axial { q, r }
}

/// A pinned diagram from (tile, cell, group) triples, declaring every group a
/// tile names. Pinned rather than standalone because the rules under test are
/// the ones a placement can express, and a placement expresses a cell and a
/// membership and nothing else.
fn try_pinned(tiles: &[(&str, Cell, Option<&str>)]) -> Result<Diagram, ModelError> {
    let mut groups: BTreeMap<GroupId, Group> = BTreeMap::new();
    let mut placed: BTreeMap<TileId, PinnedTile> = BTreeMap::new();
    let mut cells: BTreeMap<TileId, Cell> = BTreeMap::new();

    for (name, at, group) in tiles {
        let id = tile_id(name);
        let group = group.map(|g| {
            let gid = group_id(g);
            groups.entry(gid.clone()).or_insert_with(|| Group {
                label: format!("Group {g}"),
                style_key: None,
                note: None,
                extra: Vec::new(),
            });
            gid
        });
        placed.insert(
            id.clone(),
            PinnedTile {
                group,
                // Opaque to this crate, and deliberately in a namespace that
                // belongs to nobody: a fixture that named a real host would be
                // the first thread of the coupling this component refuses.
                represents: Iri(format!("https://example.org/host/{name}")),
            },
        );
        cells.insert(id, *at);
    }

    Diagram::try_new(DiagramSpec {
        slug: slug("rules-fixture"),
        label: "Rules fixture".to_string(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups,
        content: Content::Pinned {
            source: Iri("https://example.org/host/source".to_string()),
            revision: None,
            tiles: placed,
        },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
}

fn pinned(tiles: &[(&str, Cell, Option<&str>)]) -> Diagram {
    try_pinned(tiles)
        .unwrap_or_else(|e| panic!("a fixture this file believes is legal was refused: {e:?}"))
}

/// One group, `member-0..n`, at the given cells. `member-0` is the tile every
/// test grabs.
fn grouped(cells: &[Cell]) -> Diagram {
    let names: Vec<String> = (0..cells.len()).map(|i| format!("member-{i}")).collect();
    let spec: Vec<(&str, Cell, Option<&str>)> = names
        .iter()
        .zip(cells)
        .map(|(name, at)| (name.as_str(), *at, Some("stack")))
        .collect();
    pinned(&spec)
}

/// The members' cells in fixture order, so an arrangement can be compared with
/// the one it started as.
fn member_cells(d: &Diagram, n: usize) -> Vec<Cell> {
    (0..n)
        .map(|i| {
            let id = tile_id(&format!("member-{i}"));
            d.cell_of(&id).unwrap_or_else(|| {
                panic!(
                    "member-{i} has no cell after a move: a tile the occupancy index has forgotten \
                     is a tile the renderer can only drop or pile at the origin"
                )
            })
        })
        .collect()
}

/// The multiset of pairwise PIXEL offsets — the only description of "shape"
/// that does not change when the whole group moves, and the one that answers
/// the question a reader actually asks: does the group still look like itself?
///
/// Quantised to a micro-unit on purpose. Two expressions equal in the reals
/// differ in the last f64 bit, and last-bit noise must not read as a group that
/// changed shape; the cells are hundreds of units apart, so 1e-6 is far below
/// anything a real difference could be and far above the noise.
fn pixel_shape(cells: &[Cell]) -> Vec<(i64, i64)> {
    let mut offsets = Vec::with_capacity(cells.len() * cells.len());
    for a in cells {
        let (ax, ay) = LATTICE.centre(*a);
        for b in cells {
            let (bx, by) = LATTICE.centre(*b);
            offsets.push((micro(bx - ax), micro(by - ay)));
        }
    }
    offsets.sort_unstable();
    offsets
}

fn micro(v: f64) -> i64 {
    (v * 1e6).round() as i64
}

/// The four arrangements every exhaustive test below is run over. The last one
/// straddles rows 0 and -1 because row -1 is not hypothetical — negative rows
/// are ordinary, the origin being wherever the first tile landed rather than a
/// corner — and it is exactly where `/` instead of `div_euclid` stops agreeing
/// with the lattice.
fn shapes() -> Vec<(&'static str, Vec<Cell>)> {
    vec![
        (
            "a 2x2",
            vec![cell(0, 0), cell(1, 0), cell(0, 1), cell(1, 1)],
        ),
        (
            "a 3x2",
            vec![
                cell(0, 0),
                cell(1, 0),
                cell(2, 0),
                cell(0, 1),
                cell(1, 1),
                cell(2, 1),
            ],
        ),
        ("an L", vec![cell(0, 0), cell(0, 1), cell(0, 2), cell(1, 2)]),
        (
            "a shape straddling rows 0 and -1",
            vec![cell(0, -1), cell(1, -1), cell(0, 0), cell(1, 0)],
        ),
    ]
}

/// Piece sizes, sorted, with the two ways a flood fill can lie ruled out first:
/// no pieces at all, and a piece with nothing in it.
fn component_sizes(d: &Diagram, g: &GroupId) -> Vec<usize> {
    let pieces = d.group_components(g);
    assert!(
        !pieces.is_empty(),
        "a group with members reports no pieces at all: a host asking how many regions to draw is \
         told none, and every member of the group disappears from the drawing"
    );
    assert!(
        pieces.iter().all(|piece| !piece.is_empty()),
        "an empty piece was reported: the host draws a region around no cells, which is either an \
         invisible ground or a stray outline in the middle of the lattice"
    );
    let mut sizes: Vec<usize> = pieces.iter().map(|piece| piece.len()).collect();
    sizes.sort_unstable();
    sizes
}

// ------------------------------------------------- two tiles in one cell

/// Caught at construction, not at render. A `Diagram` that exists must be one
/// that can be drawn, because nothing downstream re-checks it.
#[test]
fn two_tiles_may_not_occupy_one_cell() {
    let refused = try_pinned(&[("vault", cell(2, 1), None), ("registry", cell(2, 1), None)])
        .expect_err(
            "a diagram with two tiles on one cell was constructed: it draws one hexagon over \
             another, and which one survives depends on statement order — so the same file draws \
             differently after an unrelated re-export",
        );

    match refused {
        ModelError::DuplicateCell {
            cell: collision,
            first,
            second,
        } => {
            assert_eq!(
                collision,
                cell(2, 1),
                "the refusal names a cell the collision is not in, so a host that highlights it \
                 points the user at the wrong hexagon"
            );
            let named: BTreeSet<TileId> = [first, second].into_iter().collect();
            let expected: BTreeSet<TileId> = [tile_id("vault"), tile_id("registry")]
                .into_iter()
                .collect();
            assert_eq!(
                named, expected,
                "the refusal must name BOTH tiles: 'that cell is occupied' with one name leaves \
                 the caller to find the other half of the pair itself, and it is the pair that is \
                 the problem"
            );
        }
        other => panic!(
            "two tiles on one cell were refused as {other:?}: reported under any other name, the \
             caller goes looking for a different bug than the one it has"
        ),
    }
}

/// A refusal has to carry what it refused over, and `check` has to be pure —
/// the view calls it on every pointer move so it can paint the refusal BEFORE
/// the user releases, and a refusal discovered on release is discovered too
/// late.
#[test]
fn a_refused_drop_names_the_tiles_in_the_way() {
    let d = pinned(&[("alpha", cell(0, 0), None), ("beta", cell(2, 0), None)]);
    let untouched = d.clone();

    // alpha (0,0) dropped straight onto beta (2,0).
    let refused = d
        .check(&Command::Translate {
            grabbed: tile_id("alpha"),
            delta: axial(2, 0),
            detach: false,
        })
        .expect_err(
            "a drop onto an occupied cell was approved: the two tiles end up sharing a cell, which \
             is invisible in the file and a covered hexagon on screen",
        );

    match refused {
        Rejection::Occupied { blocked } => {
            assert!(
                !blocked.is_empty(),
                "the refusal carries no evidence: the host has nothing to outline, so the user is \
                 told no and not told by what, and tries the same drop again"
            );
            assert_eq!(
                blocked,
                vec![(cell(2, 0), tile_id("beta"))],
                "the evidence does not name the tile actually in the way, so the host outlines the \
                 wrong hexagon — which is worse than outlining none"
            );
        }
        other => panic!(
            "the drop was refused as {other:?}: only Occupied carries blockers, so any other \
             rejection here silently costs the user the explanation"
        ),
    }

    assert_eq!(
        d, untouched,
        "check() mutated the diagram: it runs on every pointer move, so a check that edits drags \
         the document along with the pointer and Escape has nothing left to restore"
    );
}

/// The likeliest real bug in the whole feature: a loop that applies as it goes
/// and stops at the first conflict.
#[test]
fn a_group_move_is_all_or_nothing() {
    let block = [
        cell(0, 0),
        cell(1, 0),
        cell(2, 0),
        cell(0, 1),
        cell(1, 1),
        cell(2, 1),
    ];
    let mut d = pinned(&[
        ("member-0", block[0], Some("stack")),
        ("member-1", block[1], Some("stack")),
        ("member-2", block[2], Some("stack")),
        ("member-3", block[3], Some("stack")),
        ("member-4", block[4], Some("stack")),
        ("member-5", block[5], Some("stack")),
        // One cell of the destination footprint, and only one.
        ("boulder", cell(3, 0), None),
    ]);

    let refused = d
        .apply(Command::Translate {
            grabbed: tile_id("member-0"),
            delta: axial(1, 0),
            detach: false,
        })
        .expect_err(
            "a six-member group was moved onto a cell another tile already holds: one of the seven \
             tiles is now underneath another",
        );

    match refused {
        Rejection::Occupied { blocked } => {
            assert_eq!(
                blocked,
                vec![(cell(3, 0), tile_id("boulder"))],
                "the refusal does not name the single tile in the way, so the host cannot show the \
                 user the one cell that has to be cleared"
            );
        }
        other => panic!("the group move was refused as {other:?}, which names no blocker at all"),
    }

    assert_eq!(
        member_cells(&d, block.len()),
        block.to_vec(),
        "a refused group move left members somewhere new: five moved and one stayed behind is a \
         rearrangement nobody asked for, produced by a refusal — and the user's only way back is \
         to spot it and undo it"
    );
    assert_eq!(
        d.cell_of(&tile_id("boulder")),
        Some(cell(3, 0)),
        "the tile that did the blocking moved: the refusal edited the thing it was protecting"
    );
}

/// THE ONLY TEST FOR THE "restricted to tiles not in the moving set" CLAUSE. An
/// occupancy check that forgets it passes every other test in this file and
/// refuses every short group drag, because a group translated by one cell
/// always overlaps its own old footprint.
#[test]
fn a_group_may_overlap_its_own_old_footprint() {
    let before = [cell(0, 0), cell(1, 0), cell(0, 1), cell(1, 1)];
    let after = [cell(1, 0), cell(2, 0), cell(1, 1), cell(2, 1)];

    let origin: BTreeSet<Cell> = before.iter().copied().collect();
    let destination: BTreeSet<Cell> = after.iter().copied().collect();
    assert!(
        origin.intersection(&destination).next().is_some(),
        "the fixture's destination does not overlap its origin, so this test would pass against an \
         implementation that refuses every short group drag — which is the only thing it exists to \
         catch"
    );

    let mut d = grouped(&before);
    d.apply(Command::Translate {
        grabbed: tile_id("member-0"),
        delta: axial(1, 0),
        detach: false,
    })
    .unwrap_or_else(|e| {
        panic!(
            "a group moved one cell was refused with {e:?}: it collided with the cells it is \
             vacating, so the check is testing the destination against unrestricted occupancy and \
             no group can ever be nudged"
        )
    });

    assert_eq!(
        member_cells(&d, before.len()),
        after.to_vec(),
        "the group did not land where a one-cell translation puts it"
    );
}

/// EXHAUSTIVE OVER THE DELTAS, and the one that has to be, because offset-space
/// translation is right for every even row delta and wrong for every odd one.
/// The second half asserts the negative: that the obvious implementation IS
/// broken, so a later simplification to offset arithmetic fails a test that
/// explains why the code is not the obvious thing.
#[test]
fn a_group_keeps_its_shape_across_an_odd_row_delta() {
    let mut moved = 0usize;
    let mut stood_still = 0usize;

    for (name, cells) in shapes() {
        let start = pixel_shape(&cells);
        for q in -3..=3 {
            for r in -3..=3 {
                let mut d = grouped(&cells);
                let outcome = d.apply(Command::Translate {
                    grabbed: tile_id("member-0"),
                    delta: axial(q, r),
                    detach: false,
                });
                match outcome {
                    Ok(_inverse) => {
                        assert!(
                            q != 0 || r != 0,
                            "a zero delta was applied as a move: it is neither a change nor a \
                             refusal, and a host that is not told so fires an edit — and an undo \
                             entry — for every click that selects a tile"
                        );
                        assert_eq!(
                            pixel_shape(&member_cells(&d, cells.len())),
                            start,
                            "{name} lost its arrangement under axial delta ({q},{r}): the members \
                             no longer sit at the same offsets from one another, so the group \
                             arrived as a different drawing than the one that was dragged"
                        );
                        moved += 1;
                    }
                    Err(Rejection::NoMove) => {
                        assert!(
                            q == 0 && r == 0,
                            "{name} was told a real delta ({q},{r}) is no move at all, so the drag \
                             silently becomes a selection and the user's rearrangement is lost"
                        );
                        stood_still += 1;
                    }
                    Err(other) => panic!(
                        "{name} was refused delta ({q},{r}) with {other:?} on an otherwise empty \
                         lattice: nothing bounds the lattice — the viewBox is derived from the \
                         content — so a group alone on the board can always translate"
                    ),
                }
            }
        }
    }

    assert_eq!(
        moved,
        4 * 48,
        "{moved} of the 192 non-zero deltas actually moved a group: the invariant above was \
         asserted against fewer arrangements than this test claims to cover"
    );
    assert_eq!(
        stood_still, 4,
        "the zero delta was not reported as NoMove once per shape"
    );

    // THE NEGATIVE. Translating every member by the grabbed tile's (dcol, drow)
    // — the obvious implementation — is measured here rather than argued about.
    let mut broken = 0usize;
    let mut odd_row_deltas = 0usize;
    let mut broken_with_an_even_row_delta = 0usize;

    for (_, cells) in shapes() {
        let start = pixel_shape(&cells);
        let grab = cells[0];
        for q in -3..=3 {
            for r in -3..=3 {
                let target = Cell::from_axial(grab.to_axial().plus(axial(q, r)));
                let (dcol, drow) = (target.col - grab.col, target.row - grab.row);
                let naive: Vec<Cell> = cells
                    .iter()
                    .map(|c| cell(c.col + dcol, c.row + drow))
                    .collect();
                if drow.rem_euclid(2) == 1 {
                    odd_row_deltas += 1;
                }
                if pixel_shape(&naive) != start {
                    broken += 1;
                    if drow.rem_euclid(2) == 0 {
                        broken_with_an_even_row_delta += 1;
                    }
                }
            }
        }
    }

    assert!(
        broken > 0,
        "translating in OFFSET space broke none of the 196 cases, so this file no longer \
         demonstrates why the delta is axial — and the next reader is free to 'simplify' the \
         rule into the one that loses a group's arrangement on every odd row delta"
    );
    assert_eq!(
        broken_with_an_even_row_delta, 0,
        "an even row delta broke a shape in offset space: the two spaces agree there, so either \
         these fixtures or Cell::from_axial no longer follow the odd-r convention, and every \
         number this test reports is measuring something else"
    );
    assert_eq!(
        broken, odd_row_deltas,
        "offset-space translation broke {broken} cases against {odd_row_deltas} odd-row deltas; \
         the design measured 112 of 196, EXACTLY the odd-row ones, which is why a suite that only \
         tries even row deltas certifies the wrong implementation"
    );
}

// ------------------------------------------------------- stick and follow

/// The group follows because the region is re-derived from its members, not
/// because anything was written down when they moved.
#[test]
fn moving_one_member_moves_the_whole_group_and_stores_no_anchor() {
    let mut d = pinned(&[
        ("north", cell(0, 0), Some("stack")),
        ("south", cell(0, 1), Some("stack")),
        ("east", cell(1, 0), Some("stack")),
        ("loner", cell(6, 3), None),
    ]);
    let stack = group_id("stack");

    let moving = d.moving_set(&tile_id("north"), false);
    let members: BTreeSet<TileId> = ["north", "south", "east"]
        .into_iter()
        .map(tile_id)
        .collect();
    assert_eq!(
        moving, members,
        "grabbing one member did not pick up its group: a group whose members do not follow is \
         three tiles that happen to share a ground colour, and the user has to drag each one"
    );

    let group_before = d
        .group(&stack)
        .cloned()
        .expect("the fixture declares the group its tiles name");

    // north (0,0) -> (2,1); the other two ride along.
    let inverse = d
        .apply(Command::Translate {
            grabbed: tile_id("north"),
            delta: axial(2, 1),
            detach: false,
        })
        .unwrap_or_else(|e| panic!("a group move onto empty cells was refused with {e:?}"));

    for (name, landed) in [
        ("north", cell(2, 1)),
        ("south", cell(3, 2)),
        ("east", cell(3, 1)),
    ] {
        assert_eq!(
            d.cell_of(&tile_id(name)),
            Some(landed),
            "{name} did not travel with the group: the members arrived at different offsets from \
             one another, which is a group that changed shape in transit"
        );
    }
    assert_eq!(
        d.cell_of(&tile_id("loner")),
        Some(cell(6, 3)),
        "an ungrouped tile moved with the group: membership is what a drag acts on, and a tile \
         that follows a group it does not belong to cannot be dragged out of it"
    );

    assert_eq!(
        d.group(&stack),
        Some(&group_before),
        "the move wrote something onto the group itself: the region is DERIVED from the members' \
         cells on every render, and an anchor stored here is a second source of truth that \
         disagrees with them the moment one member is dragged"
    );

    let region: BTreeSet<Cell> = d
        .group_components(&stack)
        .into_iter()
        .flatten()
        .collect::<BTreeSet<Cell>>();
    let occupied: BTreeSet<Cell> = d
        .members(&stack)
        .iter()
        .map(|id| {
            d.cell_of(id)
                .expect("a member of the group has no cell of its own")
        })
        .collect();
    assert!(
        !region.is_empty(),
        "the group covers no cells at all, so the comparison below would hold against an \
         implementation that reports nothing"
    );
    assert_eq!(
        region, occupied,
        "the region the host is handed is not the set of cells the members are on: it is drawn by \
         growing each member's hexagon, so a region that lags its members paints ground under \
         cells nobody is standing on"
    );

    d.apply(inverse).unwrap_or_else(|e| {
        panic!(
            "the inverse `apply` handed back was itself refused with {e:?}: undo has to be \
             `apply(inverse)` or it is a second implementation of the move, free to disagree with \
             the first"
        )
    });
    for (name, home) in [
        ("north", cell(0, 0)),
        ("south", cell(0, 1)),
        ("east", cell(1, 0)),
    ] {
        assert_eq!(
            d.cell_of(&tile_id(name)),
            Some(home),
            "undo did not put {name} back where it started: the only export is a file the user \
             writes by hand, so work an undo fails to recover is work that is simply gone"
        );
    }
}

// ----------------------------------------------- contiguity, and adjacency

/// A rigid translation is an isometry, so the pieces of a group are the same
/// pieces afterwards. Asserted over every arrangement and every delta, because
/// it is the claim that makes stick-and-follow safe to leave unchecked.
#[test]
fn a_group_move_never_changes_group_connectivity() {
    let mut fixtures = shapes();
    // A deliberately split fixture, so the invariant is tested against a group
    // that has more than one piece. Without it every baseline is [n] and the
    // assertion holds for any implementation that answers "one piece, all of
    // them" — which is exactly what a degenerate flood fill answers.
    fixtures.push((
        "two pieces, deliberately",
        vec![cell(0, 0), cell(1, 0), cell(4, 0)],
    ));

    let stack = group_id("stack");
    let mut saw_a_split_fixture = false;

    for (name, cells) in fixtures {
        let baseline = component_sizes(&grouped(&cells), &stack);
        assert_eq!(
            baseline.iter().sum::<usize>(),
            cells.len(),
            "{name}: the pieces do not account for every member — a member in no piece is a cell \
             the host draws no ground under, and the group appears to have lost a tile"
        );
        if baseline.len() > 1 {
            saw_a_split_fixture = true;
        }

        for q in -3..=3 {
            for r in -3..=3 {
                if q == 0 && r == 0 {
                    continue;
                }
                let mut d = grouped(&cells);
                d.apply(Command::Translate {
                    grabbed: tile_id("member-0"),
                    delta: axial(q, r),
                    detach: false,
                })
                .unwrap_or_else(|e| {
                    panic!("{name} was refused delta ({q},{r}) on an empty lattice with {e:?}")
                });
                assert_eq!(
                    component_sizes(&d, &stack),
                    baseline,
                    "{name} came apart (or grew together) under axial delta ({q},{r}): a rigid \
                     translation cannot change which members touch, so a move that changes the \
                     pieces has moved members by different amounts"
                );
            }
        }
    }

    assert!(
        saw_a_split_fixture,
        "every fixture was a single piece, so this test never compared a multi-piece group with \
         itself and would pass against a flood fill that always answers 'one piece'"
    );
}

/// THE DECISION, WRITTEN AS A TEST. A group may be split, and the model says so
/// rather than preventing it: an editor that permits regrouping must permit the
/// state between pulling a member out and putting it back, and enforcing
/// contiguity at every step makes half of all legal rearrangements impossible.
#[test]
fn a_detach_may_fracture_a_group_and_that_is_reported_not_repaired() {
    let mut d = pinned(&[
        ("west", cell(0, 0), Some("row")),
        ("middle", cell(1, 0), Some("row")),
        ("east", cell(2, 0), Some("row")),
    ]);
    let row = group_id("row");

    assert_eq!(
        component_sizes(&d, &row),
        vec![3],
        "the fixture is not one joined piece to begin with, so the fracture below could not be \
         told from the state it started in"
    );

    // middle (1,0) pulled out to (0,1): adjacent to west, not to east, so the
    // group is left in two pieces whichever way a detached drop treats
    // membership — which is why the assertion is on the NUMBER of pieces and
    // not on their sizes.
    d.apply(Command::Translate {
        grabbed: tile_id("middle"),
        delta: axial(-1, 1),
        detach: true,
    })
    .unwrap_or_else(|e| {
        panic!(
            "pulling a member out of its group was refused with {e:?}: an editor that cannot \
             fracture a group cannot regroup at all, because taking a member out and putting it \
             back elsewhere necessarily passes through the split state"
        )
    });

    let pieces = component_sizes(&d, &row);
    assert_eq!(
        pieces.len(),
        2,
        "the split group reports {} piece(s): a host told it is whole draws one region over cells \
         that are no longer joined, and the user is shown a shape the diagram does not have",
        pieces.len()
    );

    assert_eq!(
        d.cell_of(&tile_id("middle")),
        Some(cell(0, 1)),
        "the detached tile is not where it was dropped"
    );
    for (name, home) in [("west", cell(0, 0)), ("east", cell(2, 0))] {
        assert_eq!(
            d.cell_of(&tile_id(name)),
            Some(home),
            "{name} moved during a DETACHED drag: detach narrows the moving set to the grabbed \
             tile alone, and a detach that still drags the group is a modifier that does nothing \
             visible except move tiles the user did not grab"
        );
    }

    assert_eq!(
        component_sizes(&d, &row),
        pieces,
        "the fracture healed between two reads: a model that quietly rejoins a split group undoes \
         the user's regrouping halfway through it, and does so with no command to undo"
    );
}

/// Adjacency between two groups is a fact about painting — their grounds will
/// merge — and the model REPORTS it. Refusing it would refuse diagrams that
/// already exist, and a refusal the user cannot see anything wrong with is the
/// worst kind.
#[test]
fn groups_that_come_to_touch_are_reported_and_never_refused() {
    let mut d = pinned(&[
        ("aa", cell(0, 0), Some("west-wing")),
        ("ab", cell(1, 0), Some("west-wing")),
        ("ba", cell(4, 0), Some("east-wing")),
        ("bb", cell(5, 0), Some("east-wing")),
    ]);

    assert!(
        d.touching_groups().is_empty(),
        "two groups three cells apart are already reported as touching, so the report below would \
         hold whatever the move did — and a host warning about merged grounds would cry wolf on \
         every diagram"
    );

    d.apply(Command::Translate {
        grabbed: tile_id("ba"),
        delta: axial(-2, 0),
        detach: false,
    })
    .unwrap_or_else(|e| {
        panic!(
            "a drop that only brings two groups side by side was refused with {e:?}: nothing \
             overlaps, so the user sees a refusal with no cause on screen — and a real published \
             diagram already contains such a pair, which this rule would make unloadable"
        )
    });

    let touching = d.touching_groups();
    assert_eq!(
        touching.len(),
        1,
        "{} adjacent group pair(s) reported where the two wings now share an edge: the host warns \
         that their grounds will merge, and a report that misses the pair means the merge arrives \
         unannounced",
        touching.len()
    );
    let named: BTreeSet<GroupId> = [touching[0].0.clone(), touching[0].1.clone()]
        .into_iter()
        .collect();
    let expected: BTreeSet<GroupId> = [group_id("west-wing"), group_id("east-wing")]
        .into_iter()
        .collect();
    assert_eq!(
        named, expected,
        "the reported pair does not name the two groups that touch, so the host's warning points \
         at the wrong regions"
    );
}

// ------------------------------------------------- trading places

/// The fixture every swap test below works on, laid out so that each rule the
/// guard enforces has a real pair to try it with.
///
/// ```text
///   row 0:   ana  bea          — group "north"
///   row 1:   cal  dot  eve     — cal,dot in "north"; eve in "south"
///   row 2:   fay  gus          — no group at all
/// ```
fn town() -> Diagram {
    pinned(&[
        ("ana", cell(0, 0), Some("north")),
        ("bea", cell(1, 0), Some("north")),
        ("cal", cell(0, 1), Some("north")),
        ("dot", cell(1, 1), Some("north")),
        ("eve", cell(2, 1), Some("south")),
        ("fay", cell(0, 2), None),
        ("gus", cell(1, 2), None),
    ])
}

/// The delta that takes `from` onto `to`, in the axial space a Translate speaks.
fn delta_between(from: Cell, to: Cell) -> Axial {
    to.to_axial().minus(from.to_axial())
}

fn drag(d: &Diagram, who: &str, onto: &str) -> Result<honeycomb_core::Plan, Rejection> {
    let (a, b) = (tile_id(who), tile_id(onto));
    d.check(&Command::Translate {
        grabbed: a.clone(),
        delta: delta_between(d.cell_of(&a).unwrap(), d.cell_of(&b).unwrap()),
        // A TILE DRAG IS ALWAYS DETACHED under the gesture this component now
        // has: a press on a hexagon moves that hexagon. A press on a group's
        // ground is the only thing that moves a cluster.
        detach: true,
    })
}

/// ONE TABLE, SO THE FENCE CANNOT BE DELETED WITHOUT DELETING THE FEATURE.
///
/// Swapping is deliberately the narrowest rule in this file: two tiles trade
/// places only when one tile was dragged onto exactly one blocker and the two
/// belong to the same group. Every other collision is still a refusal, and each
/// row below is one of the ways the guard could be loosened by accident.
#[test]
fn only_two_members_of_one_group_ever_trade_places() {
    let d = town();

    // (1) Two members of one group: the whole point of the feature.
    match drag(&d, "ana", "bea") {
        Ok(honeycomb_core::Plan::Exchange { a, b }) => {
            assert_eq!(a, tile_id("ana"));
            assert_eq!(b, tile_id("bea"));
        }
        other => panic!("two members of one group must trade places, got {other:?}"),
    }

    // (2) Two tiles with NO group. `None == None` is the absence of a group, not
    // a shared one — and reading it as a match would make swapping the default
    // behaviour of every diagram that has no groups at all.
    assert!(
        matches!(drag(&d, "fay", "gus"), Err(Rejection::Occupied { .. })),
        "two ungrouped tiles are not 'in the same group'"
    );

    // (3) Different groups: a refusal, still carrying its evidence.
    match drag(&d, "dot", "eve") {
        Err(Rejection::Occupied { blocked }) => {
            assert_eq!(blocked, vec![(cell(2, 1), tile_id("eve"))]);
        }
        other => panic!("across groups must refuse and name the blocker, got {other:?}"),
    }

    // (4) Grouped onto ungrouped, and (5) ungrouped onto grouped: neither is a
    // pair inside one group, so both refuse. Asserted in both directions because
    // a guard written with one `group_of` call and an `unwrap_or` would pass one
    // and fail the other.
    assert!(matches!(drag(&d, "dot", "gus"), Err(Rejection::Occupied { .. })));
    assert!(matches!(drag(&d, "gus", "dot"), Err(Rejection::Occupied { .. })));

    // (6) A GROUP drag that lands on a same-group tile is still a collision.
    // This is the row that fails if `moving.len() == 1` is ever dropped from the
    // guard in favour of "the tiles share a group": dragging the whole north
    // group one cell right puts ana onto bea, both in north.
    let ana = tile_id("ana");
    match d.check(&Command::Translate {
        grabbed: ana,
        delta: axial(2, 0),
        detach: false,
    }) {
        Err(Rejection::Occupied { blocked }) => {
            assert_eq!(blocked, vec![(cell(2, 1), tile_id("eve"))]);
        }
        other => panic!("a cluster sliding into another is a collision, not a swap: {other:?}"),
    }
}

/// `moving.len() == 1` implies `detach` for a grouped tile, so the guard needs
/// no `*detach &&` term. That is arithmetic about `moving_set`, and this is what
/// makes it a fact rather than a belief.
#[test]
fn a_single_tile_moving_set_is_exactly_a_detached_grab() {
    let d = town();
    for name in ["ana", "bea", "cal", "dot"] {
        let id = tile_id(name);
        assert_eq!(d.moving_set(&id, true).len(), 1, "{name} detached");
        assert!(
            d.moving_set(&id, false).len() > 1,
            "{name} attached must carry its group"
        );
    }
    // An ungrouped tile has a one-tile moving set either way, which is why the
    // group clauses and not this one are what keep ungrouped pairs out.
    assert_eq!(d.moving_set(&tile_id("fay"), false).len(), 1);

    // THE ONE CASE WHERE `moving.len() == 1` DOES NOT IMPLY `detach`: eve is the
    // only member of south, so an ATTACHED grab on it still carries one tile.
    // The guard survives it anyway, and for a reason worth writing down rather
    // than discovering: a sole member has no same-group tile to land on, so the
    // `ga == gb` clause can never be satisfied. Asserted, not assumed.
    assert_eq!(d.moving_set(&tile_id("eve"), false).len(), 1);
    let eve = tile_id("eve");
    for onto in ["dot", "gus"] {
        let target = d.cell_of(&tile_id(onto)).unwrap();
        assert!(
            matches!(
                d.check(&Command::Translate {
                    grabbed: eve.clone(),
                    delta: delta_between(d.cell_of(&eve).unwrap(), target),
                    detach: false,
                }),
                Err(Rejection::Occupied { .. })
            ),
            "a sole member dragged attached onto {onto} must not swap"
        );
    }
}

/// Both indexes, every tile, not just the two that moved.
///
/// The naive version of this test asserts `cell_of(a) == Some(cell_b)` and its
/// mirror, and PASSES on a `relocate` that has corrupted `occupancy` — because
/// `cell_of` reads `placement`, and the two maps only disagree in the direction
/// this checks second.
#[test]
fn a_swap_leaves_placement_and_occupancy_mutual_inverses() {
    let mut d = town();
    let before: BTreeSet<TileId> = d.cells().map(|(_, id)| id.clone()).collect();
    d.apply(Command::Translate {
        grabbed: tile_id("ana"),
        delta: delta_between(cell(0, 0), cell(1, 0)),
        detach: true,
    })
    .expect("two members of one group trade places");

    assert_eq!(d.cell_of(&tile_id("ana")), Some(cell(1, 0)));
    assert_eq!(d.cell_of(&tile_id("bea")), Some(cell(0, 0)));
    for id in &before {
        let at = d.cell_of(id).unwrap_or_else(|| panic!("{id:?} left the board"));
        assert_eq!(
            d.at(at),
            Some(id),
            "{id:?} is at {at:?} by placement, but occupancy says otherwise"
        );
    }
    let after: BTreeSet<TileId> = d.cells().map(|(_, id)| id.clone()).collect();
    assert_eq!(after, before, "a swap must not add or lose a tile");
}

/// THE KEYSTONE. The inverse a swap records has to survive the diagram changing
/// underneath it — and the obvious alternative, a negated Translate, does not.
///
/// Re-deriving the swap at undo time means asking `check` again, and by then the
/// pair may no longer share a group. `History::undo` pushes a refused command
/// back onto its stack and returns None, so every later undo retries the same
/// failure forever: the stack is wedged, with nothing on screen to explain it.
#[test]
fn the_inverse_of_a_swap_survives_a_regrouping_that_a_re_derived_one_could_not() {
    let mut d = town();
    let mut h = honeycomb_core::History::default();

    let inverse = d
        .apply(Command::Translate {
            grabbed: tile_id("ana"),
            delta: delta_between(cell(0, 0), cell(1, 0)),
            detach: true,
        })
        .expect("the swap applies");
    // It records a Swap, not a Translate. This assertion is the design decision.
    assert_eq!(
        inverse,
        Command::Swap {
            a: tile_id("ana"),
            b: tile_id("bea")
        }
    );
    h.record(inverse);

    // Now the pair stops sharing a group — the very next feature this editor is
    // going to grow.
    let detach_inverse = d.apply(Command::Detach { tile: tile_id("bea") }).unwrap();
    h.record(detach_inverse);

    assert!(h.undo(&mut d).is_some(), "the detach undoes");
    assert!(
        h.undo(&mut d).is_some(),
        "the swap must still undo after the membership changed"
    );
    assert_eq!(d.cell_of(&tile_id("ana")), Some(cell(0, 0)));
    assert_eq!(d.cell_of(&tile_id("bea")), Some(cell(1, 0)));

    // And the negated translate the rejected design would have recorded is
    // refused at exactly this point, which is what wedges the stack.
    let mut wedged = town();
    wedged
        .apply(Command::Translate {
            grabbed: tile_id("ana"),
            delta: delta_between(cell(0, 0), cell(1, 0)),
            detach: true,
        })
        .unwrap();
    wedged.apply(Command::Detach { tile: tile_id("bea") }).unwrap();
    assert!(
        matches!(
            wedged.check(&Command::Translate {
                grabbed: tile_id("ana"),
                delta: delta_between(cell(1, 0), cell(0, 0)),
                detach: true,
            }),
            Err(Rejection::Occupied { .. })
        ),
        "this is the refusal the recorded Swap exists to avoid"
    );
}

/// A `Swap` applied twice is the identity, for any pair, whatever their groups.
/// This is what makes `check`'s permissive `Swap` arm safe: the same-group rule
/// is about the GESTURE, and a host holding the command has already decided.
#[test]
fn swapping_the_same_pair_twice_is_the_identity_whatever_their_groups() {
    for (a, b) in [
        ("ana", "bea"), // same group
        ("dot", "eve"), // different groups
        ("dot", "gus"), // one grouped, one not
        ("fay", "gus"), // neither grouped
        ("ana", "gus"), // not even adjacent
    ] {
        let start = town();
        let mut d = start.clone();
        let echo = d
            .apply(Command::Swap {
                a: tile_id(a),
                b: tile_id(b),
            })
            .expect("a swap of two placed tiles applies");
        assert_eq!(
            echo,
            Command::Swap {
                a: tile_id(a),
                b: tile_id(b)
            },
            "a swap returns itself as its own inverse"
        );
        d.apply(echo).unwrap();
        assert_eq!(d, start, "{a} <-> {b} twice must be the identity");
    }
}

/// No plan `check` can produce ever names one cell twice.
///
/// That is the one corruption `relocate` cannot survive: the loser of a
/// duplicate keeps a `placement` entry that `occupancy` contradicts, and
/// `cells()` reads `occupancy`, so the tile vanishes from the render and from
/// the serialisation with nothing returning an error. The `Plan` enum is what
/// makes it unrepresentable; this is what checks the claim across every plan the
/// rules can actually reach.
#[test]
fn every_plan_check_can_return_names_each_cell_at_most_once() {
    let d = town();
    let names = ["ana", "bea", "cal", "dot", "eve", "fay", "gus"];
    let mut exchanges = 0;
    for name in names {
        let id = tile_id(name);
        for q in -3..=3 {
            for r in -3..=3 {
                for detach in [true, false] {
                    let Ok(plan) = d.check(&Command::Translate {
                        grabbed: id.clone(),
                        delta: axial(q, r),
                        detach,
                    }) else {
                        continue;
                    };
                    if matches!(plan, honeycomb_core::Plan::Exchange { .. }) {
                        exchanges += 1;
                    }
                    let moves = plan.moves(&d);
                    let targets: BTreeSet<Cell> = moves.iter().map(|(_, c)| *c).collect();
                    assert_eq!(
                        targets.len(),
                        moves.len(),
                        "{name} by ({q},{r}) detach={detach} plans two tiles into one cell"
                    );
                    // And every occupied target is vacated by this same plan.
                    for (_, to) in &moves {
                        if let Some(occ) = d.at(*to) {
                            assert!(
                                moves.iter().any(|(id, _)| id == occ),
                                "{name} by ({q},{r}) lands on {occ:?}, which is not moving"
                            );
                        }
                    }
                }
            }
        }
    }
    assert!(
        exchanges > 0,
        "the sweep never reached an Exchange, so it proves nothing about swaps"
    );
}

/// A DRAG CHANGES A GROUP'S PIECE COUNT AND NEVER ITS MEMBERSHIP — both the
/// ordinary case, a tile dragged clear, and the swap case, which is the one the
/// name used to promise and the body used not to contain.
#[test]
fn a_drag_may_change_a_groups_piece_count_and_never_its_membership() {
    let north = group_id("north");

    // (a) A TILE DRAGGED CLEAR. Routine now that a tile press detaches, and the
    // reason fracture stopped being an edge case.
    let mut d = town();
    assert_eq!(component_sizes(&d, &north), vec![4]);
    d.apply(Command::Translate {
        grabbed: tile_id("ana"),
        delta: delta_between(cell(0, 0), cell(4, 4)),
        detach: true,
    })
    .expect("a tile may be dragged clear of its own cluster");
    assert_eq!(component_sizes(&d, &north), vec![1, 3], "north is in two pieces");
    assert_eq!(
        d.group_of(&tile_id("ana")),
        Some(&north),
        "and ana is still a member — a drag never changes membership"
    );

    // (b) A SWAP, which is what this test is named for and did not contain. Put
    // ana back where a swap with a far member is possible: after (a), ana sits
    // alone at (4,4) and cal is still in the block, so trading them moves the
    // hole rather than closing it — the piece count is preserved, membership is
    // untouched, and BOTH tiles moved.
    let before: Vec<usize> = component_sizes(&d, &north);
    let inverse = d
        .apply(Command::Swap {
            a: tile_id("ana"),
            b: tile_id("cal"),
        })
        .expect("two placed tiles may be exchanged");
    assert_eq!(
        inverse,
        Command::Swap {
            a: tile_id("ana"),
            b: tile_id("cal")
        }
    );
    assert_eq!(d.cell_of(&tile_id("ana")), Some(cell(0, 1)));
    assert_eq!(d.cell_of(&tile_id("cal")), Some(cell(4, 4)));
    assert_eq!(
        component_sizes(&d, &north),
        before,
        "exchanging two members of one group moves the hole, it does not fill it"
    );
    for who in ["ana", "cal"] {
        assert_eq!(
            d.group_of(&tile_id(who)),
            Some(&north),
            "{who} lost its group to a swap"
        );
    }

    // (c) AND A SWAP REACHED THROUGH THE GESTURE, not through the command: the
    // path a user actually takes. ana at (0,1) is adjacent to dot at (1,1).
    let plan = drag(&d, "ana", "dot").expect("adjacent members of one group trade places");
    assert_eq!(plan.displaced(), Some(&tile_id("dot")));
}

// ------------------------------------------------- the palette's two verbs

/// A pinned tile for the fixture's own namespace.
fn pinned_tile(name: &str, group: Option<&str>) -> honeycomb_core::NewTile {
    honeycomb_core::NewTile::Pinned(PinnedTile {
        group: group.map(group_id),
        represents: Iri(format!("https://example.org/host/{name}")),
    })
}

/// A standalone fixture, because three of the rules below are only REACHABLE in
/// standalone mode: `EmptyLabel` needs a tile that has a label, and `WrongMode`
/// needs both directions.
fn hamlet() -> Diagram {
    let mut groups: BTreeMap<GroupId, Group> = BTreeMap::new();
    groups.insert(
        group_id("north"),
        Group {
            label: "North".into(),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    let mut tiles: BTreeMap<TileId, honeycomb_core::OwnTile> = BTreeMap::new();
    let mut cells: BTreeMap<TileId, Cell> = BTreeMap::new();
    for (name, col, row) in [("ana", 0, 0), ("bea", 1, 0)] {
        tiles.insert(
            tile_id(name),
            honeycomb_core::OwnTile {
                group: Some(group_id("north")),
                label: name.to_string(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
        cells.insert(tile_id(name), cell(col, row));
    }
    Diagram::try_new(DiagramSpec {
        slug: slug("hamlet"),
        label: "Hamlet".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups,
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .expect("the standalone fixture is legal")
}

fn own_tile(label: &str, group: Option<&str>) -> honeycomb_core::NewTile {
    honeycomb_core::NewTile::Own(Box::new(honeycomb_core::OwnTile {
        group: group.map(group_id),
        label: label.to_string(),
        comment: None,
        style_key: None,
        extra: Vec::new(),
    }))
}

/// THE HEADLINE. Over the WHOLE `Diagram`, groups included — because the one
/// thing an Add must not do is leave a trace behind after its own undo, and the
/// trace it would most plausibly leave is a group it declared on the way in.
#[test]
fn add_then_remove_is_the_identity_on_the_whole_diagram() {
    let start = town();
    let mut d = start.clone();
    let back = d
        .apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("north")),
        })
        .expect("an empty cell and a declared group");
    assert_eq!(back, Command::Remove { tile: tile_id("hal") });
    assert_ne!(d, start, "the add changed nothing");
    d.apply(back).expect("the recorded inverse applies");
    assert_eq!(d, start, "add then remove is not the identity");
}

/// The other direction, and specifically that the inverse is SELF-CONTAINED: it
/// carries the cell and the content, so it needs no evidence from a diagram that
/// no longer has them.
#[test]
fn remove_then_its_inverse_restores_the_cell_the_content_and_the_group() {
    let start = town();
    let mut d = start.clone();
    let back = d.apply(Command::Remove { tile: tile_id("eve") }).unwrap();
    match &back {
        Command::Add { tile, at, what } => {
            assert_eq!(tile, &tile_id("eve"));
            assert_eq!(*at, cell(2, 1));
            assert_eq!(what.group(), Some(&group_id("south")));
        }
        other => panic!("a Remove's inverse must be a self-contained Add, got {other:?}"),
    }
    assert!(d.cell_of(&tile_id("eve")).is_none());
    d.apply(back).unwrap();
    assert_eq!(d, start);
}

/// `ModelError::NoPlacements` refuses an empty diagram at construction, and its
/// doc used to justify that with "the command set has no delete". There is one
/// now; this is what keeps the sentence true.
#[test]
fn removing_the_last_placement_is_refused_and_changes_nothing() {
    let one = pinned(&[("solo", cell(0, 0), None)]);
    let mut d = one.clone();
    assert_eq!(
        d.apply(Command::Remove { tile: tile_id("solo") }),
        Err(Rejection::LastPlacement)
    );
    assert_eq!(d, one, "a refused remove must leave the diagram alone");
}

/// THE FORK RESOLUTION, AS A PAIR. An undeclared group is refused exactly the way
/// `Attach` refuses one; a DECLARED but memberless group is accepted. The second
/// assertion is the one the portal depends on — it pre-declares every product
/// group so this refusal is unreachable there.
#[test]
fn an_add_naming_an_undeclared_group_is_refused_the_way_an_attach_is() {
    let d = town();
    assert_eq!(
        d.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("westside")),
        }),
        Err(Rejection::UnknownGroup(group_id("westside")))
    );
    assert_eq!(
        d.check(&Command::Attach {
            tile: tile_id("ana"),
            group: group_id("westside"),
        }),
        Err(Rejection::UnknownGroup(group_id("westside"))),
        "the two ways in must refuse identically"
    );

    // A group with a member removed is still declared, so an Add into it lands.
    let mut d = town();
    d.apply(Command::Remove { tile: tile_id("eve") }).unwrap();
    assert!(d.has_group(&group_id("south")), "south lost its last member");
    assert!(
        d.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("south")),
        })
        .is_ok(),
        "a declared group with no members must still be joinable"
    );
}

/// THE LOAD-BEARING TEST OF THE DESIGN. If a Remove undeclared an emptied group,
/// this undo would have to redeclare it — and a redeclare is wrong the moment an
/// Attach has since put another tile in.
#[test]
fn a_group_survives_losing_its_last_member_so_an_undo_can_put_it_back() {
    let start = town();
    let mut d = start.clone();
    let mut h = honeycomb_core::History::default();

    let back = d.apply(Command::Remove { tile: tile_id("eve") }).unwrap();
    h.record(back);
    assert!(d.has_group(&group_id("south")));
    assert!(d.groups().contains_key(&group_id("south")));

    assert!(h.undo(&mut d).is_some(), "the remove undoes");
    assert_eq!(d, start);
}

/// THE INVARIANT, NARROWED BUT NOT WEAKENED. It used to read "no command in the
/// enum may change the set of groups a diagram declares", and three commands
/// now exist whose entire purpose is to change it. What the property was
/// actually protecting survives verbatim: `Add` refuses an undeclared group
/// without ever needing to declare one, and `Remove` leaves an emptied group
/// standing so its undo can put a tile back. So the sweep covers every command
/// EXCEPT the three group verbs, and `the_group_verbs_are_the_only_way_to_change_the_declared_set`
/// covers those.
#[test]
fn the_declared_groups_are_invariant_under_every_command() {
    let start = town();
    let want: Vec<GroupId> = start.groups().keys().cloned().collect();
    let commands = vec![
        Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(4, 4),
            detach: true,
        },
        Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(0, 4),
            detach: false,
        },
        Command::Swap {
            a: tile_id("ana"),
            b: tile_id("dot"),
        },
        Command::Attach {
            tile: tile_id("fay"),
            group: group_id("north"),
        },
        Command::Detach { tile: tile_id("ana") },
        Command::Add {
            tile: tile_id("hal"),
            at: cell(6, 6),
            what: pinned_tile("hal", Some("south")),
        },
        Command::Remove { tile: tile_id("eve") },
    ];
    for cmd in commands {
        let mut d = start.clone();
        let back = d
            .apply(cmd.clone())
            .unwrap_or_else(|e| panic!("{cmd:?} was refused: {e:?}"));
        assert_eq!(
            d.groups().keys().cloned().collect::<Vec<_>>(),
            want,
            "{cmd:?} changed the declared groups"
        );
        d.apply(back).unwrap();
        assert_eq!(d, start, "{cmd:?} did not undo cleanly");
    }
}

/// A CONTENT-ONLY ORPHAN PANICS THE EDITOR: `members` iterates content, and
/// `check`'s Translate arm calls `.expect("a member of the moving set is
/// placed, by construction")` on every one of them.
#[test]
fn content_placement_and_occupancy_stay_one_fact_across_an_add_and_a_remove() {
    let mut d = town();
    let check = |d: &Diagram, when: &str| {
        let content: BTreeSet<TileId> = d.content().ids().cloned().collect();
        let placed: BTreeSet<TileId> = d.cells().map(|(_, id)| id.clone()).collect();
        assert_eq!(content, placed, "content and placement disagree {when}");
        for id in &placed {
            let at = d.cell_of(id).unwrap();
            assert_eq!(d.at(at), Some(id), "occupancy is not placement's inverse {when}");
        }
    };
    check(&d, "at the start");
    let back = d
        .apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", None),
        })
        .unwrap();
    check(&d, "after an add");
    d.apply(back).unwrap();
    check(&d, "after the undo");
    d.apply(Command::Remove { tile: tile_id("fay") }).unwrap();
    check(&d, "after a remove");
}

#[test]
fn an_add_of_an_id_already_on_the_board_is_already_placed_not_occupied() {
    let d = town();
    assert_eq!(
        d.check(&Command::Add {
            tile: tile_id("ana"),
            at: cell(9, 9),
            what: pinned_tile("ana", None),
        }),
        Err(Rejection::AlreadyPlaced(tile_id("ana"))),
        "a duplicate id is about the ID, not about a cell"
    );
}

#[test]
fn an_add_onto_an_occupied_cell_carries_the_occupant_as_evidence() {
    let d = town();
    assert_eq!(
        d.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(1, 1),
            what: pinned_tile("hal", None),
        }),
        Err(Rejection::Occupied {
            blocked: vec![(cell(1, 1), tile_id("dot"))]
        }),
        "a host outlines the blocker, so the refusal has to name it"
    );
}

#[test]
fn an_own_tile_cannot_be_added_to_a_pinned_diagram_nor_a_pinned_tile_to_a_standalone_one() {
    let p = town();
    let mut d = p.clone();
    assert_eq!(
        d.apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: own_tile("Hal", None),
        }),
        Err(Rejection::WrongMode {
            diagram: honeycomb_core::Mode::Pinned,
            offered: honeycomb_core::Mode::Standalone,
        })
    );
    assert_eq!(d, p, "a refused add must touch neither index");

    let h = hamlet();
    let mut d = h.clone();
    assert_eq!(
        d.apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", None),
        }),
        Err(Rejection::WrongMode {
            diagram: honeycomb_core::Mode::Standalone,
            offered: honeycomb_core::Mode::Pinned,
        })
    );
    assert_eq!(d, h);
}

/// `try_new`'s check repeated where the tile now arrives — and reachable on day
/// one, because the demo is standalone and its palette hands out `OwnTile`s.
#[test]
fn an_add_of_an_own_tile_with_a_blank_label_is_refused() {
    let d = hamlet();
    for blank in ["", "   ", "\t"] {
        assert_eq!(
            d.check(&Command::Add {
                tile: tile_id("hal"),
                at: cell(4, 4),
                what: own_tile(blank, Some("north")),
            }),
            Err(Rejection::EmptyLabel(tile_id("hal"))),
            "{blank:?} is not a label"
        );
    }
}

/// `check` runs on every pointer move so a refusal can be painted before the
/// release. One that mutated would drag the document along with the pointer.
#[test]
fn checking_an_add_or_a_remove_leaves_the_diagram_untouched() {
    let untouched = town();
    let d = town();
    let _ = d.check(&Command::Add {
        tile: tile_id("hal"),
        at: cell(4, 4),
        what: pinned_tile("hal", Some("north")),
    });
    let _ = d.check(&Command::Remove { tile: tile_id("ana") });
    let _ = d.check(&Command::Remove { tile: tile_id("nobody") });
    assert_eq!(d, untouched);
}

/// The keystone's sibling. Under any design where a Remove undeclares the group
/// it emptied, this undo is refused.
#[test]
fn an_add_and_its_undo_survive_an_attach_in_between() {
    let mut d = town();
    let mut h = honeycomb_core::History::default();

    h.record(
        d.apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("south")),
        })
        .unwrap(),
    );
    h.record(
        d.apply(Command::Attach {
            tile: tile_id("fay"),
            group: group_id("south"),
        })
        .unwrap(),
    );

    assert!(h.undo(&mut d).is_some(), "the attach undoes");
    assert!(h.undo(&mut d).is_some(), "and so must the add");
    assert!(d.cell_of(&tile_id("hal")).is_none());
}

/// BYTES, not structure — the document is what a consumer's SHACL gate reads.
#[test]
fn a_full_undo_redo_cycle_restores_the_document_byte_for_byte() {
    let opts = honeycomb_core::WriteOpts::new(
        "https://example.org/d/",
        "data",
        "https://example.org/d/",
    )
    .unwrap();
    let mut d = town();
    let before = honeycomb_core::write_turtle(&d, &opts);
    let mut h = honeycomb_core::History::default();

    h.record(
        d.apply(Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("north")),
        })
        .unwrap(),
    );
    h.record(d.apply(Command::Remove { tile: tile_id("gus") }).unwrap());
    assert_ne!(honeycomb_core::write_turtle(&d, &opts), before);

    h.undo(&mut d).unwrap();
    h.undo(&mut d).unwrap();
    assert_eq!(honeycomb_core::write_turtle(&d, &opts), before, "undo");
    h.redo(&mut d).unwrap();
    h.redo(&mut d).unwrap();
    assert_ne!(honeycomb_core::write_turtle(&d, &opts), before);
    h.undo(&mut d).unwrap();
    h.undo(&mut d).unwrap();
    assert_eq!(honeycomb_core::write_turtle(&d, &opts), before, "and again");
}

/// THE TRANSLATE COUNTERPART OF `the_inverse_of_a_swap_survives_a_regrouping…`.
///
/// A group translate used to record the opposite translate, which is not a value
/// but a RULE for re-deriving one — `moving_set` reads membership at undo time.
/// So a tile attached to the group afterwards was dragged by the undo of a
/// command applied before it joined, and a tile ADDED afterwards was dragged to
/// a cell it had never occupied.
#[test]
fn undoing_a_group_drag_never_touches_a_tile_that_joined_afterwards() {
    let mut d = town();
    let mut h = honeycomb_core::History::default();

    // north is ana,bea,cal,dot. Slide it one row down, out of everyone's way.
    h.record(
        d.apply(Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(0, 3),
            detach: false,
        })
        .unwrap(),
    );
    let after_drag: Vec<(TileId, Cell)> =
        d.cells().map(|(c, id)| (id.clone(), c)).collect();

    // Now two tiles JOIN north, by the two public routes.
    d.apply(Command::Attach {
        tile: tile_id("fay"),
        group: group_id("north"),
    })
    .unwrap();
    d.apply(Command::Add {
        tile: tile_id("hal"),
        at: cell(7, 7),
        what: pinned_tile("hal", Some("north")),
    })
    .unwrap();
    let fay_at = d.cell_of(&tile_id("fay")).unwrap();

    assert!(h.undo(&mut d).is_some(), "the group drag must undo");

    // The four that moved are back.
    for (id, _) in &after_drag {
        if ["ana", "bea", "cal", "dot"].contains(&id.0.as_str()) {
            assert_eq!(
                d.cell_of(id),
                town().cell_of(id),
                "{id:?} did not return to where it started"
            );
        }
    }
    // And the two that joined afterwards did not move at all.
    assert_eq!(d.cell_of(&tile_id("fay")), Some(fay_at), "fay was dragged by an undo");
    assert_eq!(
        d.cell_of(&tile_id("hal")),
        Some(cell(7, 7)),
        "hal was dragged to a cell it has never occupied"
    );
}

/// A `Restore` moves exactly the tiles it names, and its own inverse puts them
/// back — so undo/redo over a group drag is stable however the group changes.
#[test]
fn a_restore_is_exactly_reversible_and_moves_nothing_it_does_not_name() {
    let start = town();
    let mut d = start.clone();
    let back = d
        .apply(Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(0, 3),
            detach: false,
        })
        .unwrap();
    let Command::Restore { cells } = &back else {
        panic!("a translate must record a Restore, got {back:?}");
    };
    assert_eq!(cells.len(), 4, "north has four members");
    for (id, at) in cells {
        assert_eq!(start.cell_of(id), Some(*at), "{id:?} recorded the wrong cell");
    }

    let forward = d.apply(back).expect("the restore applies");
    assert_eq!(d, start, "restore did not return the diagram");
    d.apply(forward).expect("and its own inverse re-applies");
    assert_ne!(d, start);
}

/// A restore whose cell somebody else has taken is REFUSED, with evidence —
/// which is a true statement about the board, rather than the old behaviour of
/// quietly moving whatever the group happens to contain now.
#[test]
fn a_restore_onto_an_occupied_cell_is_refused_and_names_the_blocker() {
    let mut d = town();
    let back = d
        .apply(Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(0, 3),
            detach: false,
        })
        .unwrap();
    // Somebody parks a new tile on a cell the group used to hold.
    d.apply(Command::Add {
        tile: tile_id("hal"),
        at: cell(0, 0),
        what: pinned_tile("hal", None),
    })
    .unwrap();
    match d.check(&back) {
        Err(Rejection::Occupied { blocked }) => {
            assert_eq!(blocked, vec![(cell(0, 0), tile_id("hal"))]);
        }
        other => panic!("expected a refusal naming hal, got {other:?}"),
    }
}

// ------------------------------------------------- the shape of a region

/// A GROUP WITH A HOLE IS ONE SHAPE; A GROUP IN PIECES IS NOT.
///
/// `holes` is what lets a region be drawn as one outline without a distance
/// threshold that would have to guess where a gap stops being a hole. The rule
/// is topological instead: what a flood fill from outside cannot reach.
#[test]
fn a_ring_of_cells_encloses_its_centre_and_two_pieces_enclose_nothing() {
    use honeycomb_core::holes;

    // The six neighbours of (2,2), with (2,2) itself left out.
    let centre = cell(2, 2);
    let ring: BTreeSet<Cell> = centre.neighbours().into_iter().collect();
    assert_eq!(ring.len(), 6);
    assert_eq!(
        holes(&ring),
        [centre].into_iter().collect::<BTreeSet<_>>(),
        "a cell with all six neighbours occupied is enclosed"
    );

    // Take one away and the centre has a way out.
    let mut gapped = ring.clone();
    let dropped = *gapped.iter().next().unwrap();
    gapped.remove(&dropped);
    assert!(
        holes(&gapped).is_empty(),
        "five of six neighbours leaves a way out, so nothing is enclosed"
    );

    // Two separate clusters enclose nothing between them, however close.
    let apart: BTreeSet<Cell> = [cell(0, 0), cell(1, 0), cell(5, 0), cell(6, 0)]
        .into_iter()
        .collect();
    assert!(
        holes(&apart).is_empty(),
        "a gap between two pieces is not a hole, and filling it would hide a fracture"
    );

    // An empty set, and a single cell.
    assert!(holes(&BTreeSet::new()).is_empty());
    assert!(holes(&[cell(0, 0)].into_iter().collect()).is_empty());
}

/// Filling holes must not change what `components` says: the pieces are counted
/// over the MEMBERS, and a hole has no member in it.
#[test]
fn filling_a_hole_does_not_change_how_many_pieces_a_group_is_in() {
    use honeycomb_core::{components, holes};
    let centre = cell(2, 2);
    let ring: BTreeSet<Cell> = centre.neighbours().into_iter().collect();
    assert_eq!(components(ring.clone()).len(), 1, "the ring is connected");
    let filled: BTreeSet<Cell> = ring.union(&holes(&ring)).copied().collect();
    assert_eq!(components(filled).len(), 1);
    assert_eq!(components(ring).len(), 1);
}

// ------------------------------------------------- the three group verbs

fn a_group(label: &str) -> Group {
    Group {
        label: label.to_string(),
        style_key: None,
        note: None,
        extra: Vec::new(),
    }
}

/// Declare, edit, undeclare — and each one's inverse puts the diagram back.
#[test]
fn the_group_verbs_are_the_only_way_to_change_the_declared_set() {
    let start = town();
    let west = group_id("westside");

    // DECLARE. An Add into it was refused a moment ago; now it lands.
    let mut d = start.clone();
    assert!(matches!(
        d.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("westside")),
        }),
        Err(Rejection::UnknownGroup(_))
    ));
    let back = d
        .apply(Command::DeclareGroup {
            id: west.clone(),
            group: a_group("Westside"),
        })
        .unwrap();
    assert_eq!(back, Command::RemoveGroup { id: west.clone() });
    assert!(d.has_group(&west));
    assert!(
        d.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: pinned_tile("hal", Some("westside")),
        })
        .is_ok(),
        "a group declared after construction must be joinable"
    );

    // Declaring it twice is a refusal, not a silent overwrite of its label.
    assert_eq!(
        d.check(&Command::DeclareGroup {
            id: west.clone(),
            group: a_group("Something else"),
        }),
        Err(Rejection::AlreadyDeclared(west.clone()))
    );

    // EDIT. The inverse carries the WHOLE previous value, so a form that wrote
    // one field cannot lose the other two.
    let edited = Group {
        label: "West Side".into(),
        style_key: Some(Iri("https://example.org/style/west".into())),
        note: Some("three of these".into()),
        extra: Vec::new(),
    };
    let back_edit = d
        .apply(Command::EditGroup {
            id: west.clone(),
            group: edited.clone(),
        })
        .unwrap();
    assert_eq!(d.group(&west), Some(&edited));
    assert_eq!(
        back_edit,
        Command::EditGroup {
            id: west.clone(),
            group: a_group("Westside")
        }
    );
    d.apply(back_edit).unwrap();
    assert_eq!(d.group(&west), Some(&a_group("Westside")));

    // UNDECLARE, and only when empty.
    d.apply(back).unwrap();
    assert!(!d.has_group(&west));
    assert_eq!(d, start, "the three verbs did not round-trip");
}

/// A group cannot be undeclared while tiles still name it — the refusal names
/// them, because "not empty" with nothing to point at is not actionable.
#[test]
fn undeclaring_a_group_that_still_has_members_is_refused_with_their_names() {
    let mut d = town();
    match d.check(&Command::RemoveGroup {
        id: group_id("north"),
    }) {
        Err(Rejection::GroupInUse { group, members }) => {
            assert_eq!(group, group_id("north"));
            assert_eq!(members.len(), 4, "north has four members");
            assert!(members.contains(&tile_id("ana")));
        }
        other => panic!("expected GroupInUse naming the members, got {other:?}"),
    }
    // Emptied, it goes.
    for who in ["ana", "bea", "cal", "dot"] {
        d.apply(Command::Detach { tile: tile_id(who) }).unwrap();
    }
    assert!(d.apply(Command::RemoveGroup { id: group_id("north") }).is_ok());
    assert!(!d.has_group(&group_id("north")));
}

/// A GROUP MAY HAVE NO LABEL. The ground is often the whole signal, and a
/// heading beside an obvious cluster says the same thing twice. A TILE still may
/// not: one with no label draws as an empty hexagon, which is nothing at all.
#[test]
fn a_group_may_be_unnamed_and_a_tile_may_not() {
    let mut d = town();
    assert!(
        d.apply(Command::DeclareGroup {
            id: group_id("quiet"),
            group: a_group(""),
        })
        .is_ok(),
        "a group with no label was refused"
    );
    assert!(
        d.apply(Command::EditGroup {
            id: group_id("north"),
            group: a_group("   "),
        })
        .is_ok(),
        "a group cannot be renamed to nothing"
    );
    // The tile rule is unchanged — see `an_add_of_an_own_tile_with_a_blank_label_is_refused`.
    let h = hamlet();
    assert!(matches!(
        h.check(&Command::Add {
            tile: tile_id("hal"),
            at: cell(4, 4),
            what: own_tile("", Some("north")),
        }),
        Err(Rejection::EmptyLabel(_))
    ));
}

// ------------------------------------------------- links

fn a_link(from: &str, to: &str) -> honeycomb_core::Link {
    honeycomb_core::Link {
        from: tile_id(from),
        to: tile_id(to),
        label: None,
        routing: honeycomb_core::Routing::Straight,
        style_key: None,
        extra: Vec::new(),
    }
}

fn link_id(s: &str) -> honeycomb_core::LinkId {
    honeycomb_core::LinkId(slug(s))
}

/// Connect and disconnect are each other's exact inverse, over the whole diagram.
#[test]
fn connect_then_disconnect_is_the_identity_and_moves_no_cell() {
    let start = town();
    let mut d = start.clone();
    let cells_before: Vec<(Cell, TileId)> =
        d.cells().map(|(c, id)| (c, id.clone())).collect();

    let back = d
        .apply(Command::Connect {
            id: link_id("one"),
            link: a_link("ana", "eve"),
        })
        .expect("two placed tiles may be connected");
    assert_eq!(back, Command::Disconnect { id: link_id("one") });
    assert_eq!(d.links().len(), 1);
    // A LINK MOVES NOTHING. The arrangement is untouched; only what the drawing
    // asserts has changed.
    assert_eq!(
        d.cells().map(|(c, id)| (c, id.clone())).collect::<Vec<_>>(),
        cells_before,
        "a link moved a cell"
    );

    d.apply(back).unwrap();
    assert_eq!(d, start, "connect then disconnect is not the identity");
}

/// Every way a link can fail to be drawable, refused by name.
#[test]
fn a_link_needs_two_different_placed_tiles_and_an_unused_id() {
    let mut d = town();
    assert_eq!(
        d.check(&Command::Connect {
            id: link_id("one"),
            link: a_link("ana", "ana"),
        }),
        Err(Rejection::NotDrawable(link_id("one"))),
        "a link to itself has no direction and no length"
    );
    assert_eq!(
        d.check(&Command::Connect {
            id: link_id("one"),
            link: a_link("ana", "nobody"),
        }),
        Err(Rejection::UnknownTile(tile_id("nobody"))),
        "a link to a tile the diagram does not place is a line to nowhere"
    );
    d.apply(Command::Connect {
        id: link_id("one"),
        link: a_link("ana", "eve"),
    })
    .unwrap();
    assert_eq!(
        d.check(&Command::Connect {
            id: link_id("one"),
            link: a_link("bea", "fay"),
        }),
        Err(Rejection::AlreadyConnected(link_id("one"))),
        "reusing an id would silently replace a line somebody drew"
    );
    assert_eq!(
        d.check(&Command::Disconnect {
            id: link_id("nothing")
        }),
        Err(Rejection::UnknownLink(link_id("nothing")))
    );
}

/// REMOVING A LINKED TILE IS REFUSED, NAMING THE LINKS. Cascading would mean
/// `Add` carrying links back for one case out of many, and a user who did not
/// notice them go would not notice them return.
#[test]
fn a_tile_with_links_cannot_be_removed_until_they_are() {
    let mut d = town();
    d.apply(Command::Connect {
        id: link_id("one"),
        link: a_link("ana", "eve"),
    })
    .unwrap();
    d.apply(Command::Connect {
        id: link_id("two"),
        link: a_link("bea", "ana"),
    })
    .unwrap();

    match d.check(&Command::Remove { tile: tile_id("ana") }) {
        Err(Rejection::StillLinked { tile, links }) => {
            assert_eq!(tile, tile_id("ana"));
            assert_eq!(links, vec![link_id("one"), link_id("two")]);
        }
        other => panic!("expected StillLinked naming both links, got {other:?}"),
    }
    // A tile at neither end is unaffected.
    assert!(d.check(&Command::Remove { tile: tile_id("gus") }).is_ok());

    for id in [link_id("one"), link_id("two")] {
        d.apply(Command::Disconnect { id }).unwrap();
    }
    assert!(d.apply(Command::Remove { tile: tile_id("ana") }).is_ok());
}

/// A link survives its endpoints moving, swapping and changing groups — it names
/// tiles, not cells, so the arrangement is free to change underneath it.
#[test]
fn a_link_follows_its_endpoints_wherever_they_go() {
    let mut d = town();
    d.apply(Command::Connect {
        id: link_id("one"),
        link: a_link("ana", "eve"),
    })
    .unwrap();
    for cmd in [
        Command::Translate {
            grabbed: tile_id("ana"),
            delta: axial(4, 4),
            detach: true,
        },
        Command::Swap {
            a: tile_id("ana"),
            b: tile_id("gus"),
        },
        Command::Detach { tile: tile_id("ana") },
    ] {
        d.apply(cmd.clone()).unwrap_or_else(|e| panic!("{cmd:?}: {e:?}"));
        assert_eq!(
            d.link(&link_id("one")).map(|l| (&l.from, &l.to)),
            Some((&tile_id("ana"), &tile_id("eve"))),
            "the link changed when the arrangement did"
        );
    }
}

/// A lattice route goes round what is in the way, and says so when it cannot.
#[test]
fn a_route_avoids_occupied_cells_and_reports_when_there_is_no_way_through() {
    use honeycomb_core::route;

    let clear = route(cell(0, 0), cell(3, 0), &BTreeSet::new(), 3).expect("an empty board");
    assert_eq!(clear.first(), Some(&cell(0, 0)));
    assert_eq!(clear.last(), Some(&cell(3, 0)));
    // Every step is a neighbour of the last: a route with a jump in it is not a
    // route, and a polyline drawn through it would cut across cells.
    for pair in clear.windows(2) {
        assert!(
            pair[0].is_neighbour(pair[1]),
            "{:?} and {:?} are not adjacent",
            pair[0],
            pair[1]
        );
    }

    // A wall with a gap: the route is longer and still adjacent throughout.
    let wall: BTreeSet<Cell> = (-2..=2).map(|row| cell(2, row)).collect();
    let round = route(cell(0, 0), cell(4, 0), &wall, 6).expect("there is a way round");
    assert!(round.len() > clear.len(), "it did not detour");
    assert!(round.iter().all(|c| !wall.contains(c)), "it went through the wall");

    // Sealed in: BFS returns None rather than a path through a tile.
    let sealed: BTreeSet<Cell> = cell(0, 0).neighbours().into_iter().collect();
    assert_eq!(
        route(cell(0, 0), cell(5, 5), &sealed, 8),
        None,
        "a host must fall back to a straight line rather than draw nothing"
    );

    // BOTH ENDS ARE EXEMPT FROM `blocked`, because a link runs between two
    // OCCUPIED cells by definition — without the exemption every search starts
    // inside a wall and returns nothing.
    let both: BTreeSet<Cell> = [cell(0, 0), cell(1, 0)].into_iter().collect();
    assert_eq!(
        route(cell(0, 0), cell(1, 0), &both, 3),
        Some(vec![cell(0, 0), cell(1, 0)])
    );
}
