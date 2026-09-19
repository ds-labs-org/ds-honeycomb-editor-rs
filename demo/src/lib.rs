//! The demo page, as ONE component rendered twice: once on the host by
//! `demo-ssg` at build time, once in the browser by `src/main.rs` at boot.
//!
//! Both go through `DemoApp` and `data::town_plan()` from this same lib, which
//! is the only reason the two renders agree.
//!
//! THIS FILE IS THE HOST, and the seam is visible in it. Everything drawn inside
//! a hexagon is written here; the component supplies where it goes and refuses
//! the drops that would overlap. `hive:styleKey` arrives as an opaque IRI and
//! `slot()` below is the only thing in this repository that decides one of them
//! means a colour — which is precisely what keeps a palette out of a diagram
//! format.

pub mod data;

use std::rc::Rc;

use honeycomb_yew::{
    Change, Diagram, FrameView, GROW, GroupView, History, Honeycomb, Iri, Lattice, Status,
    StatusKind, TileState, TileView,
};
use yew::prelude::*;

pub const REPO: &str = "https://github.com/ds-labs-org/ds-honeycomb-editor-rs";

/// Presentation, and never serialised: two hosts may draw the same document at
/// different sizes and it is the same diagram.
const BOARD: Lattice = Lattice::new(46.0, 1.045);

#[derive(Properties, PartialEq, Default)]
pub struct AppProps {}

