//! A LINE THAT ENDS ON A WHOLE GROUP, driven through a real browser: drawn by
//! pointer and by keyboard in all three new combinations, following the region
//! it is drawn to while that region is dragged, and refused in prose when the
//! two ends are a group and one of its own hexagons.
//!
//! A NEW FILE RATHER THAN MORE OF `links.rs`, which pins the two findings of
//! the audit that gave links a removal path and a keyboard. Nothing here is
//! about those; everything here is about the endpoint widening, and a file
//! that mixes the two makes it impossible to tell, later, which subject a
//! failure belongs to.
//!
//! Follows `links.rs`'s pattern exactly: mount the real component with
//! `yew::Renderer`, drive real `PointerEvent` / `KeyboardEvent` / `FocusEvent`
//! objects through `dispatch_event`, `settle` past whatever the scheduler
//! queued, and read the answer back out of the DOM.
//! `#![cfg(target_arch = "wasm32")]` is the same reason it is in every sibling
//! file: on the host target this compiles to an empty module, which is correct
//! — there is no browser there to run any of it in.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use honeycomb_yew::*;
use wasm_bindgen_test::*;
use web_sys::wasm_bindgen::JsCast;
use web_sys::{Document, Element, FocusEvent, KeyboardEventInit, PointerEventInit};
use yew::prelude::*;

wasm_bindgen_test_configure!(run_in_browser);

/// The lattice every test here mounts with, named once so the coordinate
/// helpers below and the props cannot disagree about the size of a hexagon.
const L: Lattice = Lattice::new(46.0, 1.045);

fn document() -> Document {
    web_sys::window()
        .expect("a window")
        .document()
        .expect("a document")
}

fn tid(s: &str) -> TileId {
    TileId(Slug::parse(s).unwrap())
}

fn gid(s: &str) -> GroupId {
    GroupId(Slug::parse(s).unwrap())
}

fn own(label: &str, group: Option<&str>) -> OwnTile {
    OwnTile {
        group: group.map(gid),
        label: label.into(),
        comment: None,
        style_key: None,
        extra: Vec::new(),
    }
}

/// A zero-length `setTimeout`, reliably after every microtask the scheduler
/// queued for a render. Copied rather than shared, for `links.rs`'s reason:
/// each `wasm-bindgen-test` file is its own crate, with no path between them.
async fn settle() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

fn pointer_event_at(kind: &str, x: f64, y: f64) -> web_sys::PointerEvent {
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_button(0);
    init.set_pointer_id(1);
    init.set_client_x(x as i32);
    init.set_client_y(y as i32);
    web_sys::PointerEvent::new_with_event_init_dict(kind, &init)
        .expect("constructing a synthetic PointerEvent")
}

fn keydown(key: &str) -> web_sys::KeyboardEvent {
    let init = KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(key);
    web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("constructing a synthetic KeyboardEvent")
}

/// `focus` and `blur` do not bubble, but `dispatch_event` invokes a listener
/// bound directly to its target regardless — the same reason `keyboard.rs` can
/// dispatch one with no real user gesture behind it. This is how Tab reaching
/// a group's region is simulated.
fn focus_event() -> FocusEvent {
    FocusEvent::new("focus").expect("constructing a synthetic FocusEvent")
}

fn live_text(container: &Element) -> String {
    container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered")
        .text_content()
        .unwrap_or_default()
}

/// Where a tile is drawn, in the board's own user units, read back off the
/// element the component rendered.
///
/// READ RATHER THAN RECOMPUTED, and that is the whole reason these helpers are
/// shaped this way. The frame's origin depends on the content's bounding box
/// grown by the frame ring, which is arithmetic this crate keeps in ONE place
/// (`frames`, private); a test that rebuilt it would be a second copy free to
/// disagree, and a wrong origin here would silently aim every gesture below at
/// the wrong cell and still look like a failing feature.
fn tile_user_xy(container: &Element, id: &str) -> (f64, f64) {
    let el = container
        .query_selector(&format!("[data-tile=\"{id}\"]"))
        .unwrap()
        .unwrap_or_else(|| panic!("{id} is on the board"));
    let t = el
        .get_attribute("transform")
        .expect("a translate transform");
    let inner = t
        .trim()
        .trim_start_matches("translate(")
        .trim_end_matches(')');
    let mut parts = inner.split_whitespace();
    let x = parts.next().expect("an x").parse().expect("a number");
    let y = parts.next().expect("a y").parse().expect("a number");
    (x, y)
}

