//! Regression tests for four pointer-observable defects, none of which any
//! existing test — native or `dom.rs` — catches: an audit found that reverting
//! any one of their fixes leaves the whole suite green.
//!
//! Each test follows `dom.rs`'s pattern: mount the real component in a real
//! browser, drive real `PointerEvent`s (respecting the component's own
//! [`PRESS_SLOP`]-equivalent — a plain down/up with no move in between is a
//! selection, never a drag), and read the result back from the DOM rather than
//! from anything private to the component.
//!
//! `#![cfg(target_arch = "wasm32")]` keeps this file out of `cargo test
//! --workspace`'s native run entirely, exactly as it does in `dom.rs`: on the
//! host target the module is empty, and it exists at all only under `--target
//! wasm32-unknown-unknown`, driven by `wasm_bindgen_test_configure!(run_in_browser)`.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use honeycomb_yew::*;
use wasm_bindgen_test::*;
use web_sys::{Document, Element, KeyboardEventInit, PointerEventInit};
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

fn own(label: &str) -> OwnTile {
    OwnTile {
        group: None,
        label: label.to_string(),
        comment: None,
        style_key: None,
        extra: Vec::new(),
    }
}

/// One own tile, alone at (0, 0). The minimal board on which the content frame
/// is a single cell — the shape defect 4 needs, since shrinking a single-cell
/// bounding box by even one ring collapses it to nothing.
fn one_tile(name: &str) -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    tiles.insert(tid(name), own(name));
    let mut cells = BTreeMap::new();
    cells.insert(tid(name), Cell { col: 0, row: 0 });
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
    .expect("one tile is a legal standalone diagram")
}

/// Two own tiles, neither linked nor grouped, side by side — a board on which
/// removing either one is an ordinary, unrefused edit.
fn two_tiles(a: &str, b: &str) -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    tiles.insert(tid(a), own(a));
    tiles.insert(tid(b), own(b));
    let mut cells = BTreeMap::new();
    cells.insert(tid(a), Cell { col: 0, row: 0 });
    cells.insert(tid(b), Cell { col: 1, row: 0 });
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

/// An armed palette chip offering a brand new own tile under `id`.
fn pending_chip(id: &str) -> Pending {
    Pending {
        id: tid(id),
        what: NewTile::Own(Box::new(own(id))),
    }
}

/// Every prop `Honeycomb` needs, with no handler wired to anything and no
/// palette armed — callers override exactly the fields their test is about via
/// `..base_props(diagram)`, the same shape `dom.rs`'s single literal has, just
/// factored out because this file needs it four times.
///
/// TAKES THE `Rc` ALREADY MADE, rather than an owned `Diagram`, because the
/// removal test below has to rebuild props around the diagram `on_change`
/// reported — the component is controlled and never mutates its own copy — and
/// that diagram already arrives as an `Rc<Diagram>` inside `Change`.
fn base_props(diagram: Rc<Diagram>) -> HoneycombProps {
    HoneycombProps {
        diagram,
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
        removable: false,
        readonly: false,
        class: Classes::new(),
        aria_label: AttrValue::from("test board"),
    }
}

/// A zero-length `setTimeout`, which is reliably after every microtask the
/// scheduler queued for a render — including the ones a state update inside an
/// event handler queues. Copied from `dom.rs` rather than shared: each
/// `wasm-bindgen-test` file is its own crate, with no path between them.
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

/// The client-less form: (0, 0) is fine for a plain select-by-click, which
/// never reads the coordinates at all, and for a `pointerup` that only ever
/// reads state recorded on the matching `pointerdown`.
fn pointer_event(kind: &str) -> web_sys::PointerEvent {
    pointer_event_at(kind, 0.0, 0.0)
}

fn keydown(key: &str) -> web_sys::KeyboardEvent {
    let init = KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_key(key);
    web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
        .expect("constructing a synthetic KeyboardEvent")
}