#[function_component(DemoApp)]
pub fn demo_app(_props: &AppProps) -> Html {
    let diagram = use_state(|| Rc::new(data::town_plan()));
    let status = use_state(rest);
    // A `History`, not a stack of document snapshots: `apply` hands back the
    // inverse of what it did, so undo cannot be a second implementation of the
    // move that is free to disagree with the first.
    let history = use_mut_ref(History::default);

    let on_change = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        Callback::from(move |c: Change| {
            // WHAT THE MOVE DID TO THE GROUP, not just that a move happened.
            // Now that a plain tile drag detaches, pulling a member out of a
            // district is the ordinary gesture rather than an expert one — so
            // "your district is now in three parts" is ordinary news, and a
            // status line that only ever says "Moved." leaves the reader to
            // notice it from the dashes.
            let split = moved_tile(&c.applied)
                .and_then(|id| c.diagram.group_of(&id).cloned())
                .map(|g| (c.diagram.group(&g).map(|x| x.label.clone()).unwrap_or_default(),
                          c.diagram.group_components(&g).len()))
                .filter(|(_, n)| *n > 1);
            history.borrow_mut().record(c.inverse);
            diagram.set(c.diagram);
            status.set(Status {
                text: match split {
                    Some((label, n)) => {
                        format!("Moved. {label} is now in {n} parts. The Turtle is up to date.")
                    }
                    None => "Moved. The Turtle on the right is up to date.".into(),
                },
                kind: StatusKind::Info,
            });
        })
    };
    let on_status = {
        let status = status.clone();
        Callback::from(move |s: Status| status.set(s))
    };
    let on_undo = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = (**diagram).clone();
            match history.borrow_mut().undo(&mut next) {
                Some(_) => {
                    diagram.set(Rc::new(next));
                    status.set(Status {
                        text: "Undone.".into(),
                        kind: StatusKind::Info,
                    });
                }
                None => status.set(Status {
                    text: "Nothing to undo.".into(),
                    kind: StatusKind::Info,
                }),
            }
        })
    };
    let on_redo = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        Callback::from(move |_: MouseEvent| {
            let mut next = (**diagram).clone();
            match history.borrow_mut().redo(&mut next) {
                Some(_) => {
                    diagram.set(Rc::new(next));
                    status.set(Status {
                        text: "Redone.".into(),
                        kind: StatusKind::Info,
                    });
                }
                None => status.set(Status {
                    text: "Nothing to redo.".into(),
                    kind: StatusKind::Info,
                }),
            }
        })
    };
    let on_reset = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        Callback::from(move |_: MouseEvent| {
            *history.borrow_mut() = History::default();
            diagram.set(Rc::new(data::town_plan()));
            status.set(rest());
        })
    };

    // use_callback, not a fresh closure: `Callback`'s PartialEq is `Rc::ptr_eq`,
    // so a closure rebuilt every render would make the component's props
    // unequal every render for no reason at all.
    let tile = use_callback((*diagram).clone(), |v: TileView, d: &Rc<Diagram>| {
        let (label, key) = content(d, &v);
        html! {
            <g data-slot={slot(key.as_ref()).to_string()}>
                <polygon class="hc-tile__hex" points={hexagon(v.r - 4.0)} />
                <text class="hc-tile__label" x="0" y="-2">{ label }</text>
                <text class="hc-tile__group" x="0" y="16">
                    { v.group.as_ref().and_then(|g| d.group(g)).map(|g| g.label.clone()).unwrap_or_default() }
                </text>
                { if matches!(v.state, TileState::Blocking) {
                    html! { <polygon class="hc-tile__bar" points={hexagon(v.r - 4.0)} /> }
                } else { Html::default() } }
            </g>
        }
    });

    let ground = use_callback((), |g: GroupView, _| {
        html! {
            <g class={classes!("hc-ground", g.fractured().then_some("is-fractured"),
                               g.touching.then_some("is-touching"),
                               g.hovered.then_some("is-hovered"),
                               g.grabbed.then_some("is-held"))}
               data-slot={slot(g.group.style_key.as_ref()).to_string()}>
                { for g.paths.iter().map(|d| html! { <path class="hc-ground__edge" d={d.clone()} /> }) }
                { for g.paths.iter().map(|d| html! { <path class="hc-ground__fill" d={d.clone()} /> }) }
                // `heading`, NOT `anchor`. The anchor is the members' centroid,
                // which for a two-row district puts the label on top of its own
                // hexagons — and the label is the handle you grab to move the
                // district, so a press aimed at it reached the tile underneath
                // and pulled one building out of the block instead.
                <text class="hc-ground__label" x={fmt(g.heading.0)} y={fmt(g.heading.1)}>
                    // THE COUNT, NOT THE WORD "2". This said "(2 pieces)"
                    // unconditionally, which was wrong the moment a group was in
                    // three — and now that a plain tile drag detaches, three is
                    // two gestures away rather than a curiosity.
                    { if g.fractured() { format!("{} · {} parts", g.group.label, g.pieces) }
                      else { g.group.label.clone() } }
                </text>
            </g>
        }
    });

    let frame = use_callback((), |f: FrameView, _| {
        html! {
            <g class="hc-frame">
                // <defs> HERE and not in the tile callback: the tile callback
                // runs once per hexagon, and twelve <pattern id="..."> with one
                // id between them is a document where which one wins is a
                // question about statement order.
                <defs>
                    <pattern id="hc-hatch-blocked" width="8" height="8"
                             patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                        // The wash travels inside the pattern so one polygon can
                        // carry both it and the hatch; a shape cannot have two
                        // fills.
                        <rect class="hc-hatch-bg" width="8" height="8" />
                        <line x1="0" y1="0" x2="0" y2="8" class="hc-hatch-line" />
                    </pattern>
                </defs>
                { for f.cells.iter().map(|c| {
                    let (x, y) = f.frame.at(*c, BOARD);
                    html! { <path class="hc-frame__cell" d={BOARD.hex_path(x, y, BOARD.r)} /> }
                }) }
            </g>
        }
    });

    let ttl = data::turtle(&diagram);
    let download = use_state(|| AttrValue::from("honeycomb-demo.ttl"));
    {
        // Revoking the previous URL matters: without it every drag leaks a Blob
        // for the lifetime of the document.
        let download = download.clone();
        let ttl = ttl.clone();
        use_effect_with(ttl, move |ttl| {
            let url = blob_url(ttl);
            if let Some(u) = url.clone() {
                download.set(AttrValue::from(u));
            }
            move || {
                if let Some(u) = url {
                    let _ = web_sys::Url::revoke_object_url(&u);
                }
            }
        });
    }
    let triples = ttl
        .lines()
        .filter(|l| l.trim_end().ends_with([';', '.']) && !l.starts_with('@'))
        .count();

    html! {
        <>
        <header class="hc-head">
            <div>
                <h1>{ "Honeycomb Editor" }<span class="hc-head__tag">{ "demo" }</span></h1>
                <p class="hc-head__sub">
                    { "A hex-lattice diagram you can rearrange. A tile drags alone; a \
                       district's ground or heading drags the whole district. Every name on \
                       this page is invented." }
                </p>
            </div>
            <a class="hc-head__repo" href={REPO}>{ "Source ↗" }</a>
        </header>

        // Rendered by the generator, so it is in the delivered HTML and a
        // visitor can read what the page is for before any wasm exists.
        <ol class="hc-try">
            <li><b>{ "Drag Library" }</b>{ " to an empty cell — it goes alone, and the Civic \
                                           Quarter is now in two parts." }</li>
            <li><b>{ "Drag the Civic Quarter's heading" }</b>{ " — all four move together." }</li>
            <li><b>{ "Drop Bakery onto Grocer" }</b>{ " — same group, so they trade places." }</li>
            <li><b>{ "Drop Museum onto Cinema" }</b>{ " — neither is in a group, so it refuses \
                                                      and says why." }</li>
            <li><b>{ "Watch the Turtle" }</b>{ " change as you go." }</li>
        </ol>

        <noscript>
            <p class="hc-noscript">
                { "JavaScript is off, so this diagram is a picture. The tiles won't move. \
                   The Turtle below is the real serialisation of exactly what you see." }
            </p>
        </noscript>

        <main class="hc-main">
            <section class="hc-board-pane">
                <Honeycomb
                    diagram={(*diagram).clone()}
                    lattice={BOARD}
                    {tile}
                    ground={Some(ground)}
                    frame={Some(frame)}
                    {on_change}
                    {on_status}
                    aria_label={format!("{}, {} tiles", diagram.label(), diagram.cells().count())}
                />
                <p class={classes!("hc-status", css(status.kind))} role="status">{ status.text.clone() }</p>
            </section>

            <aside class="hc-ttl-pane">
                <div class="hc-ttl-head">
                    <h2>{ "honeycomb-demo.ttl" }</h2>
                    <span class="hc-ttl-count">{ format!("{triples} statements") }</span>
                </div>
                <div class="hc-ttl-actions">
                    // A REAL LINK TO A REAL FILE, upgraded rather than replaced.
                    // The generated href is what a visitor with no JavaScript
                    // gets and it is correct for them: nothing has moved. Once
                    // the wasm is live the href becomes a Blob of the CURRENT
                    // arrangement — which is what `demo-ssg` has claimed in a
                    // comment since the day it was written, and what the file
                    // actually did was hand out the generated document after
                    // every drag.
                    <a class="hc-btn" href={(*download).clone()}
                       download="honeycomb-demo.ttl">{ "Download .ttl" }</a>
                    <button class="hc-btn" onclick={on_undo}>{ "Undo" }</button>
                    <button class="hc-btn" onclick={on_redo}>{ "Redo" }</button>
                    <button class="hc-btn" onclick={on_reset}>{ "Reset" }</button>
                </div>
                <pre class="hc-ttl"><code>{ ttl }</code></pre>
            </aside>
        </main>

        <details class="hc-listing">
            <summary>{ "Diagram contents as a list" }</summary>
            <table>
                <thead><tr><th>{ "Tile" }</th><th>{ "Group" }</th><th>{ "Column" }</th><th>{ "Row" }</th></tr></thead>
                <tbody>
                { for diagram.cells().map(|(cell, id)| {
                    let v = TileView { id: id.clone(), cell, r: BOARD.r,
                                       group: diagram.group_of(id).cloned(),
                                       state: TileState::Resting, focused: false };
                    let (label, _) = content(&diagram, &v);
                    html! {
                        <tr>
                            <td>{ label }</td>
                            <td>{ diagram.group_of(id).and_then(|g| diagram.group(g))
                                    .map(|g| g.label.clone()).unwrap_or_else(|| "—".into()) }</td>
                            <td>{ cell.col }</td>
                            <td>{ cell.row }</td>
                        </tr>
                    }
                }) }
                </tbody>
            </table>
        </details>

        <footer class="hc-foot">
            <p>
                { "Vocabulary: " }<code>{ honeycomb_yew::NS }</code>
                { " · Apache-2.0 · " }
                <a href={REPO}>{ "ds-labs-org/ds-honeycomb-editor-rs" }</a>
            </p>
        </footer>
        </>
    }
}