/// A point in the board's user units, in client pixels — through the SVG's own
/// screen CTM, never by hand against a bounding rect, for the same reason the
/// component itself does it that way: the element is `width:100%` inside a
/// scrolling page and manual arithmetic is right only while nothing has
/// scrolled or transformed it.
fn to_client(board: &Element, x: f64, y: f64) -> (f64, f64) {
    let svg: web_sys::SvgsvgElement = board.clone().dyn_into().expect("the root is an <svg>");
    let m = svg.get_screen_ctm().expect("a screen CTM");
    (
        (m.a() as f64) * x + (m.c() as f64) * y + (m.e() as f64),
        (m.b() as f64) * x + (m.d() as f64) * y + (m.f() as f64),
    )
}

/// The centre of any cell, in client pixels: one rendered tile's position plus
/// the lattice offset between its cell and the wanted one. `Lattice::centre` is
/// the same function the component hit-tests with, so a point aimed here lands
/// in the middle of the intended hexagon rather than near its edge, where
/// `next_candidate`'s hysteresis would legitimately keep the old candidate.
fn cell_client(
    container: &Element,
    board: &Element,
    anchor: &str,
    anchor_at: Cell,
    want: Cell,
) -> (f64, f64) {
    let (ax, ay) = tile_user_xy(container, anchor);
    let (bx, by) = L.centre(anchor_at);
    let (wx, wy) = L.centre(want);
    to_client(board, ax + wx - bx, ay + wy - by)
}