/// DEFECT 1. `ontiledown` sits on the tile and the board's own `onpointerdown`
/// sits on the root; both run on the same bubbling event, and the tile's runs
/// first. Before the fix it built an ordinary `Grip::Tile` for whatever was
/// pressed regardless of an armed chip, overwriting the armed press — so aiming
/// a palette chip at an occupied cell silently disarmed it and grabbed the
/// tile underneath instead. The fix makes `ontiledown` a no-op while a chip is
/// armed, so the event keeps bubbling to the board, which treats the press as
/// a placement attempt at that tile's own cell — refused, because the tile is
/// there.
#[wasm_bindgen_test]
async fn an_armed_chip_pressed_onto_an_occupied_tile_is_a_placement_attempt_not_a_grab() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let props = HoneycombProps {
        removable: true,
        pending: Some(pending_chip("cobbler")),
        ..base_props(Rc::new(one_tile("hall")))
    };

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let hall = container
        .query_selector("[data-tile=\"hall\"]")
        .unwrap()
        .expect("hall is on the board");
    let rect = hall.get_bounding_client_rect();
    let (cx, cy) = (
        rect.left() + rect.width() / 2.0,
        rect.top() + rect.height() / 2.0,
    );

    hall.dispatch_event(&pointer_event_at("pointerdown", cx, cy))
        .unwrap();
    settle().await;

    let live = container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered");
    let text = live.text_content().unwrap_or_default();
    assert!(
        !text.contains("Holding"),
        "a press on an occupied tile while a chip was armed grabbed the tile instead of \
         attempting to drop the chip there: {text:?}"
    );
    assert!(
        text.contains("cobbler") && text.contains("hall"),
        "the refusal should name both the armed chip and the tile blocking it: {text:?}"
    );

    hall.dispatch_event(&pointer_event_at("pointerup", cx, cy))
        .unwrap();
    settle().await;

    assert!(
        container
            .query_selector("[data-tile=\"cobbler\"]")
            .unwrap()
            .is_none(),
        "cobbler should not have been placed on top of hall"
    );
    assert!(
        !hall
            .get_attribute("class")
            .unwrap_or_default()
            .contains("is-selected"),
        "hall must not become the pointer's selection just because it was pressed while a chip \
         was armed"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// DEFECT 2. `readonly` was checked in `begin` (the ordinary drag/grab path),
/// in the keyboard grab and in the removal branch — and in none of the places
/// that arm and complete a PALETTE placement, so a readonly board with a chip
/// armed could still be added to. `holding()`'s "ready to place" text is
/// exactly what the arming effect writes to the live region the moment it
/// arms a chip, so a readonly board that stayed silent proves the arm itself
/// was refused; the tap that follows (`onpointerdown` then `onpointerup` with
/// no move, the accessible-by-touch path `onboarddown`'s own comment argues
/// for) proves a readonly board cannot be completed into a placement either,
/// if something upstream ever did arm it.
#[wasm_bindgen_test]
async fn a_readonly_board_refuses_an_armed_placement() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let props = HoneycombProps {
        readonly: true,
        pending: Some(pending_chip("annex")),
        ..base_props(Rc::new(one_tile("hall")))
    };

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let live = container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered");
    assert_eq!(
        live.text_content().unwrap_or_default(),
        "",
        "a readonly board armed a pending chip and announced it as ready to place"
    );

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let rect = board.get_bounding_client_rect();
    let (x, y) = (
        rect.left() + rect.width() * 0.75,
        rect.top() + rect.height() * 0.5,
    );
    board
        .dispatch_event(&pointer_event_at("pointerdown", x, y))
        .unwrap();
    settle().await;
    board
        .dispatch_event(&pointer_event_at("pointerup", x, y))
        .unwrap();
    settle().await;

    assert!(
        container
            .query_selector("[data-tile=\"annex\"]")
            .unwrap()
            .is_none(),
        "a readonly board with an armed chip placed a tile anyway"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// DEFECT 3. `onpointerup`'s "released off the board" branch and the keyboard
/// Delete handler both remove the same way, but only the keyboard one used to
/// clear `selected` on success — so dragging the selected tile off the board
/// left the selection dangling on a tile the diagram no longer has. The next
/// Delete then tries to remove it again, `check` refuses with
/// `Rejection::UnknownTile`, and the live region reports the just-removed tile
/// as "not on this board" instead of doing nothing, which is what it does when
/// nothing is selected.
#[wasm_bindgen_test]
async fn removing_the_selected_tile_by_dragging_it_off_the_board_clears_the_selection() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    // THE COMPONENT IS CONTROLLED (see `HoneycombProps::diagram`'s own doc): it
    // never mutates its copy, it reports an accepted `Command` through
    // `on_change` and waits to be handed the new diagram back as a prop, same
    // as a real host would. Recorded here rather than applied inline so the
    // test drives `AppHandle::update` the same way a host's own state update
    // would.
    let changes: Rc<RefCell<Vec<Change>>> = Rc::new(RefCell::new(Vec::new()));
    let on_change = {
        let changes = changes.clone();
        Callback::from(move |c: Change| changes.borrow_mut().push(c))
    };

    let props = HoneycombProps {
        removable: true,
        on_change,
        ..base_props(Rc::new(two_tiles("ana", "bea")))
    };

    let mut handle =
        yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
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

    // A plain click selects ana: pointerdown then pointerup with no move
    // between them is under the slop, so `onpointerup` treats it as a
    // selection rather than a zero-length drag.
    ana.dispatch_event(&pointer_event("pointerdown")).unwrap();
    settle().await;
    ana.dispatch_event(&pointer_event("pointerup")).unwrap();
    settle().await;
    assert!(
        ana.get_attribute("class")
            .unwrap_or_default()
            .contains("is-selected"),
        "ana should already be selected before the drag under test starts"
    );

    // Drag ana clear of the board. `pointer_event`'s (0, 0) is where the press
    // is recorded as starting; the move to deep negative territory both
    // crosses PRESS_SLOP and lands outside the board's own bounding rect, so
    // `off_board` reports it as a removal rather than a move to an empty cell.
    ana.dispatch_event(&pointer_event("pointerdown")).unwrap();
    settle().await;
    board
        .dispatch_event(&pointer_event_at("pointermove", -9999.0, -9999.0))
        .unwrap();
    settle().await;
    board.dispatch_event(&pointer_event("pointerup")).unwrap();
    settle().await;

    let change = changes
        .borrow_mut()
        .pop()
        .expect("dragging ana off the board should have been accepted as a removal");
    handle.update(HoneycombProps {
        removable: true,
        on_change: Callback::noop(),
        ..base_props(change.diagram.clone())
    });
    settle().await;
    settle().await;

    assert!(
        container
            .query_selector("[data-tile=\"ana\"]")
            .unwrap()
            .is_none(),
        "ana should have been removed by dragging it off the board"
    );
    let live = container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered");
    assert_eq!(
        live.text_content().unwrap_or_default(),
        "Removed ana.",
        "sanity check: the removal itself should have gone through cleanly"
    );

    // THE PIN. If the selection still names ana, this Delete tries to remove
    // it a second time; `check` refuses with `Rejection::UnknownTile` and the
    // live region reports ana as missing instead of doing nothing, which is
    // what it does when nothing is selected.
    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;

    let text = live.text_content().unwrap_or_default();
    assert_eq!(
        text, "Removed ana.",
        "a pointer removal must clear the selection, or the next Delete finds a dangling \
         selection and reports the tile it just removed as missing instead of doing nothing: \
         {text:?}"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// DEFECT 4. `Frame::around(ring(&content, props.frame_ring)...)` used to read
/// the prop unclamped. With one tile the content frame is a single cell, and
/// shrinking a single-cell bounding box inward by even one ring — `frame_ring:
/// -1` — makes the row range and the column range both empty; `ring` returns
/// no cells, and `Frame::around` returns `None` for an empty iterator, which
/// the component unwrapped with `.expect(...)`. That panicked on the very
/// first render, and under `panic = "abort"` a panic here does not unwind
/// into a catchable error, it kills the page.
#[wasm_bindgen_test]
async fn a_negative_frame_ring_renders_instead_of_panicking() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let props = HoneycombProps {
        frame_ring: -1,
        ..base_props(Rc::new(one_tile("hall")))
    };

    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let board = container.query_selector("svg.hc-board").unwrap();
    assert!(
        board.is_some(),
        "a negative frame_ring should render a board rather than panicking on mount"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
