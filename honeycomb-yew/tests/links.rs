//! Findings 1 and 2 of the seven-dimension audit, both about links and both
//! reachable only by a real browser: a link, once drawn, had no removal path
//! at all (finding 1), and neither a line nor a dragless tile move could be
//! made from the keyboard or from a single pointer without a sustained drag
//! (finding 2).
//!
//! Follows `dom.rs`'s pattern exactly: mount the real component in a real
//! headless Chromium with `yew::Renderer`, drive real `PointerEvent` /
//! `KeyboardEvent` / `FocusEvent` objects through `dispatch_event`, `settle`
//! past whatever the scheduler queued, and read the answer back out of the
//! DOM. `#![cfg(target_arch = "wasm32")]` is the same reason it is in every
//! sibling file: on the host target this compiles to an empty module and
//! `cargo test --workspace` reports zero tests for it, which is correct —
//! this crate has no browser on that target to run any of it in.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
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

fn own(label: &str) -> OwnTile {
    OwnTile {
        group: None,
        label: label.into(),
        comment: None,
        style_key: None,
        extra: Vec::new(),
    }
}

/// A zero-length `setTimeout`, reliably after every microtask the scheduler
/// queued for a render. Copied rather than shared: each `wasm-bindgen-test`
/// file is its own crate, with no path between them.
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
/// bound directly to its target regardless — the same reason `keyboard.rs`
/// can dispatch one with no real user gesture behind it.
fn focus_event() -> FocusEvent {
    FocusEvent::new("focus").expect("constructing a synthetic FocusEvent")
}

/// Pointerdown then pointerup with no move between them is a SELECTION, not a
/// drag — see `onpointerup`'s "under the slop" branch in `src/lib.rs`.
async fn click(el: &Element) {
    el.dispatch_event(&pointer_event("pointerdown")).unwrap();
    settle().await;
    el.dispatch_event(&pointer_event("pointerup")).unwrap();
    settle().await;
}

fn live_text(container: &Element) -> String {
    container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered")
        .text_content()
        .unwrap_or_default()
}

/// Two own tiles, ana and bea, joined by a link named corridor. The minimal
/// board on which `Rejection::StillLinked` bites — removing ana is refused
/// while corridor still connects it — and on which removing corridor is the
/// fix finding 1 adds.
fn linked_pair() -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for name in ["ana", "bea"] {
        tiles.insert(tid(name), own(name));
    }
    let mut cells = BTreeMap::new();
    cells.insert(tid("ana"), Cell { col: 0, row: 0 });
    cells.insert(tid("bea"), Cell { col: 1, row: 0 });
    let mut links = BTreeMap::new();
    links.insert(
        LinkId(Slug::parse("corridor").unwrap()),
        Link {
            // TILE ENDS, in a fixture about what a link's REMOVAL does. A group
            // end is a different fixture's job — the anchor moves, and nothing
            // these tests assert is about where the line meets a hexagon.
            from: Endpoint::Tile(tid("ana")),
            to: Endpoint::Tile(tid("bea")),
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
        groups: BTreeMap::new(),
        content: Content::Standalone { tiles },
        cells,
        extra: Vec::new(),
        links,
    })
    .expect("two tiles and a link between them is a legal standalone diagram")
}

/// Two own tiles, neither linked nor grouped, side by side.
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

