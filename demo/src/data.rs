//! The dummy dataset: a town plan. Twelve tiles, three districts.
//!
//! Generic on purpose — no product names, no real deployment data, nothing
//! anyone could mistake for a record of something. "A district moves as one"
//! needs no explaining to a stranger, which is the whole job of demo data.
//!
//! IT IS A hive:standalone DIAGRAM, and that is the only mode a demo can
//! honestly be. A pinned diagram carries no labels at all — every word on the
//! hexagons comes from the external source it lays out — so a pinned demo would
//! draw twelve empty cells unless this file also invented the source, which is
//! exactly the "a picture of a thing that never existed" trap the static render
//! already has to avoid.
//!
//! It is a plain `fn` of nothing: no clock, no randomness, no environment, no
//! locale. That is a hard requirement, not a habit. `demo-ssg` calls this on the
//! HOST at build time and the browser calls it again at boot; if the two could
//! disagree, the page a visitor sees before the wasm lands would be a picture of
//! a diagram that never existed.
//!
//! The placement is rigged so each behaviour is one short drag away:
//!
//!   * Museum (3,3) sits alone with free cells all round — a plain move.
//!   * Museum is edge-adjacent to Cinema: axial (+1,-1) from (3,3) is (4,2).
//!     The shortest possible refused drop, and both tiles are ungrouped, so the
//!     refusal is not confounded by a group also moving.
//!   * Station (5,0) sits directly above Grocer (5,1), so dragging Market Row up
//!     one row puts Grocer onto Station — a GROUP move refused because ONE
//!     member collides, which is the actual rule.

use std::collections::BTreeMap;

use honeycomb_yew::{
    Cell, Content, Diagram, DiagramSpec, Group, GroupId, Iri, LatticeConvention, OwnTile, Slug,
    TileId, WriteOpts, write_turtle,
};

/// Where this document's own subjects live. Invented, and deliberately not a
/// host anybody operates: a demo that minted its subjects under a real domain
/// would be asserting something about that domain.
const BASE: &str = "https://example.org/honeycomb/demo/";
const SUBJECTS: &str = "https://example.org/honeycomb/demo/town-plan/";
/// Opaque to the vocabulary and to the component. The demo page is the only
/// thing in this repository that knows one of these means a colour — `pub`
/// because the districts drawer mints one when somebody changes a ground, and
/// it has to be the SAME prefix `slot()` reads back or the colour silently does
/// not change.
pub const STYLE: &str = "https://example.org/honeycomb/demo/style/";

fn slug(s: &str) -> Slug {
    Slug::parse(s)
        .unwrap_or_else(|e| panic!("the demo fixture's own identifier is not a slug: {e:?}"))
}

fn tile(label: &str, style: &str, group: Option<&str>) -> OwnTile {
    OwnTile {
        group: group.map(|g| GroupId(slug(g))),
        label: label.to_string(),
        comment: None,
        style_key: Some(Iri(format!("{STYLE}{style}"))),
        extra: Vec::new(),
    }
}

fn group(label: &str, style: &str) -> Group {
    Group {
        label: label.to_string(),
        style_key: Some(Iri(format!("{STYLE}{style}"))),
        note: None,
        extra: Vec::new(),
    }
}

/// THREE BUILDINGS THAT ARE NOT ON THE PLAN, for the palette to hand out.
///
/// STANDALONE, which is the point of them being here at all: a `PinnedTile` has
/// nowhere to put a label, so a pinned demo could not show a palette that hands
/// out anything a reader could recognise. One per existing district, so dropping
/// any of them demonstrates a tile joining a group it was never next to.
///
/// A pure function of nothing, for the same reason `town_plan` is: the
/// build-time render and the browser's first render have to produce the same
/// bytes, and a clock or a shuffle is the one thing that guarantees they cannot.
pub fn bench() -> Vec<(TileId, OwnTile)> {
    vec![
        (
            TileId(slug("archive")),
            tile("Archive", "civic", Some("civic")),
        ),
        (
            TileId(slug("allotment")),
            tile("Allotment", "green", Some("green")),
        ),
        (
            TileId(slug("cobbler")),
            tile("Cobbler", "market", Some("market")),
        ),
    ]
}

