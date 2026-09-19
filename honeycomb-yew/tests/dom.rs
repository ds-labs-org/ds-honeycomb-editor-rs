//! The end-to-end version of defect 2, at the level a screen-reader user
//! experiences it. `src/lib.rs`'s `every_rejection_has_prose_and_none_leaks_its_debug`
//! proves `unknown()` RETURNS a string with no `{` in it; it says nothing about
//! whether that string ever reaches the DOM. This file mounts the real
//! component in a real browser, drives the exact gesture a keyboard user would
//! — select a linked tile, press Delete — and reads the `aria-live` region a
//! screen reader would actually be listening to.
//!
//! `#![cfg(target_arch = "wasm32")]` keeps this file out of `cargo test
//! --workspace`'s native run entirely: on the host target the module is empty
//! and the binary passes with zero tests, and it exists at all only under
//! `--target wasm32-unknown-unknown`, run in a browser via
//! `wasm_bindgen_test_configure!(run_in_browser)`.

#![cfg(target_arch = "wasm32")]

use std::collections::BTreeMap;
use std::rc::Rc;

use honeycomb_yew::*;
use wasm_bindgen_test::*;
use web_sys::{Document, Element, KeyboardEventInit, PointerEventInit};
use yew::prelude::*;

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> Document {
    web_sys::window().expect("a window").document().expect("a document")
}

fn tid(s: &str) -> TileId {
    TileId(Slug::parse(s).unwrap())
}

/// Two own tiles, ana and bea, joined by a link named corridor — the minimal
/// board on which removing ana is refused with `Rejection::StillLinked`, the
/// one rejection the old wildcard arm in `unknown()` had no prose for.
fn linked_pair() -> Diagram {
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for name in ["ana", "bea"] {
        tiles.insert(
            tid(name),
            OwnTile {
                group: None,
                label: name.to_string(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
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

/// A zero-length `setTimeout`, which is reliably after every microtask the
/// scheduler queued for a render — including the ones a state update inside
/// an event handler queues.
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

#[wasm_bindgen_test]
async fn refusing_to_remove_a_linked_tile_speaks_prose_to_the_live_region_not_a_debug_dump() {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();

    let props = HoneycombProps {
        diagram: Rc::new(linked_pair()),
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
        removable: true,
        readonly: false,
        class: Classes::new(),
        aria_label: AttrValue::from("test board"),
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

    // pointerdown then pointerup with no pointermove between them is a
    // SELECTION, not a drag — see `onpointerup`'s "under the slop" branch in
    // src/lib.rs.
    ana.dispatch_event(&pointer_event("pointerdown")).unwrap();
    settle().await;
    ana.dispatch_event(&pointer_event("pointerup")).unwrap();
    settle().await;

    // Delete, on the board — where `onkeydown` lives — with ana selected and
    // `removable: true`. `check()` refuses this with `Rejection::StillLinked`
    // because `corridor` still connects it to bea; `removal()` turns that
    // refusal into `unknown()`'s prose and the component writes it straight
    // into the aria-live `<text>`.
    board.dispatch_event(&keydown("Delete")).unwrap();
    settle().await;
    settle().await;

    let live = container
        .query_selector("text.hc-live")
        .unwrap()
        .expect("the aria-live region rendered");
    let text = live.text_content().unwrap_or_default();

    assert!(
        !text.contains('{'),
        "the aria-live region leaked a Rust Debug dump instead of prose: {text:?}"
    );
    assert!(
        text.contains("ana"),
        "a screen-reader user needs to be told WHICH tile the refusal is about: {text:?}"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
