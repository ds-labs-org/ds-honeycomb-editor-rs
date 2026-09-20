//! Five keyboard-path defects, each already fixed in `src/lib.rs`, none of
//! them caught by any test that existed before this file — an audit reverted
//! each fix in turn and the whole suite, including `tests/dom.rs`, stayed
//! green.
//!
//! `tests/dom.rs` is the harness this file follows: mount the real component
//! in a real browser with `yew::Renderer`, drive real `PointerEvent` /
//! `KeyboardEvent` / `FocusEvent` objects through `dispatch_event`, let a
//! zero-length `setTimeout` (`settle`, below) stand in for "the render has
//! settled", and read the answer back out of the DOM — the live tile
//! elements, the `aria-live` text, and (new here) `document.active_element`.
//! See that file's module doc for why `#![cfg(target_arch = "wasm32")]` is
//! what keeps this out of `cargo test --workspace`'s native run.
//!
//! The five defects, and the commits that fixed them (`git log --oneline --
//! honeycomb-yew/src/lib.rs`):
//!
//! 1. `ec4af35`: a pointer press never called `.focus()` on the board, so a
//!    user who arrived by clicking never focused the element `onkeydown`
//!    lives on — every keyboard equivalent was dead for the rest of the
//!    session.
//! 2. `ec4af35`: `focused_group` was cleared only by the root's `onblur`,
//!    which a non-bubbling `blur` on a group's own wrapper never reaches —
//!    one Tab through a group region killed the arrow keys permanently.
//!    Fixed by clearing it on the root's own `onfocus` instead.
//! 3. `ec4af35`: a Space grab seeded `Drag.moves` with `Vec::new()`, so the
//!    grabbed tile faded to the `Dragging` opacity with no ghost painted on
//!    top of it — a keyboard user's first sight of their own grab was the
//!    tile disappearing.
//! 4. `ec4af35`: dropping a keyboard grab back where it started computed a
//!    `NoMove` status and threw it away, leaving the live region announcing
//!    the grab that had just ended — silence indistinguishable from the key
//!    doing nothing.
//! 5. `db801e1`: Delete/Backspace checked `removable` and `readonly` but not
//!    `focused_group`, so with the focus ring on a group's region the key
//!    removed whichever tile had last been clicked rather than nothing.

#![cfg(target_arch = "wasm32")]

use std::collections::BTreeMap;
use std::rc::Rc;

use honeycomb_yew::*;
use wasm_bindgen_test::*;
use web_sys::{Document, Element, FocusEvent, KeyboardEventInit, PointerEventInit};
use yew::prelude::*;

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> Document {
    web_sys::window()
        .expect("a window")
        .document()
        .expect("a document")
}

fn tid(s: &str) -> TileId {
    TileId(Slug::parse(s).unwrap())
}

/// A zero-length `setTimeout`, which is reliably after every microtask the
/// scheduler queued for a render — including the ones a state update inside
/// an event handler queues. Copied from `tests/dom.rs` rather than shared,
/// because each `wasm-bindgen-test` file is its own crate.
async fn settle() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