/// The tile a command moved, for the one question the status line asks of it.
/// `Swap` names two; the one the user grabbed is `a`.
fn moved_tile(cmd: &honeycomb_yew::Command) -> Option<honeycomb_yew::TileId> {
    match cmd {
        honeycomb_yew::Command::Translate { grabbed, .. } => Some(grabbed.clone()),
        honeycomb_yew::Command::Swap { a, .. } => Some(a.clone()),
        _ => None,
    }
}

/// A `blob:` URL for the current serialisation, or None on a host that has no
/// `URL.createObjectURL` — in which case the generated href stays, which is a
/// stale file rather than a broken button.
fn blob_url(ttl: &str) -> Option<String> {
    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(ttl));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("text/turtle");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).ok()?;
    web_sys::Url::create_object_url_with_blob(&blob).ok()
}

/// What the generated HTML says, and therefore what a visitor with no
/// JavaScript reads. It has to be true in that state.
fn rest() -> Status {
    Status {
        text: "Drag a tile and it moves alone. Drag a district's heading and the whole \
               district moves with it."
            .into(),
        kind: StatusKind::Info,
    }
}

/// The host's own lookup. The component handed over an id and a cell and knows
/// nothing about either of these.
fn content(d: &Diagram, v: &TileView) -> (String, Option<Iri>) {
    match d.content() {
        honeycomb_yew::Content::Standalone { tiles } => match tiles.get(&v.id) {
            Some(t) => (t.label.clone(), t.style_key.clone()),
            None => (v.id.0.as_str().to_string(), None),
        },
        // A pinned diagram has no labels HERE, by construction: they live at the
        // subject `hive:represents` names, and a host that can resolve it draws
        // from there. Falling back to the slug is the honest thing to show, and
        // it is why the demo is standalone.
        honeycomb_yew::Content::Pinned { .. } => (v.id.0.as_str().to_string(), None),
    }
}

