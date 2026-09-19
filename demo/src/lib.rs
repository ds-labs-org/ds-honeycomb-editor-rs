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
    Change, Command, Diagram, FrameView, Group, GroupId, GroupView, History, Honeycomb, Iri,
    Lattice, Link, LinkId, LinkState, LinkView, NewTile, Pending, PendingEnd, Routing, Slug,
    Status, StatusKind, TileId, TileState, TileView,
};
use wasm_bindgen::JsCast as _;
use yew::prelude::*;

pub const REPO: &str = "https://github.com/ds-labs-org/ds-honeycomb-editor-rs";

/// Presentation, and never serialised: two hosts may draw the same document at
/// different sizes and it is the same diagram.
const BOARD: Lattice = Lattice::new(46.0, 1.045);

/// `<Honeycomb>`'s own defaults, named here rather than left implicit, because
/// `natural_size` below has to be called with the SAME `pad` and `frame_ring`
/// the component itself renders with or the two numbers answer different
/// questions. `<Honeycomb>`'s invocation passes both explicitly now, for the
/// same reason `frames` in `honeycomb-yew` is one function and not two: one
/// written-down value that both sides read, rather than a default on one side
/// and a literal on the other that happens to match today.
const BOARD_PAD: f64 = 26.0;
const BOARD_RING: i32 = 1;

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
    // EVERYTHING THAT COULD BE ON THE PLAN, and the drawer shows whichever of
    // them is not. Deriving the list from the diagram rather than tracking
    // adds and removes is what makes undo work for free: the first version
    // added and removed chips as commands arrived, and `on_undo` does not go
    // through `on_change`, so undoing a placement left the building on neither
    // the plan nor the bench.
    let roster = use_state(data::bench);
    let armed = use_state(|| Option::<Pending>::None);
    let linking = use_state(|| false);
    // Links are named as they are drawn. A counter rather than a clock or a
    // random number, for `town_plan`'s reason: the build-time render and the
    // browser's first render have to produce the same bytes.
    let next_link = use_mut_ref(|| 0u32);
    // Which building the board says is selected. The component has always
    // offered this and nothing on this page had ever asked for it — which was
    // fine while every edit was a drag, and is not once a form has to know what
    // it is editing.
    let selected = use_state(|| Option::<TileId>::None);

    let on_change = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        let roster = roster.clone();
        Callback::from(move |c: Change| {
            // WHAT THE MOVE DID TO THE GROUP, not just that a move happened.
            // Now that a plain tile drag detaches, pulling a member out of a
            // district is the ordinary gesture rather than an expert one — so
            // "your district is now in three parts" is ordinary news, and a
            // status line that only ever says "Moved." leaves the reader to
            // notice it from the dashes.
            let verb = match &c.applied {
                honeycomb_yew::Command::Add { .. } => "Placed.",
                honeycomb_yew::Command::Remove { .. } => "Taken off the plan.",
                _ => "Moved.",
            };
            let split = moved_tile(&c.applied)
                .and_then(|id| c.diagram.group_of(&id).cloned())
                .map(|g| {
                    (
                        c.diagram
                            .group(&g)
                            .map(|x| x.label.clone())
                            .unwrap_or_default(),
                        c.diagram.group_components(&g).len(),
                    )
                })
                .filter(|(_, n)| *n > 1);
            // A REMOVED BUILDING JOINS THE ROSTER, so it can be put back. The
            // diagram no longer holds its content — that is what a
            // self-contained inverse means — so the inverse is the only place it
            // still exists.
            if let Some((tile, t)) = returning(&c.applied, &c.inverse) {
                let mut all = (*roster).clone();
                if !all.iter().any(|(id, _)| *id == tile) {
                    all.push((tile, t));
                    all.sort_by(|a, b| a.1.label.cmp(&b.1.label));
                    roster.set(all);
                }
            }
            history.borrow_mut().record(c.inverse);
            diagram.set(c.diagram);
            status.set(Status {
                text: match split {
                    Some((label, n)) => {
                        format!("{verb} {label} is now in {n} parts. The Turtle is up to date.")
                    }
                    None => format!("{verb} The Turtle on the right is up to date."),
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
        let roster = roster.clone();
        let armed = armed.clone();
        Callback::from(move |_: MouseEvent| {
            *history.borrow_mut() = History::default();
            diagram.set(Rc::new(data::town_plan()));
            roster.set(data::bench());
            armed.set(None);
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

    // THE HOST DECIDES WHAT A LINE IS. The component reports two ends; the id,
    // the wording and the routing are this page's business — which is why the
    // three routings cycle here rather than being a fourth control: the demo's
    // job is to show that all three exist and are the same document.
    // APPLY, RECORD, SAY — in one place. Every edit on this page did these three
    // in its own five lines, and the fourth and fifth verbs would have made it
    // seven copies of a sequence whose ORDER matters: record before the state
    // moves, or an undo taken before the next render has no entry.
    let run: Rc<dyn Fn(Command, String)> = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        Rc::new(move |cmd: Command, said: String| {
            let mut next = (**diagram).clone();
            match next.apply(cmd) {
                Ok(inverse) => {
                    history.borrow_mut().record(inverse);
                    status.set(Status {
                        text: format!("{said} The Turtle is up to date."),
                        kind: StatusKind::Info,
                    });
                    diagram.set(Rc::new(next));
                }
                Err(r) => status.set(Status {
                    text: refusal(&r),
                    kind: StatusKind::Refused,
                }),
            }
        })
    };

    let on_link = {
        let diagram = diagram.clone();
        let status = status.clone();
        let history = history.clone();
        let next_link = next_link.clone();
        Callback::from(move |(from, to): (TileId, TileId)| {
            let n = {
                let mut c = next_link.borrow_mut();
                *c += 1;
                *c
            };
            let routing = match n % 3 {
                1 => Routing::Straight,
                2 => Routing::Arc,
                _ => Routing::LatticePath,
            };
            let id = LinkId(Slug::parse(&format!("line-{n}")).expect("a counted slug"));
            let mut next = (**diagram).clone();
            let cmd = Command::Connect {
                id,
                link: Link {
                    from: from.clone(),
                    to: to.clone(),
                    label: None,
                    routing,
                    style_key: None,
                    extra: Vec::new(),
                },
            };
            match next.apply(cmd) {
                Ok(inverse) => {
                    history.borrow_mut().record(inverse);
                    status.set(Status {
                        text: format!(
                            "Linked {} to {}, drawn {}. The Turtle is up to date.",
                            from.0.as_str(),
                            to.0.as_str(),
                            routing.term()
                        ),
                        kind: StatusKind::Info,
                    });
                    diagram.set(Rc::new(next));
                }
                Err(r) => status.set(Status {
                    text: refusal(&r),
                    kind: StatusKind::Refused,
                }),
            }
        })
    };

    let link = use_callback((), |v: LinkView, _| {
        let faded = matches!(v.state, LinkState::Moving);
        html! {
            <g class="hc-link" opacity={if faded { "0.45" } else { "1" }}>
                <path class="hc-link__line" d={v.path.clone()} fill="none"
                      marker-end="url(#hc-arrow)" />
                { v.link.label.as_ref().map(|t| html! {
                    <text class="hc-link__label"
                          x={fmt((v.from.0 + v.to.0) / 2.0)}
                          y={fmt((v.from.1 + v.to.1) / 2.0 - 6.0)}>{ t.clone() }</text>
                }).unwrap_or_default() }
            </g>
        }
    });

    let frame = use_callback((), |f: FrameView, _| {
        html! {
            // `is-armed` when a palette chip is waiting for a cell: without it
            // the board looks identical whether or not something is armed, and
            // the only feedback is a sentence in the status line.
            <g class={classes!("hc-frame", f.armed.then_some("is-armed"))}>
                // <defs> HERE and not in the tile callback: the tile callback
                // runs once per hexagon, and twelve <pattern id="..."> with one
                // id between them is a document where which one wins is a
                // question about statement order.
                <defs>
                    // ONE arrowhead for every line on the board. `markerUnits`
                    // is strokeWidth by default, so this scales with the line
                    // rather than staying a fixed size on a thicker one.
                    <marker id="hc-arrow" viewBox="0 0 10 10" refX="9" refY="5"
                            markerWidth="5" markerHeight="5" orient="auto-start-reverse">
                        <path d="M 0 1 L 10 5 L 0 9 z" class="hc-arrow" />
                    </marker>
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

    // DERIVED ON EVERY RENDER, never stored: whatever is on the roster and not
    // on the plan. There is no palette state here that an undo could get wrong.
    let bench: Vec<(TileId, honeycomb_yew::OwnTile)> = roster
        .iter()
        .filter(|(id, _)| diagram.cell_of(id).is_none())
        .cloned()
        .collect();

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
                       district's ground or heading drags the whole district; and the \
                       palette holds the buildings that are not on the plan. Every name on \
                       this page is invented." }
                </p>
            </div>
            <a class="hc-head__repo" href={REPO}>{ "Source ↗" }</a>
        </header>

        // Rendered by the generator, so it is in the delivered HTML and a
        // visitor can read what the page is for before any wasm exists.
        <ol class="hc-try">
            // "AWAY FROM THE OTHERS" IS LOAD-BEARING. Library goes alone
            // wherever it lands, but the Quarter only SPLITS if the cell it
            // lands on is not adjacent to a remaining member — and four of the
            // six nearest empty cells are. An instruction that promises a split
            // and delivers a solid block teaches the reader that the dashes mean
            // nothing.
            <li><b>{ "Drag Library" }</b>{ " to an empty cell away from the others — it goes \
                                           alone, and the Civic Quarter is now in two parts." }</li>
            <li><b>{ "Drag the Civic Quarter's heading" }</b>{ " — all four move together." }</li>
            <li><b>{ "Drop Bakery onto Grocer" }</b>{ " — same group, so they trade places." }</li>
            <li><b>{ "Drop Museum onto Cinema" }</b>{ " — neither is in a group, so it refuses \
                                                      and says why." }</li>
            <li><b>{ "Click Archive in the palette" }</b>{ ", then click an empty cell — or \
                                                          drag it straight onto the plan." }</li>
            <li><b>{ "Select a building and press Delete" }</b>{ " — it goes back to the \
                                                                 palette, and Undo brings \
                                                                 it back." }</li>
            <li><b>{ "Press Link, then drag between two buildings" }</b>{ " — the three \
                    routings cycle: straight, bowed, and along the comb." }</li>
            <li><b>{ "Rename a district below" }</b>{ ", change its ground, or clear its name \
                    entirely \u{2014} a district may have none." }</li>
            <li><b>{ "Watch the Turtle" }</b>{ " change as you go." }</li>
        </ol>

        <noscript>
            <p class="hc-noscript">
                { "JavaScript is off, so this diagram is a picture. The tiles won't move. \
                   The Turtle below is the real serialisation of exactly what you see." }
            </p>
        </noscript>

        <details class="hc-palette" open={!bench.is_empty()}>
            <summary>
                { "Palette" }
                <span class="hc-palette__count">
                    { format!("{} not on the plan", bench.len()) }
                </span>
            </summary>
            // PLAIN BUTTONS AND A PLAIN LIST. With JavaScript off this renders
            // as the names of three buildings that are not on the plan, which is
            // true and readable; the buttons do nothing, which is why the
            // sentence beside them says so rather than leaving a reader pressing
            // them.
            <p class="hc-palette__note">
                { "Click one, then click a cell. Or drag it onto the plan. \
                   With JavaScript off this is a list." }
            </p>
            <ul class="hc-palette__list">
            { for bench.iter().map(|(id, t)| {
                let armed_now = armed.as_ref().is_some_and(|w| &w.id == id);
                let arm = {
                    let armed = armed.clone();
                    let entry = (id.clone(), t.clone());
                    Callback::from(move |_: MouseEvent| {
                        let (id, t) = entry.clone();
                        armed.set(match &*armed {
                            Some(w) if w.id == id => None,
                            _ => Some(Pending { id, what: NewTile::Own(Box::new(t)) }),
                        });
                    })
                };
                // NO POINTER CAPTURE HERE, and the component's props say why:
                // captured, the board receives no pointermove at all and the
                // drag is silently dead.
                let grab = {
                    let armed = armed.clone();
                    let entry = (id.clone(), t.clone());
                    Callback::from(move |_: PointerEvent| {
                        let (id, t) = entry.clone();
                        armed.set(Some(Pending { id, what: NewTile::Own(Box::new(t)) }));
                    })
                };
                html! {
                    <li>
                        <button type="button" class="hc-chip" aria-pressed={armed_now.to_string()}
                                onclick={arm} onpointerdown={grab}>
                            { t.label.clone() }
                        </button>
                    </li>
                }
            }) }
            </ul>
        </details>

        <main class="hc-main">
            <section class="hc-board-pane">
                // A COLUMN NARROWER THAN THE BOARD SCROLLS RATHER THAN SHRINKS
                // IT. `<Honeycomb>`'s own root is `width:100%`, on purpose — it
                // fits whatever column a host gives it — which on a phone-width
                // column means a real board of hexagons and group labels
                // scales down to a few hundred pixels of illegible ink, worse
                // than the no-JavaScript fallback, which is at least readable
                // by scrolling. `natural_size` is the component's own answer to
                // "how wide does this diagram actually want to be", computed
                // from the SAME pad and ring the `<Honeycomb>` below renders
                // with; giving that number to THIS wrapper as a `min-width`,
                // with `.hc-board-pane` carrying `overflow-x: auto` (see
                // `styles.css`), is what turns "shrinks past reading" into
                // "scrolls at a legible size" — the fix this crate can offer
                // without ever setting its own width, which stays the host's
                // to own.
                <div class="hc-board-scroll"
                     style={format!("min-width:{:.0}px", honeycomb_yew::natural_size(
                         &diagram, BOARD, BOARD_PAD, BOARD_RING).0)}>
                    <Honeycomb
                        diagram={(*diagram).clone()}
                        lattice={BOARD}
                        pad={BOARD_PAD}
                        frame_ring={BOARD_RING}
                        {tile}
                        ground={Some(ground)}
                        frame={Some(frame)}
                        {on_change}
                        {on_status}
                        {link}
                        {on_link}
                        linking={*linking}
                        on_select={ {
                            let selected = selected.clone();
                            Callback::from(move |id: Option<TileId>| selected.set(id))
                        } }
                        pending={(*armed).clone()}
                        on_pending={ {
                            let armed = armed.clone();
                            Callback::from(move |end: PendingEnd| {
                                // Cleared either way: the host owns this prop, and a
                                // chip that stayed pressed after its tile landed
                                // would arm a second copy on the next board click.
                                let _ = end;
                                armed.set(None);
                            })
                        } }
                        removable=true
                        aria_label={format!("{}, {} tiles", diagram.label(), diagram.cells().count())}
                    />
                </div>
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
                    // A TOGGLE, NOT A MODIFIER. This editor has no modifier
                    // keys — the one it used to use is claimed by the window
                    // manager on a common desktop — so a verb this different
                    // from moving something gets a visible mode instead.
                    <button class={classes!("hc-btn", linking.then_some("is-on"))}
                            aria-pressed={linking.to_string()}
                            onclick={ {
                                let linking = linking.clone();
                                Callback::from(move |_: MouseEvent| linking.set(!*linking))
                            } }>
                        { if *linking { "Linking" } else { "Link" } }
                    </button>
                </div>
                <pre class="hc-ttl"><code>{ ttl }</code></pre>
            </aside>
        </main>

        // EDITING THE DISTRICTS THEMSELVES, below the board the way the palette
        // sits above it. Everything here changes the DIAGRAM — a district's name,
        // its ground and its note are `hive:` terms this document owns, not
        // facts about anything outside it.
        //
        // ON CHANGE, NOT ON INPUT. `oninput` would apply a command per keystroke
        // and put eleven entries on the undo stack for the word "Bakery"; a
        // field commits when it loses focus or takes Enter.
        <details class="hc-groups" open=true>
            <summary>
                { "Districts" }
                <span class="hc-groups__count">{ format!("{}", diagram.groups().len()) }</span>
            </summary>

            <table class="hc-groups__table">
                <thead><tr>
                    <th>{ "Name" }</th><th>{ "Ground" }</th><th>{ "Note" }</th>
                    <th>{ "In it" }</th><th><span class="hc-sr">{ "Delete" }</span></th>
                </tr></thead>
                <tbody>
                { for diagram.groups().iter().map(|(id, g)| {
                    let members = diagram.members(id).len();
                    let edit = {
                        let run = run.clone();
                        let id = id.clone();
                        let g = g.clone();
                        move |what: &'static str, value: String| {
                            let mut next = g.clone();
                            match what {
                                "label" => next.label = value.clone(),
                                "note" => {
                                    next.note = (!value.trim().is_empty()).then_some(value.clone())
                                }
                                _ => {
                                    next.style_key = (!value.is_empty())
                                        .then(|| Iri(format!("{}{value}", data::STYLE)))
                                }
                            }
                            run(
                                Command::EditGroup { id: id.clone(), group: next },
                                format!("{} changed.", id.0.as_str()),
                            );
                        }
                    };
                    let on_label = {
                        let edit = edit.clone();
                        Callback::from(move |e: Event| {
                            if let Some(i) = e.target_dyn_into::<web_sys::HtmlInputElement>() {
                                edit("label", i.value());
                            }
                        })
                    };
                    let on_note = {
                        let edit = edit.clone();
                        Callback::from(move |e: Event| {
                            if let Some(i) = e.target_dyn_into::<web_sys::HtmlInputElement>() {
                                edit("note", i.value());
                            }
                        })
                    };
                    let on_style = {
                        let edit = edit.clone();
                        Callback::from(move |e: Event| {
                            if let Some(i) = e.target_dyn_into::<web_sys::HtmlSelectElement>() {
                                edit("style", i.value());
                            }
                        })
                    };
                    let on_delete = {
                        let run = run.clone();
                        let id = id.clone();
                        Callback::from(move |_: MouseEvent| {
                            run(
                                Command::RemoveGroup { id: id.clone() },
                                format!("{} is gone.", id.0.as_str()),
                            )
                        })
                    };
                    let current = g.style_key.as_ref().map(|Iri(k)| {
                        k.rsplit('/').next().unwrap_or("").to_string()
                    }).unwrap_or_default();
                    html! {
                        <tr>
                            <td>
                                // A DISTRICT MAY HAVE NO NAME. The ground is often
                                // the whole signal, and the placeholder says so
                                // rather than leaving an empty box looking broken.
                                <input type="text" value={g.label.clone()}
                                       placeholder="unnamed" onchange={on_label}
                                       aria-label={format!("Name of {}", id.0.as_str())} />
                            </td>
                            <td>
                                <select onchange={on_style}
                                        aria-label={format!("Ground of {}", id.0.as_str())}>
                                { for [("", "plain"), ("civic", "blue"), ("green", "green"),
                                       ("market", "amber")].iter().map(|(k, name)| html! {
                                    <option value={*k} selected={current == *k}>{ *name }</option>
                                }) }
                                </select>
                            </td>
                            <td>
                                <input type="text" value={g.note.clone().unwrap_or_default()}
                                       placeholder="\u{2014}" onchange={on_note}
                                       aria-label={format!("Note on {}", id.0.as_str())} />
                            </td>
                            <td class="hc-groups__n">{ members }</td>
                            <td>
                                // DISABLED WHILE ANYTHING IS IN IT, and the command
                                // refuses as well: the button explains, the rule
                                // enforces, and neither is doing the other's job.
                                <button class="hc-btn hc-btn--quiet" onclick={on_delete}
                                        disabled={members > 0}
                                        title={if members > 0 {
                                            "Move its buildings out first"
                                        } else { "Delete this district" }}>
                                    { "Delete" }
                                </button>
                            </td>
                        </tr>
                    }
                }) }
                </tbody>
            </table>

            <div class="hc-groups__new">
                <input type="text" id="hc-new-group" placeholder="new district"
                       aria-label="Name of a new district" />
                <button class="hc-btn" onclick={ {
                    let run = run.clone();
                    let status = status.clone();
                    Callback::from(move |_: MouseEvent| {
                        let Some(input) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.get_element_by_id("hc-new-group"))
                            .and_then(|e| e.dyn_into::<web_sys::HtmlInputElement>().ok())
                        else { return };
                        let label = input.value();
                        // THE SLUG IS DERIVED FROM THE NAME, because a form with
                        // two boxes where one is "a lower-case identifier with no
                        // spaces" is a form that teaches the reader about slugs.
                        let id = label.trim().to_lowercase()
                            .split(|c: char| !c.is_ascii_alphanumeric())
                            .filter(|p| !p.is_empty())
                            .collect::<Vec<_>>().join("-");
                        match Slug::parse(&id) {
                            Ok(sl) => {
                                run(
                                    Command::DeclareGroup {
                                        id: GroupId(sl),
                                        group: Group {
                                            label: label.trim().to_string(),
                                            style_key: None,
                                            note: None,
                                            extra: Vec::new(),
                                        },
                                    },
                                    format!("{id} is a district now."),
                                );
                                input.set_value("");
                            }
                            Err(_) => status.set(Status {
                                text: "A district needs a name with letters or digits in it."
                                    .to_string(),
                                kind: StatusKind::Refused,
                            }),
                        }
                    })
                } }>{ "Add district" }</button>
            </div>

            // MEMBERSHIP, AS A FORM RATHER THAN A GESTURE. We settled that a drag
            // never changes which district a building is in — so this is the way
            // to change it, and `Attach`/`Detach` have been in the core the whole
            // time with nothing able to reach them.
            <p class="hc-groups__member">
            { match selected.as_ref().and_then(|id| diagram.cell_of(id).map(|_| id)) {
                None => html! { <span class="hc-groups__none">
                    { "Select a building to move it between districts." }</span> },
                Some(id) => {
                    let now = diagram.group_of(id).cloned();
                    let on_move = {
                        let run = run.clone();
                        let id = id.clone();
                        Callback::from(move |e: Event| {
                            let Some(sel) = e.target_dyn_into::<web_sys::HtmlSelectElement>()
                            else { return };
                            let v = sel.value();
                            let cmd = match Slug::parse(&v) {
                                Ok(sl) => Command::Attach {
                                    tile: id.clone(),
                                    group: GroupId(sl),
                                },
                                Err(_) => Command::Detach { tile: id.clone() },
                            };
                            run(cmd, format!("{} moved.", id.0.as_str()));
                        })
                    };
                    html! {
                        <>
                            <label for="hc-member">{ format!("{} is in", id.0.as_str()) }</label>
                            <select id="hc-member" onchange={on_move}>
                                <option value="" selected={now.is_none()}>{ "no district" }</option>
                                { for diagram.groups().iter().map(|(gid, g)| html! {
                                    <option value={gid.0.as_str().to_string()}
                                            selected={now.as_ref() == Some(gid)}>
                                        { if g.label.trim().is_empty() {
                                            gid.0.as_str().to_string()
                                          } else { g.label.clone() } }
                                    </option>
                                }) }
                            </select>
                        </>
                    }
                }
            } }
            </p>
        </details>

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

/// The content a `Remove` took off the board, read out of the `Add` that would
/// undo it. The diagram no longer has it — that is the whole point of a
/// self-contained inverse — so this is the only place it still exists.
fn returning(
    applied: &honeycomb_yew::Command,
    inverse: &honeycomb_yew::Command,
) -> Option<(TileId, honeycomb_yew::OwnTile)> {
    match (applied, inverse) {
        (honeycomb_yew::Command::Remove { .. }, honeycomb_yew::Command::Add { tile, what, .. }) => {
            match what {
                NewTile::Own(t) => Some((tile.clone(), (**t).clone())),
                NewTile::Pinned(_) => None,
            }
        }
        _ => None,
    }
}

/// A refused edit, in words a reader of this page can act on. The component
/// writes the sentences for a drop; these are the ones only this page can reach
/// — `run` for the districts table and `on_link` for a drawn line both end here.
///
/// EXHAUSTIVE, WITH NO `_` ARM — the same repair `honeycomb-yew`'s own
/// `unknown()` got in commit `c92b315`, applied here for the same reason and
/// against the identical shape of defect. This used to end `other =>
/// format!("That was refused: {other:?}.")`, which read as "every variant I
/// forgot to write out still gets SOME text" — and what it actually did was
/// let a struct through: `on_link`'s own error handling formatted `{r:?}`
/// directly, with no wording of its own at all, so drawing a line onto one
/// already connected spoke `AlreadyConnected(LinkId(Slug("line-3")))` into this
/// page's status line. Both leaks are fixed the same way: no wildcard, so a
/// `Rejection` variant with no line here is a compile error rather than a
/// dump a reader has to decode.
///
/// Most of these variants are unreachable through this page today — `run`
/// only ever offers `EditGroup`, `RemoveGroup`, `DeclareGroup`, `Attach` and
/// `Detach`, and `on_link` only `Connect` — the same position `unknown()`'s
/// own comment notes for `Occupied` and `NoMove`. Exhaustive anyway, so the
/// day this page grows a new way to refuse an edit it does not get to leak
/// one by omission.
fn refusal(r: &honeycomb_yew::Rejection) -> String {
    use honeycomb_yew::Rejection;
    match r {
        Rejection::GroupInUse { group, members } => format!(
            "{} still has {} in it, so it cannot be deleted. Move them out first.",
            group.0.as_str(),
            members
                .iter()
                .map(|m| m.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Rejection::AlreadyDeclared(g) => {
            format!("There is already a district called {}.", g.0.as_str())
        }
        Rejection::UnknownGroup(g) => {
            format!("{} is not a district of this plan.", g.0.as_str())
        }
        Rejection::UnknownTile(t) => format!("{} is not on this plan.", t.0.as_str()),
        Rejection::AlreadyPlaced(t) => format!("{} is already on this plan.", t.0.as_str()),
        Rejection::WrongMode { .. } => "That building cannot go on this plan.".to_string(),
        Rejection::EmptyLabel(t) => {
            format!(
                "{} needs a name before it can go on the plan.",
                t.0.as_str()
            )
        }
        Rejection::Occupied { blocked } => format!(
            "Blocked by {}.",
            blocked
                .iter()
                .map(|(_, id)| id.0.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Rejection::NoMove => "That would not move anything.".to_string(),
        Rejection::AlreadyConnected(id) => {
            format!("{} is already a line on this plan.", id.0.as_str())
        }
        Rejection::UnknownLink(id) => format!("{} is not a line on this plan.", id.0.as_str()),
        Rejection::NotDrawable(id) => format!(
            "{} would have no direction and no length: a line cannot connect a building to \
             itself.",
            id.0.as_str()
        ),
        Rejection::StillLinked { tile, links } => {
            let names: Vec<&str> = links.iter().map(|l| l.0.as_str()).collect();
            let noun = if names.len() == 1 { "a line" } else { "lines" };
            format!(
                "{} is still connected by {}: {}. Remove {} first, then the building can come \
                 off the plan.",
                tile.0.as_str(),
                noun,
                names.join(", "),
                if names.len() == 1 { "it" } else { "them" }
            )
        }
        Rejection::LastPlacement => "This is the last building. A plan with nothing on it is a \
                                      file that lost its contents, not a blank plan."
            .to_string(),
        Rejection::SubjectCollision {
            local,
            first,
            second,
        } => format!(
            "{second} would be written with the same identifier ({local}) as {first}. Rename \
             one of them."
        ),
    }
}

/// The tile a command moved, for the one question the status line asks of it.
/// `Swap` names two; the one the user grabbed is `a`.
fn moved_tile(cmd: &honeycomb_yew::Command) -> Option<honeycomb_yew::TileId> {
    match cmd {
        honeycomb_yew::Command::Translate { grabbed, .. } => Some(grabbed.clone()),
        honeycomb_yew::Command::Swap { a, .. } => Some(a.clone()),
        honeycomb_yew::Command::Add { tile, .. } => Some(tile.clone()),
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