/// ana and bea in north, eve and fay in south, four columns of clear comb
/// between the two regions.
///
/// THE GAP IS LOAD-BearING. The cells one step outside a membership are where
/// a group's ground is painted and where its heading sits, and they are how a
/// pointer names a region rather than a hexagon; two groups close enough to
/// share such a cell would make every assertion below about which region was
/// meant an assertion about the tie-break instead.
fn two_districts() -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    let mut cells = BTreeMap::new();
    for (name, col, group) in [
        ("ana", 0, "north"),
        ("bea", 1, "north"),
        ("eve", 4, "south"),
        ("fay", 5, "south"),
    ] {
        tiles.insert(tid(name), own(name, Some(group)));
        cells.insert(tid(name), Cell { col, row: 0 });
    }
    let mut groups = BTreeMap::new();
    for g in ["north", "south"] {
        groups.insert(
            gid(g),
            Group {
                label: g.into(),
                style_key: None,
                note: None,
                extra: Vec::new(),
            },
        );
    }
    Diagram::try_new(DiagramSpec {
        slug: Slug::parse("plan").unwrap(),
        label: "Plan".into(),
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
    .expect("two districts with two tiles each is a legal standalone diagram")
}

/// The same board with one line already on it, from the WHOLE of north to fay.
fn two_districts_linked() -> Diagram {
    let base = two_districts();
    let mut links = BTreeMap::new();
    links.insert(
        LinkId(Slug::parse("spur").unwrap()),
        Link {
            from: Endpoint::Group(gid("north")),
            to: Endpoint::Tile(tid("fay")),
            label: None,
            routing: Routing::Straight,
            style_key: None,
            extra: Vec::new(),
        },
    );
    rebuild(&base, links)
}

/// A group of exactly one tile, with a line reaching it: the board on which
/// removing that tile is `Rejection::LastMemberStillLinked` — decision 4.
fn a_group_of_one() -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    let mut cells = BTreeMap::new();
    tiles.insert(tid("sam"), own("sam", Some("solo")));
    cells.insert(tid("sam"), Cell { col: 0, row: 0 });
    tiles.insert(tid("ana"), own("ana", None));
    cells.insert(tid("ana"), Cell { col: 3, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        gid("solo"),
        Group {
            label: "solo".into(),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    let mut links = BTreeMap::new();
    links.insert(
        LinkId(Slug::parse("spur").unwrap()),
        Link {
            from: Endpoint::Group(gid("solo")),
            to: Endpoint::Tile(tid("ana")),
            label: None,
            routing: Routing::Straight,
            style_key: None,
            extra: Vec::new(),
        },
    );
    Diagram::try_new(DiagramSpec {
        slug: Slug::parse("plan").unwrap(),
        label: "Plan".into(),
        note: None,
        convention: LatticeConvention::OddRPointyTop,
        generator: None,
        generated_at: None,
        groups,
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links,
    })
    .expect("a one-tile group with a line to it is a legal standalone diagram")
}

/// `Diagram` has no "with these links" constructor, and a test has no business
/// inventing one: this rebuilds the spec from the accessors the crate already
/// exposes, so a fixture cannot drift from what `try_new` will accept.
fn rebuild(base: &Diagram, links: BTreeMap<LinkId, Link>) -> Diagram {
    let tiles: BTreeMap<TileId, OwnTile> = match base.content() {
        Content::Standalone { tiles } => tiles.clone(),
        _ => panic!("these fixtures are standalone"),
    };
    let cells: BTreeMap<TileId, Cell> = base.cells().map(|(c, id)| (id.clone(), c)).collect();
    Diagram::try_new(DiagramSpec {
        slug: base.slug().clone(),
        label: base.label_text().clone(),
        note: None,
        convention: base.convention(),
        generator: None,
        generated_at: None,
        groups: base.groups().clone(),
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links,
    })
    .expect("the same diagram plus a link is still legal")
}

fn base_props(diagram: Rc<Diagram>) -> HoneycombProps {
    HoneycombProps {
        diagram,
        lattice: L,
        pad: 26.0,
        frame_ring: 1,
        tile: Callback::from(|_: TileView| html! {}),
        ground: None,
        frame: None,
        link: Some(Callback::from(|_: LinkView| html! { <path /> })),
        // EVERY TEST HERE BUT THE REMOVAL IS IN LINK MODE, which is the mode a
        // drag means "draw" in (see `HoneycombProps::linking`).
        linking: true,
        on_link: Callback::noop(),
        on_change: Callback::noop(),
        on_reject: Callback::noop(),
        on_status: Callback::noop(),
        on_select: Callback::noop(),
        pending: None,
        on_pending: Callback::noop(),
        removable: false,
        readonly: false,
        class: Classes::new(),
        aria_label: AttrValue::from("test board"),
    }
}

/// Mounts the board in link mode with a recorder wired to `on_link`.
async fn mounted(
    diagram: Diagram,
) -> (
    Element,
    Rc<RefCell<Vec<(Endpoint, Endpoint)>>>,
    yew::AppHandle<Honeycomb>,
) {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();
    let drawn: Rc<RefCell<Vec<(Endpoint, Endpoint)>>> = Rc::new(RefCell::new(Vec::new()));
    let on_link = {
        let drawn = drawn.clone();
        Callback::from(move |ends: (Endpoint, Endpoint)| drawn.borrow_mut().push(ends))
    };
    let props = HoneycombProps {
        on_link,
        ..base_props(Rc::new(diagram))
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;
    (container, drawn, handle)
}

fn board(container: &Element) -> Element {
    container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted")
}

fn ground(container: &Element, g: &str) -> Element {
    container
        .query_selector(&format!("[data-group=\"{g}\"]"))
        .unwrap()
        .unwrap_or_else(|| panic!("{g}'s region mounted"))
}

/// Press on `press_on`, drag to `to`, release: the whole pointer gesture, with
/// a real pointermove between down and up because the component treats a press
/// that never travels as a selection (see `PRESS_SLOP`).
///
/// THE RELEASE POINT IS COMPUTED AFTER THE PRESS, NOT BEFORE, and that is not
/// tidiness: `begin` focuses the board on pointerdown (it must — cancelling the
/// event also cancels the focus the compatibility mousedown would have moved),
/// and focusing an element that is out of view SCROLLS IT INTO VIEW. Client
/// coordinates worked out before that press are then measured against a page
/// that has since moved, and the component rightly reports a release off the
/// board. Seen for real: with earlier tests' containers still in the document,
/// this file's later drags all landed outside.
async fn drag(container: &Element, press_on: &Element, from: Cell, to: Cell) {
    let b = board(container);
    let (dx, dy) = cell_client(container, &b, "ana", Cell { col: 0, row: 0 }, from);
    press_on
        .dispatch_event(&pointer_event_at("pointerdown", dx, dy))
        .unwrap();
    settle().await;
    let (ux, uy) = cell_client(container, &b, "ana", Cell { col: 0, row: 0 }, to);
    b.dispatch_event(&pointer_event_at("pointermove", ux, uy))
        .unwrap();
    settle().await;
    b.dispatch_event(&pointer_event_at("pointerup", ux, uy))
        .unwrap();
    settle().await;
}

/// An empty cell one step outside north's membership: where its ground is
/// painted, and how a pointer says "the region" rather than "that hexagon".
const NORTH_GROUND: Cell = Cell { col: 0, row: 1 };
/// The same for south, whose members are at columns 4 and 5.
const SOUTH_GROUND: Cell = Cell { col: 3, row: 0 };

// ================================================== drawing, by pointer

/// GROUP TO GROUP. Press on one region's ground, release over another's: the
/// component reports two group ends and the host draws one line between two
/// districts.
#[wasm_bindgen_test]
async fn a_pointer_drag_from_one_ground_to_another_draws_a_line_between_two_groups() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let north = ground(&container, "north");

    drag(&container, &north, NORTH_GROUND, SOUTH_GROUND).await;

    assert_eq!(
        drawn.borrow().as_slice(),
        &[(Endpoint::Group(gid("north")), Endpoint::Group(gid("south")))],
        "a drag from one ground to another must report a line between the two GROUPS, not \
         between whichever hexagons happened to be nearest: the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// GROUP TO CELL, and the status line while the pointer is still down. A line
/// out of a region into one hexagon of another.
#[wasm_bindgen_test]
async fn a_pointer_drag_from_a_ground_onto_a_hexagon_draws_a_line_from_the_group() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let north = ground(&container, "north");

    drag(&container, &north, NORTH_GROUND, Cell { col: 4, row: 0 }).await;

    assert_eq!(
        drawn.borrow().as_slice(),
        &[(Endpoint::Group(gid("north")), Endpoint::Tile(tid("eve")))],
        "the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// CELL TO GROUP, the mirror: a press on a hexagon already starts a line
/// (`ontiledown`, in link mode), and releasing over a ground now ends it on the
/// whole region.
#[wasm_bindgen_test]
async fn a_pointer_drag_from_a_hexagon_onto_a_ground_draws_a_line_to_the_group() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let ana = container
        .query_selector("[data-tile=\"ana\"]")
        .unwrap()
        .expect("ana is on the board");

    drag(&container, &ana, Cell { col: 0, row: 0 }, SOUTH_GROUND).await;

    assert_eq!(
        drawn.borrow().as_slice(),
        &[(Endpoint::Tile(tid("ana")), Endpoint::Group(gid("south")))],
        "the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// ================================================= drawing, by keyboard

/// A drag-only gesture fails WCAG 2.1 SC 2.1.1, and groups have been tab stops
/// with `role="button"` since the group drag existed — so Space on a focused
/// ground in link mode has to START a line rather than grab the cluster, and
/// Space again has to land it. Both new combinations that begin at a region
/// are here: ground to ground, and ground to hexagon.
#[wasm_bindgen_test]
async fn the_keyboard_draws_from_a_focused_ground_to_a_ground_and_to_a_hexagon() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let b = board(&container);

    // Tab to north's region, then Space: the line starts at the region.
    ground(&container, "north")
        .dispatch_event(&focus_event())
        .unwrap();
    settle().await;
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;
    assert!(
        live_text(&container).starts_with("Drawing a line from north"),
        "Space on a focused ground in link mode must start a line from the GROUP, not grab the \
         cluster: {:?}",
        live_text(&container)
    );

    // Three steps right from ana's cell is column 3: empty comb one step
    // outside south, which is south's ground.
    for _ in 0..3 {
        b.dispatch_event(&keydown("ArrowRight")).unwrap();
        settle().await;
    }
    assert_eq!(
        live_text(&container),
        "Link north to south.",
        "the status must name the region the line would land on"
    );
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    // And again, one cell further: eve's own hexagon.
    ground(&container, "north")
        .dispatch_event(&focus_event())
        .unwrap();
    settle().await;
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;
    for _ in 0..4 {
        b.dispatch_event(&keydown("ArrowRight")).unwrap();
        settle().await;
    }
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    assert_eq!(
        drawn.borrow().as_slice(),
        &[
            (Endpoint::Group(gid("north")), Endpoint::Group(gid("south"))),
            (Endpoint::Group(gid("north")), Endpoint::Tile(tid("eve"))),
        ],
        "both keyboard-drawn lines must be reported, in the order they were drawn"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// The third combination from the keyboard: a hexagon at the start and a
/// region at the end. The roving arrow selection is the only path to a
/// `selected` tile that never touches a pointer, exactly as `links.rs`'s own
/// keyboard test explains.
#[wasm_bindgen_test]
async fn the_keyboard_draws_from_a_selected_hexagon_to_a_ground() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let b = board(&container);

    b.dispatch_event(&keydown("ArrowRight")).unwrap();
    settle().await;
    assert!(
        live_text(&container).starts_with("ana"),
        "test precondition failed: the first arrow should select ana: {:?}",
        live_text(&container)
    );
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;
    for _ in 0..3 {
        b.dispatch_event(&keydown("ArrowRight")).unwrap();
        settle().await;
    }
    b.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    assert_eq!(
        drawn.borrow().as_slice(),
        &[(Endpoint::Tile(tid("ana")), Endpoint::Group(gid("south")))],
        "the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// ======================================================== the anchor

/// THE LINE FOLLOWS THE REGION IT IS DRAWN TO. A group end is met at whichever
/// member faces the other end, recomputed against the arrangement the drag is
/// PROPOSING — so dragging north one column right must move the line's own end
/// one column right with it, on screen, before anything is dropped.
///
/// Read off the `d` attribute the component computes, which is the only thing
/// a host ever gets: this is the unit-level rule in `link_views` arriving in a
/// real render, where a press, a preview and a repaint all have to line up.
#[wasm_bindgen_test]
async fn the_line_to_a_group_moves_with_the_ground_while_it_is_dragged() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();
    // NOT IN LINK MODE: this is an ordinary group DRAG, which is the gesture
    // that moves a region and therefore the gesture the line has to follow.
    let props = HoneycombProps {
        linking: false,
        ..base_props(Rc::new(two_districts_linked()))
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let b = board(&container);
    let path_now = || {
        container
            .query_selector("[data-link=\"spur\"] path")
            .unwrap()
            .expect("the line's hit path rendered")
            .get_attribute("d")
            .expect("a d attribute")
    };
    let before = path_now();
    let start = |d: &str| {
        let mut it = d.trim_start_matches("M ").split_whitespace();
        let x: f64 = it.next().unwrap().parse().unwrap();
        let y: f64 = it.next().unwrap().parse().unwrap();
        (x, y)
    };

    // Press north's ground and drag one column right. The whole region moves
    // rigidly, so bea — the member facing fay — goes from column 1 to column 2.
    let (dx, dy) = cell_client(&container, &b, "ana", Cell { col: 0, row: 0 }, NORTH_GROUND);
    ground(&container, "north")
        .dispatch_event(&pointer_event_at("pointerdown", dx, dy))
        .unwrap();
    settle().await;
    // After the press, for `drag`'s reason: focusing the board may have
    // scrolled it.
    let (ux, uy) = cell_client(
        &container,
        &b,
        "ana",
        Cell { col: 0, row: 0 },
        Cell { col: 1, row: 1 },
    );
    b.dispatch_event(&pointer_event_at("pointermove", ux, uy))
        .unwrap();
    settle().await;

    let (x0, y0) = start(&before);
    let (x1, y1) = start(&path_now());
    assert!(
        (x1 - x0 - L.step()).abs() < 0.05 && (y1 - y0).abs() < 0.05,
        "the line's group end must move with the ghosts — one column right is one step right — \
         but it went from ({x0}, {y0}) to ({x1}, {y1}); a line still leaving from the cell the \
         document records is a drag showing the user a board they are not about to get"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// ======================================================== refusals

/// DECISION 3, AT THE GESTURE. Start on a ground and let go on one of its own
/// hexagons — the likeliest mis-drag the group endpoint makes possible — and
/// the refusal has to arrive as prose in the live region BEFORE anything is
/// reported to the host, not as a struct and not as silence.
#[wasm_bindgen_test]
async fn a_line_from_a_ground_into_its_own_hexagon_is_refused_in_prose() {
    let (container, drawn, handle) = mounted(two_districts()).await;
    let north = ground(&container, "north");

    drag(&container, &north, NORTH_GROUND, Cell { col: 1, row: 0 }).await;

    let said = live_text(&container);
    assert!(
        said.contains("bea is already part of north"),
        "the refusal must say what is wrong and what to do instead, in words: {said:?}"
    );
    assert!(
        !said.contains("LinkToOwnMember") && !said.contains('{'),
        "no Debug dump may reach a screen reader: {said:?}"
    );
    assert!(
        drawn.borrow().is_empty(),
        "a refused line must never be reported to the host: {:?}",
        drawn.borrow()
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// DECISION 4. Removing the last hexagon of a group a line reaches is refused
/// by `rules.rs`, and what a user sees of that refusal is this sentence — which
/// has to name the links, because links can be removed and "disconnect these
/// first" is then advice that works.
#[wasm_bindgen_test]
async fn removing_the_last_hexagon_of_a_linked_group_is_refused_in_prose() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();
    let changes: Rc<RefCell<Vec<Change>>> = Rc::new(RefCell::new(Vec::new()));
    let on_change = {
        let changes = changes.clone();
        Callback::from(move |c: Change| changes.borrow_mut().push(c))
    };
    let props = HoneycombProps {
        linking: false,
        removable: true,
        on_change,
        ..base_props(Rc::new(a_group_of_one()))
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let b = board(&container);
    let sam = container
        .query_selector("[data-tile=\"sam\"]")
        .unwrap()
        .expect("sam is on the board");
    let (sx, sy) = cell_client(
        &container,
        &b,
        "sam",
        Cell { col: 0, row: 0 },
        Cell { col: 0, row: 0 },
    );
    sam.dispatch_event(&pointer_event_at("pointerdown", sx, sy))
        .unwrap();
    settle().await;
    sam.dispatch_event(&pointer_event_at("pointerup", sx, sy))
        .unwrap();
    settle().await;
    b.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;

    let said = live_text(&container);
    assert!(
        said.contains("sam is the last hexagon in solo") && said.contains("spur"),
        "the refusal must name the group AND the line in the way, since the line can be \
         removed: {said:?}"
    );
    assert!(
        !said.contains("LastMemberStillLinked") && !said.contains('{'),
        "no Debug dump may reach a screen reader: {said:?}"
    );
    assert!(
        changes.borrow().is_empty(),
        "nothing may have been applied: {:?}",
        changes.borrow()
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
