//! A LABEL IN A LANGUAGE STILL DRAWS THE WORDS.
//!
//! `honeycomb-core` now carries a language tag through the model, so a label is
//! a `Text` — value plus optional tag — where it used to be a `String`. That is
//! a change every renderer is downstream of, and the failure it could cause is
//! not subtle: a heading reading `Mairie@fr` on screen, or an accessible name
//! reading `Text { value: "Mairie", lang: Some("fr") }`, because somebody
//! reached for the whole value where only the words belong.
//!
//! TWO RENDERERS ARE PINNED HERE AND ONLY ONE OF THEM IS A HOST. The ground
//! markup is the HOST's (mirrored from `demo/src/lib.rs`, the same way
//! `notes.rs` beside this file mirrors it, because this crate does not and
//! should not depend on `demo`). The `aria-label` on the group's press target
//! is THIS CRATE'S OWN — `group_views` hands the whole `Group` over and the
//! component builds the accessible name itself, so a tagged label reaching a
//! screen reader as anything but its words is this crate's bug, not a host's.
//!
//! THIS FILE HAS NO RED STATE AND SAYS SO, for `notes.rs`'s reason and one
//! more of its own: before the change there was no way to give a label a tag
//! at all — `Group::label` was a `String` — so the test could not have been
//! written against the old API, let alone watched to fail. What it is worth is
//! permanence: it is what makes a later `Display for Text` that decided to
//! include the tag, or a host reaching for `format!("{:?}", …)`, fail here
//! instead of in a browser nobody is looking at.

#![cfg(target_arch = "wasm32")]

use std::collections::BTreeMap;
use std::rc::Rc;

use honeycomb_yew::*;
use wasm_bindgen_test::*;
use web_sys::{Document, Element};
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

async fn settle() {
    gloo_timers::future::TimeoutFuture::new(0).await;
}

fn fr(s: &str) -> Text {
    Text::tagged(s, "fr").expect("`fr` is a language tag")
}

/// The same two-tile group `notes.rs` uses, written in French: the diagram's
/// label, the group's and both tiles' all carry `@fr`.
fn one_french_group() -> Diagram {
    let gid = GroupId(Slug::parse("centre").unwrap());
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for (name, label) in [("mairie", "Mairie"), ("bibliotheque", "Bibliothèque")] {
        tiles.insert(
            tid(name),
            OwnTile {
                group: Some(gid.clone()),
                label: fr(label),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
    }
    let mut cells = BTreeMap::new();
    cells.insert(tid("mairie"), Cell { col: 0, row: 0 });
    cells.insert(tid("bibliotheque"), Cell { col: 1, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        gid,
        Group {
            label: fr("Centre administratif"),
            style_key: None,
            note: None,
            extra: Vec::new(),
        },
    );
    Diagram::try_new(DiagramSpec {
        slug: Slug::parse("plan").unwrap(),
        label: fr("Plan de la mairie"),
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
    .expect("a two-tile group with French labels is a legal standalone diagram")
}

/// `demo/src/lib.rs`'s own ground callback, narrowed to the one line this file
/// is about: the heading, drawn from `g.group.label`.
fn ground() -> Callback<GroupView, Html> {
    Callback::from(|g: GroupView| {
        html! {
            <g class="hc-ground" data-group={g.id.0.as_str().to_string()}>
                { for g.paths.iter().map(|d| html! { <path d={d.clone()} /> }) }
                <text class="hc-ground__label" x={g.heading.0.to_string()} y={g.heading.1.to_string()}>
                    { g.group.label.as_str().to_string() }
                </text>
            </g>
        }
    })
}

/// The host's own tile markup, likewise narrowed: the label a hexagon shows.
fn tile() -> Callback<TileView, Html> {
    Callback::from(|v: TileView| {
        html! {
            <text class="hc-tile__label" data-tile={v.id.0.as_str().to_string()}>
                { v.id.0.as_str().to_string() }
            </text>
        }
    })
}

fn mount(diagram: Diagram) -> (Element, yew::AppHandle<Honeycomb>) {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();
    let props = HoneycombProps {
        diagram: Rc::new(diagram),
        lattice: Lattice::new(46.0, 1.045),
        pad: 26.0,
        frame_ring: 1,
        tile: tile(),
        ground: Some(ground()),
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
    };
    let handle = yew::Renderer::<Honeycomb>::with_root_and_props(container.clone(), props).render();
    (container, handle)
}

/// THE WORDS AND NOT THE TAG, in the live DOM, from both renderers at once.
#[wasm_bindgen_test]
async fn a_tagged_label_renders_its_text_in_the_ground_and_in_the_accessible_name() {
    let (container, handle) = mount(one_french_group());
    settle().await;
    settle().await;

    let heading = container
        .query_selector("text.hc-ground__label")
        .unwrap()
        .expect("the host's ground callback drew no heading at all");
    assert_eq!(
        heading.text_content().unwrap_or_default(),
        "Centre administratif",
        "a group whose label is written in French drew something other than its own words"
    );

    let region = container
        .query_selector("g.hc-group")
        .unwrap()
        .expect("the component draws a press target for every group");
    let name = region
        .get_attribute("aria-label")
        .expect("the group's press target must have an accessible name");
    assert!(
        name.starts_with("Centre administratif, 2 tiles"),
        "this crate's OWN accessible name for a group did not open with the label's words; a \
         screen reader reads it verbatim, so anything but the text here is read aloud: {name}"
    );
    assert!(
        !name.contains("@fr") && !name.contains("lang"),
        "the language tag leaked into the accessible name: {name}"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