/// THE ONLY PLACE A styleKey BECOMES AN APPEARANCE. The vocabulary does not know
/// what one means, the component does not dereference one, and this function is
/// the seam that keeps it that way: an unknown key draws as the default rather
/// than failing, because a host meeting a key from a newer release must still
/// be able to draw the diagram.
fn slot(key: Option<&Iri>) -> &'static str {
    match key.map(|Iri(k)| k.rsplit('/').next().unwrap_or("")) {
        Some("civic") => "1",
        Some("green") => "2",
        Some("market") => "3",
        _ => "0",
    }
}

fn css(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Refused => "hc-status--refused",
        StatusKind::Warning => "hc-status--warning",
        StatusKind::Info => "hc-status--rest",
    }
}

/// A pointy-top hexagon CENTRED ON (0,0): the component decides where it lands,
/// so the markup must not know.
fn hexagon(r: f64) -> String {
    let dx = r * 0.866_025_403_784_438_6;
    let h = r / 2.0;
    format!(
        "{},{} {},{} {},{} {},{} {},{} {},{}",
        fmt(0.0),
        fmt(-r),
        fmt(dx),
        fmt(-h),
        fmt(dx),
        fmt(h),
        fmt(0.0),
        fmt(r),
        fmt(-dx),
        fmt(h),
        fmt(-dx),
        fmt(-h),
    )
}

fn fmt(v: f64) -> String {
    format!("{v:.2}")
}