pub fn town_plan() -> Diagram {
    // (slug, label, cell, group)
    let rows: [(&str, &str, Cell, Option<&str>); 12] = [
        ("hall", "Town Hall", Cell { col: 1, row: 0 }, Some("civic")),
        ("library", "Library", Cell { col: 2, row: 0 }, Some("civic")),
        ("school", "School", Cell { col: 1, row: 1 }, Some("civic")),
        ("clinic", "Clinic", Cell { col: 2, row: 1 }, Some("civic")),
        ("bakery", "Bakery", Cell { col: 4, row: 1 }, Some("market")),
        ("grocer", "Grocer", Cell { col: 5, row: 1 }, Some("market")),
        (
            "florist",
            "Florist",
            Cell { col: 6, row: 1 },
            Some("market"),
        ),
        ("park", "Park", Cell { col: 0, row: 3 }, Some("green")),
        ("orchard", "Orchard", Cell { col: 1, row: 3 }, Some("green")),
        ("station", "Station", Cell { col: 5, row: 0 }, None),
        ("cinema", "Cinema", Cell { col: 4, row: 2 }, None),
        ("museum", "Museum", Cell { col: 3, row: 3 }, None),
    ];

    let mut tiles = BTreeMap::new();
    let mut cells = BTreeMap::new();
    for (id, label, cell, g) in rows {
        let key = TileId(slug(id));
        tiles.insert(key.clone(), tile(label, g.unwrap_or("plain"), g));
        cells.insert(key, cell);
    }

    let mut groups = BTreeMap::new();
    groups.insert(GroupId(slug("civic")), group("Civic Quarter", "civic"));
    groups.insert(GroupId(slug("green")), group("Green Belt", "green"));
    groups.insert(GroupId(slug("market")), group("Market Row", "market"));

    Diagram::try_new(DiagramSpec {
        slug: slug("town-plan"),
        label: "Town plan (demo)".to_string(),
        note: Some("Every name on this page is invented.".to_string()),
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        // NO TIMESTAMP, AND THAT IS THE POINT. The build-time render and the
        // browser render must produce the same bytes, and a clock is the one
        // thing that guarantees they cannot. A document that states no time is
        // honest; one that states a time it did not measure is not.
        generated_at: None,
        groups,
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links: BTreeMap::new(),
    })
    .unwrap_or_else(|e| panic!("the demo fixture is not a legal diagram: {e:?}"))
}

/// The one set of write options, so the file `demo-ssg` writes and the text the
/// page shows are the same bytes rather than two spellings of them.
pub fn opts() -> WriteOpts {
    WriteOpts::new(BASE, "plan", SUBJECTS)
        .unwrap_or_else(|e| panic!("the demo's write options are not valid: {e:?}"))
}

pub fn turtle(d: &Diagram) -> String {
    write_turtle(d, &opts())
}

#[cfg(test)]
mod tests {
    use super::*;
    use honeycomb_yew::{Axial, Command, ReadOpts, Rejection, read_turtle};

    fn tid(s: &str) -> TileId {
        TileId(slug(s))
    }

    /// The fixture is the contract between the build-time render and the
    /// browser render. If it ever stops being a pure function of nothing, this
    /// is what says so — and the page a visitor sees before the wasm lands
    /// becomes a picture of a diagram that never existed.
    #[test]
    fn fixture_is_deterministic() {
        assert_eq!(
            town_plan(),
            town_plan(),
            "the demo fixture is not a pure function of nothing, so the generated HTML and the \
             browser's first render can disagree"
        );
        assert_eq!(
            turtle(&town_plan()),
            turtle(&town_plan()),
            "the demo serialises differently on two calls, so the .ttl the download link points at \
             and the .ttl printed on the page are two different documents"
        );
    }