fn base_props(diagram: Rc<Diagram>) -> HoneycombProps {
    HoneycombProps {
        diagram,
        lattice: Lattice::new(46.0, 1.045),
        pad: 26.0,
        frame_ring: 1,
        tile: Callback::from(|_: TileView| html! {}),
        ground: None,
        frame: None,
        link: Some(Callback::from(|_: LinkView| html! { <path /> })),
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

// ============================================================= finding 1

/// FINDING 1. `Command::Disconnect` existed with nothing anywhere
/// constructing one: no gesture, no button, no key removed a link. This pins
/// the keyboard path this crate now offers — Tab reaches a link's own tab
/// stop (`hc-linkhit`, `src/lib.rs`) the same way it already reaches a
/// group's, and Delete there is `Command::Disconnect`.
///
/// Also pins the regression `Command::Disconnect`'s own removal from the DOM
/// creates: removing the element that holds keyboard focus would otherwise
/// strand focus on `<body>`, killing every later keystroke — `onkeydown`'s
/// `focused_link` branch moves focus back to the board first.
#[wasm_bindgen_test]
async fn a_focused_link_is_removed_by_delete_and_focus_returns_to_the_board() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let props = HoneycombProps {
        removable: true,
        ..base_props(Rc::new(linked_pair()))
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");
    let link = container
        .query_selector("[data-link=\"corridor\"]")
        .unwrap()
        .expect("corridor's hit region mounted");

    // Tab reaching the link, simulated the way `keyboard.rs` simulates Tab
    // reaching a group: dispatching `focus` directly at the element, since a
    // synthetic KeyboardEvent for Tab does not drive real browser focus
    // navigation.
    link.dispatch_event(&focus_event()).unwrap();
    settle().await;

    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    settle().await;

    assert_eq!(
        live_text(&container),
        "Removed the line from ana to bea.",
        "Delete on a focused link must remove it and say so in prose, not silence or a Debug \
         dump"
    );

    let after = document().active_element();
    assert!(
        after
            .as_ref()
            .and_then(|e| e.get_attribute("class"))
            .is_some_and(|c| c.contains("hc-board")),
        "removing the focused link's own element must not strand keyboard focus on <body>; \
         active_element was {:?}",
        after.and_then(|e| e.get_attribute("class"))
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// THE END-TO-END CASE FINDING 1 IS ABOUT: a mis-aimed link used to make its
/// two tiles permanently undeletable except by Undo taken immediately.
/// Removing the link first is now the offer `StillLinked`'s own doc says a
/// host should be able to make — and the tile it was holding stuck becomes
/// removable again the moment it is gone.
#[wasm_bindgen_test]
async fn removing_a_blocking_link_frees_the_tile_it_was_stuck_to() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    // THE COMPONENT IS CONTROLLED (see `HoneycombProps::diagram`'s own doc):
    // it never mutates its copy, it reports an accepted `Command` through
    // `on_change` and waits to be handed the new diagram back as a prop —
    // exactly the way `pointer.rs`'s own removal test drives it, and for the
    // same reason: this test checks that corridor and then ana actually
    // leave the DOM, which only happens once the component is re-rendered
    // against a diagram that no longer has them.
    let changes: Rc<RefCell<Vec<Change>>> = Rc::new(RefCell::new(Vec::new()));
    let on_change = {
        let changes = changes.clone();
        Callback::from(move |c: Change| changes.borrow_mut().push(c))
    };

    let props = HoneycombProps {
        removable: true,
        on_change,
        ..base_props(Rc::new(linked_pair()))
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
    let link = container
        .query_selector("[data-link=\"corridor\"]")
        .unwrap()
        .expect("corridor's hit region mounted");

    // Select ana and try to remove it: refused, because corridor still
    // reaches it. Establishes the stuck state finding 1 is about.
    click(&ana).await;
    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    assert!(
        live_text(&container).contains("still connected"),
        "test precondition failed: ana should still be stuck behind corridor: {}",
        live_text(&container)
    );
    assert!(
        changes.borrow().is_empty(),
        "test precondition failed: the refusal must not have reported a Change"
    );

    // Remove the link instead, from the keyboard, and hand the resulting
    // diagram back to the component the way a real host would.
    link.dispatch_event(&focus_event()).unwrap();
    settle().await;
    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    settle().await;
    let change = changes
        .borrow_mut()
        .pop()
        .expect("removing corridor should have reported a Change");
    assert!(matches!(change.applied, Command::Disconnect { .. }));
    handle.update(HoneycombProps {
        removable: true,
        on_change: Callback::noop(),
        ..base_props(change.diagram.clone())
    });
    settle().await;
    settle().await;
    assert!(
        container
            .query_selector("[data-link=\"corridor\"]")
            .unwrap()
            .is_none(),
        "corridor should be gone from the DOM once removed"
    );

    // ana is stuck no longer, and still selected from the very first click
    // above (nothing in this test has touched `selected` since — clicking it
    // AGAIN here would pick it up rather than select it again, finding 2's
    // own new gesture, which is not what this step is testing). Delete
    // reaches it directly.
    assert!(
        container
            .query_selector("[data-tile=\"ana\"]")
            .unwrap()
            .is_some(),
        "ana should still be on the board, now unblocked"
    );
    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    settle().await;

    assert_eq!(
        live_text(&container),
        "Removed ana.",
        "once corridor is gone, ana must be an ordinary removable tile again"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

// ============================================================= finding 2

/// FINDING 2, HALF ONE. `onkeydown`'s own doc used to say "a line is a
/// pointer gesture — the keyboard equivalent of linking is the host's form",
/// and no host had built one; meanwhile Space on a selected tile built a MOVE
/// grip unconditionally, so toggling Link mode announced itself
/// (`aria-pressed="true"`) and then moved the tile anyway the moment a
/// keyboard user pressed Space. This pins the fix: with `linking` on, Space
/// on a selected tile draws a line instead, and no `Translate` is ever
/// committed for it.
#[wasm_bindgen_test]
async fn space_while_linking_draws_a_line_from_the_keyboard_instead_of_moving_the_tile() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    // `on_link` CARRIES A PAIR OF `Endpoint`s NOW, and it carried a pair of
    // `TileId`s when this test was written. Either end of a line may name a
    // whole group; `Endpoint::Tile` is what a press on a hexagon hands back,
    // so this spells exactly the same two ends the assertion below always
    // meant. Nothing about what this test pins has changed.
    let links: Rc<RefCell<Vec<(Endpoint, Endpoint)>>> = Rc::new(RefCell::new(Vec::new()));
    let on_link = {
        let links = links.clone();
        Callback::from(move |(a, b): (Endpoint, Endpoint)| links.borrow_mut().push((a, b)))
    };
    let moves: Rc<RefCell<Vec<Change>>> = Rc::new(RefCell::new(Vec::new()));
    let on_change = {
        let moves = moves.clone();
        Callback::from(move |c: Change| moves.borrow_mut().push(c))
    };

    let props = HoneycombProps {
        linking: true,
        on_link,
        on_change,
        ..base_props(Rc::new(two_tiles("ana", "bea")))
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    settle().await;
    settle().await;

    let board = container
        .query_selector("svg.hc-board")
        .unwrap()
        .expect("the board mounted");

    // SELECTED BY ARROW, NOT BY CLICK. A press on a tile while `linking` is
    // on already builds `Grip::Linking` (`ontiledown`), so clicking ana here
    // would start drawing on the FIRST click rather than selecting it — which
    // is fine for a pointer user but leaves nothing for a keyboard-only user
    // to select with. The roving arrow selection is not gated on `linking` at
    // all, so it is the one path that reaches a `selected` tile without ever
    // touching a pointer — exactly what this test needs to isolate the
    // keyboard-only case finding 2 is about.
    board.dispatch_event(&keydown("ArrowRight")).unwrap();
    settle().await;
    assert!(
        live_text(&container).starts_with("ana"),
        "test precondition failed: ArrowRight with nothing selected should select ana first: {:?}",
        live_text(&container)
    );

    board.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    assert!(
        live_text(&container).starts_with("Drawing a line from ana"),
        "Space on a selected tile with Link mode on must start a line, not a move — the live \
         region said {:?}",
        live_text(&container)
    );
    assert!(
        !live_text(&container).starts_with("Holding ana"),
        "the mode must not silently announce a move grab: {:?}",
        live_text(&container)
    );

    // ArrowRight: from ana's cell (col 0, row 0), the axial step (+1, 0)
    // lands exactly on bea's cell (col 1, row 0).
    board.dispatch_event(&keydown("ArrowRight")).unwrap();
    settle().await;
    board.dispatch_event(&keydown(" ")).unwrap();
    settle().await;

    assert_eq!(
        links.borrow().as_slice(),
        &[(Endpoint::Tile(tid("ana")), Endpoint::Tile(tid("bea")))],
        "drawing from the keyboard must report the link exactly once"
    );
    assert!(
        moves.borrow().is_empty(),
        "no Translate should ever have been committed while linking: {:?}",
        moves.borrow()
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// FINDING 2, HALF TWO (WCAG 2.1 SC 2.5.7, Dragging Movements). Moving a
/// tile with a pointer used to be drag-only: a press-and-release with no
/// movement in between was always a SELECTION, with no way to complete a move
/// through a single pointer without sustaining a drag. This pins the fix: a
/// second click on the tile already selected picks it up (paints a ghost,
/// same as a keyboard Space grab), and a third click elsewhere — still no
/// drag, ever — drops it there as an ordinary `Translate`.
#[wasm_bindgen_test]
async fn a_second_click_on_the_selected_tile_picks_it_up_for_a_dragless_move() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

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
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
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

    // First click: an ordinary selection, exactly as today.
    click(&ana).await;
    assert!(
        ana.get_attribute("class")
            .is_some_and(|c| c.contains("is-selected")),
        "test precondition failed: the first click should select ana"
    );

    // Second click on the SAME, already-selected tile: picks it up. No
    // pointermove has fired anywhere in this test, so this is not a drag by
    // any definition the component uses.
    click(&ana).await;
    let ghost = container
        .query_selector(".hc-tile--ghost[data-tile=\"ana\"]")
        .unwrap();
    assert!(
        ghost.is_some(),
        "a second click on the selected tile must pick it up and paint a ghost, the pointer \
         equivalent of a keyboard Space grab"
    );

    // Third click, on empty comb well clear of both tiles: drops it there.
    let rect = board.get_bounding_client_rect();
    let (x, y) = (
        rect.left() + rect.width() * 0.9,
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

    let change = changes.borrow_mut().pop().expect(
        "a click on empty comb after picking ana up should have moved it — no pointer ever \
         moved between any of these events, so a drag-only implementation would have done \
         nothing at all",
    );
    match change.applied {
        Command::Translate { grabbed, delta, .. } => {
            assert_eq!(
                grabbed,
                tid("ana"),
                "the tile that was picked up must be the one moved"
            );
            assert!(
                delta.q != 0 || delta.r != 0,
                "ana must actually have moved somewhere: delta was {delta:?}"
            );
        }
        other => panic!("expected a Translate, got {other:?}"),
    }

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
