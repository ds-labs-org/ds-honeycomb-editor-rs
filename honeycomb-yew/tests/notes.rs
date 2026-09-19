//! A group's note, drawn — the CONTRACT this pins, not a defect it fixes.
//!
//! Decision 3 of the audit this file belongs to found the note write-only:
//! `honeycomb-core`'s writer already persists `hive:note` on a group (see
//! `honeycomb-core/tests/modes.rs`'s `a_groups_note_survives_write_read_write`,
//! added beside this file), and `GroupView` already hands the whole `Group` —
//! note included — to every host's own `ground` callback. The place the note
//! actually vanished was TWO HOSTS' OWN MARKUP: both `demo/src/lib.rs` (this
//! repository) and the portal's `editor-wasm` painted `g.group.label` and
//! nothing else. Neither host is this crate.
//!
//! SO THIS FILE HAS NO RED STATE, and says so rather than inventing one. There
//! was nothing broken in `honeycomb-yew` to watch fail. What is worth a
//! permanent test is that the CONTRACT a host's fix depends on — the note
//! reaches the callback, and drawing a second line under the heading does not
//! move the heading `GroupView::heading` itself hands out — stays true. A
//! future change to `group_views()` that started computing `heading`
//! differently when a group carries a note (say, to make room for it) would
//! silently misplace every heading the portal's own `headings()` mirrors
//! (see that function's doc, one repository over) without a single test here
//! noticing. This one would.

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