fn pointer_event(kind: &str) -> web_sys::PointerEvent {
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_button(0);
    init.set_pointer_id(1);
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

/// `focus` and `blur` do not bubble (see defect 2's own comment in
/// `src/lib.rs`, near line 1792), but `dispatch_event` invokes a listener
/// bound directly to its target regardless of bubbling — the same reason
/// `tests/dom.rs` can dispatch a `pointerdown` without a real user gesture.
fn focus_event() -> FocusEvent {
    FocusEvent::new("focus").expect("constructing a synthetic FocusEvent")
}

/// Pointerdown then pointerup with no pointermove between them is a
/// SELECTION, not a drag — see `onpointerup`'s "under the slop" branch in
/// `src/lib.rs`, and the identical comment in `tests/dom.rs`.
async fn click(el: &Element) {
    el.dispatch_event(&pointer_event("pointerdown")).unwrap();
    settle().await;
    el.dispatch_event(&pointer_event("pointerup")).unwrap();
    settle().await;
}

/// Two own tiles, ana and bea, with no group and no link — ana at column 0,
/// bea at column 1. The minimal board for anything that only needs two
/// selectable tiles in a known roving order and nothing else to complicate
/// the picture.
fn two_tiles() -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for name in ["ana", "bea"] {
        tiles.insert(
            tid(name),
            OwnTile {
                group: None,
                label: name.into(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
    }
    let mut cells = BTreeMap::new();
    cells.insert(tid("ana"), Cell { col: 0, row: 0 });
    cells.insert(tid("bea"), Cell { col: 1, row: 0 });
    Diagram::try_new(DiagramSpec {
        slug: Slug::parse("plan").unwrap(),
        label: "Plan".into(),
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
    .expect("two unlinked tiles is a legal standalone diagram")
}

/// A group "cluster" with two members (g1, g2) plus one standalone tile
/// (solo) that belongs to no group. The minimal board on which "a group has
/// keyboard focus" and "a tile is selected" can be true at the same time,
/// which is exactly the state defects 2 and 5 are about.
fn grouped_and_solo() -> Diagram {
    let gid = GroupId(Slug::parse("cluster").unwrap());
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for name in ["g1", "g2"] {
        tiles.insert(
            tid(name),
            OwnTile {
                group: Some(gid.clone()),
                label: name.into(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
    }
    tiles.insert(
        tid("solo"),
        OwnTile {
            group: None,
            label: "solo".into(),
            comment: None,
            style_key: None,
            extra: Vec::new(),
        },
    );
    let mut cells = BTreeMap::new();
    cells.insert(tid("g1"), Cell { col: 0, row: 0 });
    cells.insert(tid("g2"), Cell { col: 1, row: 0 });
    cells.insert(tid("solo"), Cell { col: 3, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        gid,
        Group {
            label: "Cluster".into(),
            style_key: None,
            note: None,
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
        links: BTreeMap::new(),
    })
    .expect("a two-tile group plus one standalone tile is a legal standalone diagram")
}

/// The props every test starts from. `tile` renders nothing — none of these
/// tests reads the host's own markup, only the wrapper `<g data-tile>` and
/// `<g data-group>` elements this crate draws around it, and the `aria-live`
/// text it owns outright.
fn base_props(diagram: Diagram, removable: bool) -> HoneycombProps {
    HoneycombProps {
        diagram: Rc::new(diagram),
        lattice: Lattice::new(46.0, 1.045),
        pad: 26.0,
        frame_ring: 1,
        tile: Callback::from(|_: TileView| html! {}),
        ground: None,
        frame: None,
        link: None,
        linking: false,
        on_link: Callback::noop(),
        on_change: Callback::noop(),
        on_reject: Callback::noop(),
        on_status: Callback::noop(),
        on_select: Callback::noop(),
        pending: None,
        on_pending: Callback::noop(),
        removable,
        readonly: false,
        class: Classes::new(),
        aria_label: AttrValue::from("test board"),
    }
}

fn live_text(container: &Element) -> String {
    container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered")
        .text_content()
        .unwrap_or_default()
}

// --------------------------------------------------------------- defect 1

/// Pins `ec4af35`'s "AND THEREFORE FOCUS THE BOARD BY HAND" fix in `begin`
/// (the shared press entry point every pointerdown handler funnels through).
///
/// Without that `el.focus()` call, `document.active_element` never moves off
/// whatever had it before the click — so a `KeyboardEvent` dispatched at
/// `document.active_element`, the only place a browser ever actually sends
/// one, never reaches the board's `onkeydown` at all. Dispatching a keydown
/// directly ON the board (as every other test in this file does, once a
/// keyboard user is assumed to already be there) would not catch this: the
/// listener fires either way. Only reading `document.active_element` back
/// tells the two cases apart, so that is what this test does, and then, to
/// show the consequence rather than just the mechanism, goes on to dispatch
/// ArrowRight at exactly that element the way a real browser would.
#[wasm_bindgen_test]
async fn a_pointer_press_focuses_the_board_so_the_keyboard_path_survives_a_click() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(
        container.clone(),
        base_props(two_tiles(), false),
    )
    .render();
    settle().await;
    settle().await;

    let ana = container
        .query_selector("[data-tile=\"ana\"]")
        .unwrap()
        .expect("ana is on the board");

    // Precondition: nothing has focused the board yet, so there is a real
    // transition for the click to make.
    let before = document().active_element();
    assert!(
        before
            .and_then(|e| e.get_attribute("class"))
            .is_none_or(|c| !c.contains("hc-board")),
        "test precondition failed: the board must not already hold focus"
    );

    click(&ana).await;

    let after = document().active_element();
    assert!(
        after
            .as_ref()
            .and_then(|e| e.get_attribute("class"))
            .is_some_and(|c| c.contains("hc-board")),
        "a pointer press on a tile must move DOM focus to the board itself, \
         so onkeydown can ever fire again; document.active_element was {:?}",
        after.and_then(|e| e.get_attribute("class"))
    );

    // The consequence: a real browser sends every keystroke to whatever is
    // ACTUALLY focused, not to the board by name. If focus never moved,
    // this ArrowRight lands on the wrong element and roving selection never
    // happens.
    let target = document()
        .active_element()
        .expect("something is focused after the click");
    target.dispatch_event(&keydown("ArrowRight")).unwrap();
    settle().await;

    let bea = container
        .query_selector("[data-tile=\"bea\"]")
        .unwrap()
        .expect("bea is on the board");
    assert!(
        bea.get_attribute("class")
            .is_some_and(|c| c.contains("is-selected")),
        "ArrowRight sent to the real focus target should rove the selection \
         onto bea, which only happens if the click actually focused the board"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// --------------------------------------------------------------- defect 2

/// Pins `ec4af35`'s move of the `focused_group` clear from the root's
/// `onblur` to the root's `onfocus`.
///
/// `focus`/`blur` do not bubble, so nothing about a group wrapper losing
/// focus ever reaches the root — the only reliable signal that a stale group
/// focus should be forgotten is the root itself becoming focused again. This
/// test puts `focused_group` in exactly that stale state (focus a group,
/// then send focus back to the board without ever blurring the group — the
/// same event sequence a non-bubbling blur would leave the root unable to
/// observe) and then checks the one place a stale `focused_group` is
/// observable from outside: the roving-selection guard a few lines below it,
/// which swallows ArrowRight whenever `focused_group.is_some()`.
#[wasm_bindgen_test]
async fn focused_group_is_cleared_when_the_board_itself_regains_focus() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(
        container.clone(),
        base_props(grouped_and_solo(), false),
    )
    .render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let group = container
        .query_selector("[data-group=\"cluster\"]")
        .unwrap()
        .expect("the cluster group region mounted");

    // Focus the group region (what one Tab into it does), then send focus
    // back to the board without an intervening blur on the group — the
    // observable end state a non-bubbling blur leaves behind.
    group.dispatch_event(&focus_event()).unwrap();
    settle().await;
    board.dispatch_event(&focus_event()).unwrap();
    settle().await;

    assert_eq!(
        live_text(&container),
        "",
        "test precondition failed: nothing should have spoken yet"
    );

    board.dispatch_event(&keydown("ArrowRight")).unwrap();
    settle().await;

    assert!(
        live_text(&container).contains("column"),
        "ArrowRight after the board regains focus should rove the tile \
         selection and announce a cell, which only happens if focused_group \
         was cleared; the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// --------------------------------------------------------------- defect 3

/// Pins `ec4af35`'s change of the Space-grab's seeded `Drag.moves` from
/// `Vec::new()` to `diagram.moving_set(...)`.
///
/// The instant a `Drag` exists, the whole moving set is painted at the
/// `Dragging` opacity (`state_of`, in `src/lib.rs`) — that part never
/// depended on `moves`. What depends on `moves` is the ghost layer, drawn
/// from `plan_moves` a few lines below it: with an empty `moves`, a Space
/// grab faded the real tile out and painted no ghost anywhere to replace it.
/// So the fix is invisible in the "is the tile now dragging" state and only
/// visible in whether a second, `hc-tile--ghost`-classed element with the
/// same `data-tile` now exists.
#[wasm_bindgen_test]
async fn a_space_grab_seeds_the_ghost_preview_so_the_grabbed_tile_is_not_invisible() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(
        container.clone(),
        base_props(two_tiles(), false),
    )
    .render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let ana = container
        .query_selector("[data-tile=\"ana\"]")
        .unwrap()
        .expect("ana is on the board");

    click(&ana).await;
    board.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    let ghost = container
        .query_selector(".hc-tile--ghost[data-tile=\"ana\"]")
        .unwrap();
    assert!(
        ghost.is_some(),
        "grabbing ana with Space must paint a ghost of it, or a keyboard \
         user sees only the real tile fade out with nothing put in its place"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// --------------------------------------------------------------- defect 4

/// Pins `ec4af35`'s addition of the `Err(Rejection::NoMove)` arm in the
/// Space/Enter drop handler, which used to fall into the same `commit(cmd,
/// status)` as every other verdict and so never spoke `status` at all for a
/// no-op.
///
/// Grabbing ana announces "Holding ana." (`holding`, in `src/lib.rs`).
/// Dropping it immediately, with no arrow key in between, resolves to the
/// same cell it started on — `Rejection::NoMove` — and the fix's whole
/// contract is that the live region says something ELSE at that point rather
/// than leaving "Holding ana." standing as if the grab were still open.
#[wasm_bindgen_test]
async fn dropping_a_keyboard_grab_in_place_still_announces_something() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(
        container.clone(),
        base_props(two_tiles(), false),
    )
    .render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let ana = container
        .query_selector("[data-tile=\"ana\"]")
        .unwrap()
        .expect("ana is on the board");

    click(&ana).await;
    board.dispatch_event(&keydown(" ")).unwrap();
    settle().await;
    assert_eq!(
        live_text(&container),
        "Holding ana.",
        "test precondition failed: grabbing ana should announce holding it"
    );

    // Drop immediately, with no arrow key in between: the candidate is still
    // ana's own cell, so this is a no-op move.
    board.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    assert!(
        live_text(&container).contains("already is"),
        "dropping a keyboard grab back where it started must announce that \
         nothing moved rather than leaving the old \"Holding\" announcement \
         standing; the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// --------------------------------------------------------------- defect 5

/// Pins `db801e1`'s addition of `focused_group.is_some()` to Delete/
/// Backspace's guard, alongside the pre-existing `!removable || readonly`.
///
/// `solo` is selected (by a pointer click) while the `cluster` group region
/// holds keyboard focus — the exact state a Tab from the selected tile onto
/// a group's region, without ever pressing Delete in between, leaves behind.
/// Without the `focused_group` guard, Delete acts on `selected` regardless
/// of where the focus ring actually is and removes `solo` anyway; with it,
/// Delete is refused outright and neither the selection nor the live region
/// changes.
#[wasm_bindgen_test]
async fn delete_is_refused_while_a_group_holds_keyboard_focus_even_with_a_tile_selected() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(
        container.clone(),
        base_props(grouped_and_solo(), true),
    )
    .render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let group = container
        .query_selector("[data-group=\"cluster\"]")
        .unwrap()
        .expect("the cluster group region mounted");
    let solo = container
        .query_selector("[data-tile=\"solo\"]")
        .unwrap()
        .expect("solo is on the board");

    click(&solo).await;
    assert!(
        solo.get_attribute("class")
            .is_some_and(|c| c.contains("is-selected")),
        "test precondition failed: clicking solo should select it"
    );
    // The press itself announces "Holding solo." (`begin`, in `src/lib.rs`,
    // calls `holding` before anything else). That is the baseline a refused
    // Delete must leave undisturbed.
    let before = live_text(&container);
    assert_eq!(
        before, "Holding solo.",
        "test precondition failed: selecting solo should announce holding it"
    );

    // Move the focus ring onto the group region without touching the
    // selection — a plain Tab does exactly this.
    group.dispatch_event(&focus_event()).unwrap();
    settle().await;

    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    settle().await;

    assert!(
        solo.get_attribute("class")
            .is_some_and(|c| c.contains("is-selected")),
        "Delete with a group focused must not touch a tile the user is not \
         pointing at: solo should still be selected and on the board"
    );
    assert_eq!(
        live_text(&container),
        before,
        "a Delete refused by the focused_group guard must not announce a \
         removal that never happened; the live region said {:?}",
        live_text(&container)
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