    /// The page tells a visitor to try these. If they stop behaving as
    /// described, the instructions are a lie and the demo teaches the wrong
    /// rule.
    ///
    /// EVERY ASSERTION SENDS THE `detach` FLAG ITS OWN GESTURE PRODUCES, and
    /// that is the whole reason this test is worth rewriting rather than
    /// extending. It used to pass `detach: false` everywhere, which is what a
    /// press on a hexagon used to mean — so when the gesture inverted, all four
    /// assertions stayed green while every instruction on the page became
    /// wrong. A test that cannot fail when the thing it describes changes is
    /// worse than no test, because it is read as coverage.
    #[test]
    fn the_rigged_drags_behave_as_the_page_claims() {
        let d = town_plan();

        // "Drag Library to an empty cell — it goes alone." A TILE PRESS, so
        // detached: this is the gesture the first instruction now describes.
        let alone = d
            .check(&Command::Translate {
                grabbed: tid("library"),
                delta: Axial { q: 2, r: 2 },
                detach: true,
            })
            .expect("Library cannot make the plain move the page invites first");
        assert_eq!(
            alone.moves(&d).len(),
            1,
            "a tile press must move one tile; the page's first instruction says so"
        );

        // "...and the Civic Quarter is now in two parts."
        let mut after = d.clone();
        after
            .apply(Command::Translate {
                grabbed: tid("library"),
                delta: Axial { q: 2, r: 2 },
                detach: true,
            })
            .unwrap();
        assert_eq!(
            after.group_components(&GroupId(slug("civic"))).len(),
            2,
            "the page promises a visible split and the model does not produce one"
        );
        assert_eq!(
            after.group_of(&tid("library")),
            Some(&GroupId(slug("civic"))),
            "a drag must never change membership, however far the tile went"
        );

        // "Drag the Civic Quarter's heading — all four move together." A GROUND
        // press, so attached. The pair of assertions is deliberate: one gesture
        // each, on the same tile, proving they differ.
        assert_eq!(
            d.moving_set(&tid("library"), false).len(),
            4,
            "the Civic Quarter no longer has four members that move together"
        );

        // "Drop Bakery onto Grocer — same group, so they trade places."
        match d.check(&Command::Translate {
            grabbed: tid("bakery"),
            delta: Axial { q: 1, r: 0 },
            detach: true,
        }) {
            Ok(plan) => {
                assert_eq!(plan.displaced(), Some(&tid("grocer")));
                let mut moves = plan.moves(&d);
                moves.sort();
                assert_eq!(
                    moves,
                    vec![
                        (tid("bakery"), Cell { col: 5, row: 1 }),
                        (tid("grocer"), Cell { col: 4, row: 1 }),
                    ],
                    "the swap the page demonstrates does not land the two tiles on each \
                     other's cells"
                );
            }
            other => panic!(
                "Bakery onto Grocer was not a swap but {other:?}; the page's third \
                 instruction demonstrates nothing"
            ),
        }

        // "Drop Museum onto Cinema — neither is in a group, so it refuses."
        // THE ROW THAT KEEPS SWAPPING NARROW: two ungrouped tiles are not "in
        // the same group", and if that ever becomes true this instruction is
        // wrong in the most confusing possible way — the page would say refuse
        // and the board would swap.
        assert_eq!(d.group_of(&tid("museum")), None);
        assert_eq!(d.group_of(&tid("cinema")), None);
        match d.check(&Command::Translate {
            grabbed: tid("museum"),
            delta: Axial { q: 1, r: -1 },
            detach: true,
        }) {
            Err(Rejection::Occupied { blocked }) => assert_eq!(
                blocked,
                vec![(Cell { col: 4, row: 2 }, tid("cinema"))],
                "the refusal does not name Cinema, so the status line points at the wrong hexagon"
            ),
            other => panic!(
                "dropping Museum onto Cinema was not refused as an overlap but as {other:?}; the \
                 page's fourth instruction demonstrates nothing"
            ),
        }

        // An ungrouped tile onto a grouped one, and the mirror: neither is a
        // pair inside one group, so both refuse. Not on the page, but it is the
        // asymmetry a guard written with one `group_of` call gets wrong.
        // The delta is DERIVED from the two cells rather than written out: an
        // odd-r offset pair does not translate into an axial delta by
        // inspection, and a hand-written one that is quietly wrong makes this
        // assert about an empty cell instead of about the rule.
        for (who, onto) in [("cinema", "bakery"), ("bakery", "cinema")] {
            let from = d.cell_of(&tid(who)).unwrap();
            let to = d.cell_of(&tid(onto)).unwrap();
            assert!(
                matches!(
                    d.check(&Command::Translate {
                        grabbed: tid(who),
                        delta: to.to_axial().minus(from.to_axial()),
                        detach: true,
                    }),
                    Err(Rejection::Occupied { .. })
                ),
                "{who} onto {onto} must refuse: they are not in one group"
            );
        }

        // "Click Archive in the palette, then click an empty cell." THE PAGE
        // GREW TWO INSTRUCTIONS AND THIS TEST DID NOT, which is how the palette
        // shipped with its keyboard half broken: nothing here exercised an Add
        // at all, and the doc comment above promises the opposite.
        let (archive_id, archive) = bench()
            .into_iter()
            .find(|(id, _)| id.0.as_str() == "archive")
            .expect("the page names Archive in its fifth instruction");
        let free = Cell { col: 1, row: 4 };
        assert!(d.at(free).is_none(), "the fixture moved under this test");
        let mut placed = d.clone();
        placed
            .apply(Command::Add {
                tile: archive_id.clone(),
                at: free,
                what: honeycomb_yew::NewTile::Own(Box::new(archive)),
            })
            .expect("Archive cannot be placed, so the page's fifth instruction is a lie");
        assert_eq!(placed.cell_of(&archive_id), Some(free));
        assert_eq!(
            placed.group_of(&archive_id),
            Some(&GroupId(slug("civic"))),
            "a placed building must join the district its chip names"
        );

        // "Select a building and press Delete — it goes back to the palette, and
        // Undo brings it back."
        let inverse = placed
            .apply(Command::Remove { tile: archive_id.clone() })
            .expect("Delete cannot remove it, so the page's sixth instruction is a lie");
        assert!(placed.cell_of(&archive_id).is_none());
        placed.apply(inverse).expect("Undo must bring it back");
        assert_eq!(
            placed.cell_of(&archive_id),
            Some(free),
            "Undo did not return the building to where it was placed"
        );

        // "Rename a district below, change its ground, or clear its name
        // entirely — a district may have none." The drawer is a form and its
        // behaviour is the core's; what this pins is that the page's promise and
        // the fixture agree, because the fixture is what the promise is about.
        let civic = GroupId(slug("civic"));
        let mut edited = d.clone();
        let was = edited.group(&civic).expect("the plan has a Civic Quarter").clone();
        assert!(!was.label.is_empty(), "the page says you can CLEAR a name");
        let inverse = edited
            .apply(Command::EditGroup {
                id: civic.clone(),
                group: Group {
                    label: String::new(),
                    ..was.clone()
                },
            })
            .expect("a district may be left unnamed");
        assert_eq!(
            edited.group(&civic).map(|g| g.label.as_str()),
            Some(""),
            "clearing a district's name was refused, so the page's instruction is a lie"
        );
        edited.apply(inverse).expect("and it comes back");
        assert_eq!(edited.group(&civic), Some(&was));

        // Delete is offered per district and disabled while anything is in it.
        // The button explains and the rule enforces; this is the rule.
        assert!(matches!(
            edited.check(&Command::RemoveGroup { id: civic.clone() }),
            Err(Rejection::GroupInUse { .. })
        ));

        // A new district is declared from the name alone, its slug derived —
        // and an Add into it then lands, which is the whole reason a host can
        // declare one at all.
        let harbour = GroupId(slug("harbour-quarter"));
        let mut grown = d.clone();
        grown
            .apply(Command::DeclareGroup {
                id: harbour.clone(),
                group: group("Harbour Quarter", "civic"),
            })
            .expect("a new district");
        assert!(grown.members(&harbour).is_empty());
        assert!(
            grown
                .apply(Command::RemoveGroup { id: harbour })
                .is_ok(),
            "an empty district must be deletable"
        );

        // A GROUP move refused because ONE member collides: Market Row up one
        // row puts Grocer onto Station. Still `detach: false`, because that is
        // still what a ground press sends.
        match d.check(&Command::Translate {
            grabbed: tid("bakery"),
            delta: Axial { q: 0, r: -1 },
            detach: false,
        }) {
            Err(Rejection::Occupied { blocked }) => assert_eq!(
                blocked,
                vec![(Cell { col: 5, row: 0 }, tid("station"))],
                "the group refusal does not name Station as the single blocker"
            ),
            other => panic!("Market Row moved onto Station, or was refused as {other:?}"),
        }
    }

