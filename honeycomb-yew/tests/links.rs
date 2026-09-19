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
        label: label.to_string(),
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
            from: tid("ana"),
            to: tid("bea"),
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
