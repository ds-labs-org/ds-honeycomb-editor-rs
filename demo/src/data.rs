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
//!
//! AND THREE LINES ARE ALREADY DRAWN, one of each combination a link endpoint
//! can now be: district to district, district to building, building to
//! district. See `lines` below for why the fixture carries them rather than
//! leaving the feature for a visitor to discover.

use std::collections::BTreeMap;

use honeycomb_yew::{
    Cell, Content, Diagram, DiagramSpec, Endpoint, Group, GroupId, Iri, LatticeConvention, Link,
    LinkId, OwnTile, Routing, Slug, TileId, WriteOpts, write_turtle,
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
        label: label.into(),
        comment: None,
        style_key: Some(Iri(format!("{STYLE}{style}"))),
        extra: Vec::new(),
    }
}

fn group(label: &str, style: &str) -> Group {
    Group {
        label: label.into(),
        style_key: Some(Iri(format!("{STYLE}{style}"))),
        note: None,
        extra: Vec::new(),
    }
}

/// Same as `group`, plus a note — the district's only annotation channel. A
/// pinned placement has nowhere to hold per-tile content at all (`hsh:
/// PlacementShape` is `sh:closed`), so a note on the district that holds it is
/// the one thing this format lets an author say beyond names: "deployed once
/// per participant", a caveat, a count. Used on exactly one district below so
/// the demo shows the note being DRAWN rather than merely being present in the
/// Turtle a reader would have to open the download to notice.
fn group_with_note(label: &str, style: &str, note: &str) -> Group {
    Group {
        note: Some(note.to_string()),
        ..group(label, style)
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

/// THE THREE LINES THE PLAN IS PUBLISHED WITH, one per combination a link
/// endpoint can now be.
///
/// WHY THE FIXTURE CARRIES ANY AT ALL. `demo-ssg` renders this document on the
/// host at build time, so the generated HTML — the page a search engine
/// indexes, and the whole of the page for a reader with no wasm — shows
/// exactly what `town_plan` put there and nothing a gesture could add. A
/// fixture with no group-ended link publishes a demo of the widening that
/// demonstrates none of it, and leaves the feature reachable only by a visitor
/// who presses Link and then guesses that a district's coloured ground is
/// pressable.
///
/// WHY ALL THREE AND NOT ONE. Group-to-group, group-to-cell and cell-to-group
/// are three different pictures, and it is the ASYMMETRIC pair that teaches:
/// a line with a district at one end and a hexagon at the other puts the two
/// terminators side by side in one stroke, so a reader can see which is which
/// without being told.
///
/// EACH ON A DIFFERENT ROUTING, for the same reason the page's Link gesture
/// cycles them: all three routings have to keep working with a region at an
/// end, and `Routing::LatticePath` is the one that could not have — it walks
/// the comb from a real cell, which a centroid could never have given it. The
/// greenway is the lattice-path one deliberately.
///
/// THE IDS ARE WORDS, NOT `line-N`. `DemoApp` mints `line-1`, `line-2`, … from
/// a counter as a visitor draws, and a fixture that took a name out of that
/// sequence would make the FIRST line anybody drew collide — `AlreadyConnected`
/// on the very gesture the page invites, with nothing on screen to explain why.
fn lines() -> BTreeMap<LinkId, Link> {
    let mut links = BTreeMap::new();

    // DISTRICT TO DISTRICT. Clinic (2,1) and Bakery (4,1) are the facing pair
    // at distance 2, so the line runs along row 1 through the one empty cell
    // between them — short, horizontal and unambiguous. It is the one with a
    // label, because "the whole Civic Quarter deals with the whole Market Row"
    // is the claim a reader is least likely to guess from a bare stroke.
    links.insert(
        LinkId(slug("errands")),
        Link {
            from: Endpoint::Group(GroupId(slug("civic"))),
            to: Endpoint::Group(GroupId(slug("market"))),
            label: Some("errands".into()),
            routing: Routing::Straight,
            style_key: None,
            extra: Vec::new(),
        },
    );

    // DISTRICT TO BUILDING, and the one the page's own instruction about
    // re-anchoring is written against: the Green Belt sits west of the Museum
    // with Orchard on its eastern side, so the line leaves from Orchard, and
    // dragging the district clear past the Museum swaps it to Park.
    links.insert(
        LinkId(slug("greenway")),
        Link {
            from: Endpoint::Group(GroupId(slug("green"))),
            to: Endpoint::Tile(TileId(slug("museum"))),
            label: None,
            routing: Routing::LatticePath,
            style_key: None,
            extra: Vec::new(),
        },
    );

    // BUILDING TO DISTRICT. Station is ungrouped and sits on row 0, so the arc
    // bows up into the empty comb above the board rather than across anything
    // — and its end is a genuine tie, Library and Clinic both three cells
    // away, which is why nothing anywhere asserts WHICH of them it meets.
    links.insert(
        LinkId(slug("commute")),
        Link {
            from: Endpoint::Tile(TileId(slug("station"))),
            to: Endpoint::Group(GroupId(slug("civic"))),
            label: None,
            routing: Routing::Arc,
            style_key: None,
            extra: Vec::new(),
        },
    );

    links
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
    groups.insert(
        GroupId(slug("civic")),
        group_with_note(
            "Civic Quarter",
            "civic",
            "Rebuilt twice; the name outlasted both.",
        ),
    );
    groups.insert(GroupId(slug("green")), group("Green Belt", "green"));
    groups.insert(GroupId(slug("market")), group("Market Row", "market"));

    Diagram::try_new(DiagramSpec {
        slug: slug("town-plan"),
        label: "Town plan (demo)".into(),
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
        links: lines(),
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
    use honeycomb_yew::{
        Axial, Command, Endpoint, Link, LinkId, ReadOpts, Rejection, Text, read_turtle,
    };

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

    /// A LINE THAT ENDS ON A WHOLE DISTRICT, SHIPPED IN THE FIXTURE ITSELF.
    ///
    /// The feature is invisible on the published page unless the dummy dataset
    /// already uses it: a visitor who never presses Link, and every reader of
    /// the generated HTML who has no wasm at all, sees only what `town_plan`
    /// put there. `demo-ssg` renders exactly this document at build time, so a
    /// fixture with no group-ended link means the static page — the one a
    /// search engine and a JavaScript-less reader get — demonstrates nothing
    /// of the widening.
    ///
    /// ALL THREE COMBINATIONS, not one of them. Group-to-group, group-to-cell
    /// and cell-to-group are three different pictures: the first meets a
    /// region at both ends, the other two meet a region at one end and a
    /// hexagon at the other, and it is precisely the ASYMMETRIC pair that
    /// shows a reader what a district end looks like — there is a placement
    /// end in the same line to compare it against.
    #[test]
    fn the_plan_ships_a_line_at_every_one_of_the_three_new_combinations() {
        let d = town_plan();

        let mut seen: Vec<(&'static str, LinkId)> = Vec::new();
        for (id, l) in d.links() {
            let kind = match (&l.from, &l.to) {
                (Endpoint::Group(_), Endpoint::Group(_)) => "group-to-group",
                (Endpoint::Group(_), Endpoint::Tile(_)) => "group-to-cell",
                (Endpoint::Tile(_), Endpoint::Group(_)) => "cell-to-group",
                (Endpoint::Tile(_), Endpoint::Tile(_)) => continue,
            };
            seen.push((kind, id.clone()));
        }
        let kinds: Vec<&str> = {
            let mut k: Vec<&str> = seen.iter().map(|(k, _)| *k).collect();
            k.sort_unstable();
            k.dedup();
            k
        };
        assert_eq!(
            kinds,
            vec!["cell-to-group", "group-to-cell", "group-to-group"],
            "the town plan does not ship one line of each new combination, so the published \
             page shows the group endpoint only to a visitor who goes looking for it"
        );

        // DECISION 1, AS A PROPERTY RATHER THAN A NAMED MEMBER. The anchor is
        // the NEAREST member, and asserting a particular slug here would pin
        // whichever way `anchors` happens to break a tie as though it were the
        // rule. What the decision actually says is that no member is closer
        // than the one the line meets, which is true with or without a tie.
        for (id, l) in d.links() {
            let (a, b) = d.link_anchors(l).unwrap_or_else(|| {
                panic!(
                    "{} has no anchors, so the renderer cannot draw it and the board is in the \
                     one state the four decisions exist to forbid",
                    id.0.as_str()
                )
            });
            for (end, near, far) in [(&l.from, a, b), (&l.to, b, a)] {
                let cells = d.endpoint_cells(end);
                assert!(
                    !cells.is_empty(),
                    "{} names {end}, which has no cell at all: decision 2 forbids an empty \
                     group as an endpoint",
                    id.0.as_str()
                );
                let chosen = near.to_axial().distance(far.to_axial());
                let closest = cells
                    .iter()
                    .map(|c| c.to_axial().distance(far.to_axial()))
                    .min()
                    .expect("a non-empty endpoint has a nearest cell");
                assert_eq!(
                    chosen,
                    closest,
                    "{} meets {end} at {near:?}, which is not its nearest cell to {far:?}: the \
                     line does not touch the region's edge facing what it connects to",
                    id.0.as_str()
                );
            }
        }

        // DECISION 3, on the fixture rather than at the doorway. `try_new`
        // refuses a document that states one, so this cannot fail while the
        // fixture builds — it is here because the fixture is edited by hand
        // and a reader adding a fourth line deserves the sentence, not a
        // `ModelError` debug dump out of `town_plan`'s own panic.
        for (id, l) in d.links() {
            for (g, t) in [(&l.from, &l.to), (&l.to, &l.from)] {
                if let (Some(g), Some(t)) = (g.group(), t.tile()) {
                    assert_ne!(
                        d.group_of(t),
                        Some(g),
                        "{} joins {} to one of its own buildings",
                        id.0.as_str(),
                        g.0.as_str()
                    );
                }
            }
        }

        // THE ANCHOR IS RECOMPUTED, NOT STORED — the other half of decision 1,
        // and the half a fixture alone cannot show. The Green Belt sits west
        // of the Museum with Orchard on its eastern side, so Orchard is the
        // member that faces it; drag the whole district five columns east,
        // PAST the Museum, and Park is suddenly the near side. A rigid
        // translation cannot reorder two members along a line, which is
        // exactly why the district has to be dragged CLEAR of the other end
        // rather than merely towards it — and why an anchor computed once and
        // stored would survive this move looking perfectly correct.
        let greenway = d
            .links()
            .iter()
            .find(|(_, l)| l.from == Endpoint::Group(GroupId(slug("green"))))
            .map(|(id, _)| id.clone())
            .expect("the Green Belt is one line's own end");
        let before = d.link_anchors(d.link(&greenway).unwrap()).unwrap().0;
        assert_eq!(
            d.at(before),
            Some(&tid("orchard")),
            "the Green Belt's line should leave from Orchard, its member nearest the Museum"
        );
        let mut moved = d.clone();
        moved
            .apply(Command::Translate {
                grabbed: tid("park"),
                delta: Axial { q: 5, r: 0 },
                detach: false,
            })
            .expect("the Green Belt can move five columns east, clear of the Museum");
        let after = moved
            .link_anchors(moved.link(&greenway).unwrap())
            .unwrap()
            .0;
        assert_eq!(
            moved.at(after),
            Some(&tid("park")),
            "the line still leaves from Orchard after the district moved, so the anchor is \
             stored somewhere instead of being re-derived from the arrangement"
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
            .apply(Command::Remove {
                tile: archive_id.clone(),
            })
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
        let was = edited
            .group(&civic)
            .expect("the plan has a Civic Quarter")
            .clone();
        assert!(!was.label.is_blank(), "the page says you can CLEAR a name");
        let inverse = edited
            .apply(Command::EditGroup {
                id: civic.clone(),
                group: Group {
                    label: Text::default(),
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

        // Delete is offered per district, and what stops it is ASKED IN AN
        // ORDER. Every district on this plan is now an end of some line, so
        // the answer for all three of them is the link and not the members —
        // `GroupStillLinked` is checked before `GroupInUse` precisely so the
        // user is never told to empty a district whose last building cannot
        // legally leave. The page's own Delete button has to say the same
        // thing in the same order; `hc-groups__table` is where it does.
        assert!(
            matches!(
                edited.check(&Command::RemoveGroup { id: civic.clone() }),
                Err(Rejection::GroupStillLinked { .. })
            ),
            "the Civic Quarter is an end of two lines and deleting it was not refused for that \
             reason first, so the button's advice would send a reader down a path that ends in \
             another refusal"
        );

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

        // AND THE MEMBERS RULE, WHICH NO DISTRICT ON THE PLAN CAN SHOW ANY
        // MORE. It used to be asserted of the Civic Quarter, and the three
        // lines above took that away: a linked district always has at least
        // one building in it — decision 4 is what guarantees that — so it can
        // never be the plain `GroupInUse` case. A district the test builds
        // itself is the only unlinked one left, and the button's second
        // sentence is still about this.
        let mut occupied = grown.clone();
        occupied
            .apply(Command::Attach {
                tile: tid("museum"),
                group: harbour.clone(),
            })
            .expect("a building can join a new district");
        assert!(matches!(
            occupied.check(&Command::RemoveGroup {
                id: harbour.clone()
            }),
            Err(Rejection::GroupInUse { .. })
        ));

        assert!(
            grown.apply(Command::RemoveGroup { id: harbour }).is_ok(),
            "an empty district must be deletable"
        );

        // "Press Link, then drag from a district's heading to another
        // district." A `Connect` between two districts, which is the command
        // `on_link` builds from the two ends the component reports — the host
        // never decides WHICH kind of end it was handed, so the only thing
        // this page can get wrong is whether the rules accept it.
        let mut drawn = d.clone();
        drawn
            .apply(Command::Connect {
                id: LinkId(slug("drawn-by-hand")),
                link: Link {
                    from: Endpoint::Group(GroupId(slug("civic"))),
                    to: Endpoint::Group(GroupId(slug("green"))),
                    label: None,
                    routing: honeycomb_yew::Routing::Straight,
                    style_key: None,
                    extra: Vec::new(),
                },
            })
            .expect("a district cannot be linked to another district, so instruction 8 is a lie");

        // The same gesture the other two ways round, because all three
        // combinations are on the page and a rule that held for two of them
        // has been the shape of every endpoint bug so far.
        for (n, from, to) in [
            (
                "district to building",
                Endpoint::Group(GroupId(slug("civic"))),
                Endpoint::Tile(tid("museum")),
            ),
            (
                "building to district",
                Endpoint::Tile(tid("cinema")),
                Endpoint::Group(GroupId(slug("green"))),
            ),
        ] {
            let mut one = d.clone();
            one.apply(Command::Connect {
                id: LinkId(slug("drawn-by-hand")),
                link: Link {
                    from,
                    to,
                    label: None,
                    routing: honeycomb_yew::Routing::Straight,
                    style_key: None,
                    extra: Vec::new(),
                },
            })
            .unwrap_or_else(|e| {
                panic!(
                    "{n} was refused as {e:?}, so the page invites a gesture \
                                        that cannot work"
                )
            });
        }

        // DECISION 3, as the page's own likeliest mis-drag: start on the Green
        // Belt's ground, release on one of its own hexagons. The component
        // refuses this before it ever reports a pair of ends, so this is what
        // the rule underneath that refusal says.
        assert!(
            matches!(
                d.check(&Command::Connect {
                    id: LinkId(slug("drawn-by-hand")),
                    link: Link {
                        from: Endpoint::Group(GroupId(slug("green"))),
                        to: Endpoint::Tile(tid("park")),
                        label: None,
                        routing: honeycomb_yew::Routing::Straight,
                        style_key: None,
                        extra: Vec::new(),
                    },
                }),
                Err(Rejection::LinkToOwnMember { .. })
            ),
            "a line from the Green Belt to its own Park was accepted; containment already says \
             it, and the page promises the refusal"
        );

        // DECISION 4, ON THE PATH THIS PAGE ACTUALLY OFFERS. The districts
        // drawer's membership picker is the one control that can empty a
        // district — the Delete key goes through the component, and the
        // district's own Delete button is disabled while anything is in it —
        // so "move the last building out of a linked district" is a `Detach`
        // from that select, and it must be refused BY NAME.
        let mut emptying = d.clone();
        emptying
            .apply(Command::Detach { tile: tid("park") })
            .expect("the first of the Green Belt's two buildings can leave");
        match emptying.check(&Command::Detach {
            tile: tid("orchard"),
        }) {
            Err(Rejection::LastMemberStillLinked { group, links, .. }) => {
                assert_eq!(group, GroupId(slug("green")));
                assert!(
                    !links.is_empty(),
                    "the refusal names no line, so the reader is told to remove something the \
                     page will not tell them the name of"
                );
            }
            other => panic!(
                "emptying the linked Green Belt was not refused but {other:?}; the board can \
                 reach a state where a line exists that nothing can draw"
            ),
        }

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
            assert!(!t.label.is_blank(), "a blank label cannot be added");
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