/// One group, "civic", with two members and a note — the shape a host draws a
/// region and a heading for. The note text is the exact wording the audit's
/// own example uses, so a failure here reads the same way the finding did.
fn one_noted_group() -> Diagram {
    let gid = GroupId(Slug::parse("civic").unwrap());
    let mut tiles: BTreeMap<TileId, OwnTile> = BTreeMap::new();
    for name in ["hall", "library"] {
        tiles.insert(
            tid(name),
            OwnTile {
                group: Some(gid.clone()),
                label: name.to_string(),
                comment: None,
                style_key: None,
                extra: Vec::new(),
            },
        );
    }
    let mut cells = BTreeMap::new();
    cells.insert(tid("hall"), Cell { col: 0, row: 0 });
    cells.insert(tid("library"), Cell { col: 1, row: 0 });
    let mut groups = BTreeMap::new();
    groups.insert(
        gid,
        Group {
            label: "Civic Quarter".to_string(),
            style_key: None,
            note: Some("deployed once per participant — ×7 here".to_string()),
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
    .expect("a two-tile group with a note is a legal standalone diagram")
}

/// THE GROUND CALLBACK EVERY HOST NOW WRITES — mirrored here rather than
/// exercised through `demo`, which this crate does not and should not depend
/// on: `GroupView` is handed to a `Callback<GroupView, Html>` exactly this
/// shape, and this is the minimal one that draws both lines. Position matches
/// `demo/src/lib.rs`'s own: the label at `g.heading` unmoved, the note one
/// line under it. Neither number is invented — `demo`'s ground callback uses
/// the same offset, chosen so the note reads as belonging to the heading
/// above it without the two colliding.
const NOTE_DY: f64 = 15.0;

fn ground_with_note() -> Callback<GroupView, Html> {
    Callback::from(|g: GroupView| {
        html! {
            <g class="hc-ground" data-group={g.id.0.as_str().to_string()}>
                { for g.paths.iter().map(|d| html! { <path d={d.clone()} /> }) }
                <text class="hc-ground__label" x={g.heading.0.to_string()} y={g.heading.1.to_string()}>
                    { g.group.label.clone() }
                </text>
                { g.group.note.as_ref().map(|n| html! {
                    <text class="hc-ground__note" x={g.heading.0.to_string()}
                          y={(g.heading.1 + NOTE_DY).to_string()}>
                        { n.clone() }
                    </text>
                }).unwrap_or_default() }
            </g>
        }
    })
}

fn mount(
    diagram: Diagram,
    ground: Callback<GroupView, Html>,
) -> (Element, yew::AppHandle<Honeycomb>) {
    let container: Element = document().create_element("div").unwrap();
    document().body().unwrap().append_child(&container).unwrap();
    let props = HoneycombProps {
        diagram: Rc::new(diagram),
        lattice: Lattice::new(46.0, 1.045),
        pad: 26.0,
        frame_ring: 1,
        tile: Callback::from(|_: TileView| html! {}),
        ground: Some(ground),
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

/// THE CONTRACT: a `ground` callback that reads `g.group.note` sees it, in
/// the live DOM, once the component actually mounts — the exact moment the
/// finding says it used to vanish.
#[wasm_bindgen_test]
async fn a_groups_note_reaches_the_ground_callback_and_renders() {
    let (container, handle) = mount(one_noted_group(), ground_with_note());
    settle().await;
    settle().await;

    let note = container
        .query_selector("text.hc-ground__note")
        .unwrap()
        .expect("a group carrying a note must give the ground callback something to draw");
    assert_eq!(
        note.text_content().unwrap_or_default(),
        "deployed once per participant — ×7 here",
        "the note reached the DOM with the wrong text, or truncated"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// THE GEOMETRY GUARD: drawing a second line under the heading must not be
/// what MOVES the heading. `GroupView::heading` is a single point the portal's
/// own `headings()` recomputes independently and must agree with (see that
/// function's doc on `ds-honeycomb-editor`'s side of this decision) — so the
/// heading `<text>` has to land at the SAME point whether or not anything is
/// drawn beneath it, and the note has to land strictly below it, never on top
/// of it or above it.
#[wasm_bindgen_test]
async fn the_note_sits_below_the_heading_and_never_moves_it() {
    let (container, handle) = mount(one_noted_group(), ground_with_note());
    settle().await;
    settle().await;

    let label = container
        .query_selector("text.hc-ground__label")
        .unwrap()
        .expect("the heading must still render with a note present");
    let note = container
        .query_selector("text.hc-ground__note")
        .unwrap()
        .expect("the note must still render");

    let attr = |el: &Element, name: &str| -> f64 {
        el.get_attribute(name)
            .unwrap_or_else(|| panic!("{name} missing"))
            .parse()
            .unwrap_or_else(|_| panic!("{name} is not a number"))
    };
    let (label_x, label_y) = (attr(&label, "x"), attr(&label, "y"));
    let (note_x, note_y) = (attr(&note, "x"), attr(&note, "y"));

    assert_eq!(
        label_x, note_x,
        "the note is not horizontally aligned with the heading it belongs under"
    );
    assert!(
        (note_y - label_y - NOTE_DY).abs() < 0.001,
        "the note did not land exactly one offset below the heading (label y={label_y}, note \
         y={note_y}); either the heading moved to make room for it or the note drifted from it"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}

/// A GROUP WITH NO NOTE DRAWS NO NOTE ELEMENT AT ALL — not an empty one. The
/// same `sh:maxCount 1`/no `sh:minCount` shape that makes the note optional in
/// the file makes it optional on the board: a host that always rendered the
/// `<text class="hc-ground__note">` node, empty when there is nothing to say,
/// would leave a zero-height hit target and an extra DOM node behind on every
/// unnoted group, forever.
#[wasm_bindgen_test]
async fn a_group_with_no_note_draws_no_note_element() {
    let mut d = one_noted_group();
    // Overwrite in place via the same command the drawer form uses, rather
    // than building a second fixture: `EditGroup` is what a host actually
    // calls when a user clears a note, so this exercises the real path.
    let gid = GroupId(Slug::parse("civic").unwrap());
    let was = d.group(&gid).unwrap().clone();
    d.apply(Command::EditGroup {
        id: gid,
        group: Group { note: None, ..was },
    })
    .expect("clearing a note is always legal");

    let (container, handle) = mount(d, ground_with_note());
    settle().await;
    settle().await;

    assert!(
        container
            .query_selector("text.hc-ground__note")
            .unwrap()
            .is_none(),
        "an unnoted group rendered a note element anyway"
    );

    handle.destroy();
    document().body().unwrap().remove_child(&container).unwrap();
}