    /// THE BENCH AND THE PLAN MUST NOT OVERLAP.
    ///
    /// `Command::Add` refuses an id the board already has, so a bench entry that
    /// shares a name with a building on the plan is a chip that can never be
    /// placed — and the drawer, which shows whatever is on the roster and not on
    /// the plan, simply never renders it. It vanishes silently. The first
    /// version of `bench` had exactly that collision (`orchard`), and what found
    /// it was a browser showing two chips where the code said three.
    #[test]
    fn nothing_on_the_bench_is_already_on_the_plan() {
        let d = town_plan();
        for (id, t) in bench() {
            assert!(
                d.cell_of(&id).is_none(),
                "{} ({}) is already on the plan, so its chip can never be placed",
                t.label,
                id.0.as_str()
            );
            assert!(
                d.has_group(t.group.as_ref().expect("every bench tile names a district")),
                "{} names a district the plan does not declare, so Add would refuse it",
                t.label
            );
            assert!(!t.label.trim().is_empty(), "a blank label cannot be added");
        }
        assert_eq!(bench().len(), 3, "the page's count sentence says three");
    }

    /// The download link points at a real file; the page prints what it
    /// believes is the same document. If the writer's own output cannot be read
    /// back, the file on disk is not a diagram anybody can reload.
    #[test]
    fn the_published_ttl_reads_back_as_the_diagram_it_came_from() {
        let d = town_plan();
        let ttl = turtle(&d);
        let back = read_turtle(&ttl, &ReadOpts::default()).unwrap_or_else(|e| {
            panic!(
                "the demo's own .ttl could not be read back ({e:?}), so the file the page \
                    publishes is not loadable by the editor that wrote it"
            )
        });
        assert_eq!(
            turtle(&back),
            ttl,
            "the demo diagram does not survive a round trip byte for byte"
        );
    }
}
