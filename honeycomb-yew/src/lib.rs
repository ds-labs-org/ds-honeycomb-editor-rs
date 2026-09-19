//! The Yew component: dragging, dropping, and refusing.
//!
//! Everything that is not a browser lives in `honeycomb-core`, which this crate
//! re-exports so a consumer needs one dependency and not two.
//!
//! THE SEAM, STATED ONCE. This component owns the lattice, occupancy, drop
//! legality, rigid translation, the `<svg>` root and its viewBox, pointer
//! capture and hit-testing, the keyboard equivalents, and the refusal. The HOST
//! owns every pixel of paint, every label, every mark, and the MEANING of
//! `styleKey` and `represents`. Nothing here knows what a tile represents, and
//! that is the line that makes this a component rather than one site's widget.
//!
//! WHY THE HOST DRAWS THE INSIDE OF A HEXAGON AND IS GIVEN NO COORDINATES. The
//! markup it returns is centred on (0,0), and this component decides where that
//! lands. Position-independent markup makes a drag cost one attribute write per
//! frame instead of re-rendering every moving cell, and it lets a host emit each
//! distinct pictogram once into `<defs>` and `<use>` it — which on a real page
//! is the difference between a diagram and a third of a megabyte of repeated
//! path data.

// The whole of the pure crate, re-exported: a consumer adds one dependency and
// gets the model, the geometry and the serialiser with the component. A private
// `use` of the same names here would SHADOW this glob and make them private
// again — which is exactly what happened on the first attempt, and the compiler
// reported it as "struct `Lattice` is private" pointing at core, which reads
// like core is at fault when the import here is.
pub use honeycomb_core::*;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use yew::prelude::*;

// ------------------------------------------------------------- host's view

/// One hexagon, for the host to draw. `r` is the radius the markup should be
/// centred on and drawn at; the component supplies it so a host never has to
/// know the lattice's geometry to fill a cell.
#[derive(Debug, Clone, PartialEq)]
pub struct TileView {
    pub id: TileId,
    pub cell: Cell,
    pub r: f64,
    pub group: Option<GroupId>,
    pub state: TileState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileState {
    Resting,
    Selected,
    /// The tile's committed position while its ghost is elsewhere. The REAL tile
    /// does not move during a drag: Escape has to restore the state, and if the
    /// real tile had already moved, cancelling would mean undoing DOM the model
    /// no longer describes.
    Dragging,
    /// The snapped candidate, drawn at full strength. It SNAPS rather than
    /// following the pointer freely, because a snapped ghost is literally the
    /// truth about what the drop will do.
    Ghost,
    /// The same candidate once it is illegal — drawn AS illegal, before the user
    /// releases. A refusal discovered on release is discovered too late.
    GhostRefused,
    /// A tile that is in the way. "You can't drop here" is useless; "that one is
    /// in the way" is actionable, which is why the rejection carries evidence
    /// and why this state exists for the host to paint it.
    Blocking,
}

/// One group's region. The cells and the outlines are DERIVED on every render
/// from the members' positions — there is no anchor stored anywhere, which is
/// the whole meaning of "the group follows its members".
#[derive(Debug, Clone, PartialEq)]
pub struct GroupView {
    pub id: GroupId,
    pub group: Group,
    pub cells: Vec<Cell>,
    /// Ready-made union outlines: each member's hexagon grown by [`GROW`], for
    /// the host to stroke and then fill over the top so only the perimeter the
    /// union does not cover survives.
    pub paths: Vec<String>,
    pub anchor: (f64, f64),
    /// Reported, never refused: an editor that permits regrouping must permit
    /// the split state between pulling a member out and putting it back.
    pub fractured: bool,
    /// This group's cells touch another group's, so the two grounds will merge
    /// when drawn. A warning, because the arrangement is legal and refusing it
    /// would refuse diagrams that already exist.
    pub touching: bool,
}

/// An empty lattice behind the content, for a host that wants to show where the
/// cells are.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameView {
    pub frame: Frame,
    pub cells: Vec<Cell>,
}

/// ONE STRING, shown in the host's status line AND announced in this
/// component's own aria-live region. One function produces both, so they cannot
/// drift apart.
///
/// IT NAMES SLUGS AND CELLS, NEVER LABELS, and that is the anti-drift property
/// arriving one more time: in a pinned diagram there are no labels here to name
/// — the host holds them. A host that wants prettier words rewrites the text
/// from the ids it already knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub text: String,
    pub kind: StatusKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Refused,
    Warning,
}

/// Everything an accepted edit produced.
///
/// IT CARRIES THE INVERSE, and that is a deliberate addition to the designed
/// callback payload rather than an accident. `Diagram::apply` returns the
/// inverse command and `History::record` takes one; this component is the only
/// thing that ever calls `apply`, so a payload that dropped the inverse would
/// put undo out of the host's reach entirely — and the host would be left
/// snapshotting whole documents to get it back.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub diagram: Rc<Diagram>,
    pub applied: Command,
    pub inverse: Command,
}

/// Grown-hexagon factor for a group region. 2 * 1.16r = 2.32r exceeds the
/// centre-to-centre step of 1.732r, so edge-adjacent cells overlap and read as
/// one region; and 2.32r is short of 3r, the second-ring distance, so
/// non-adjacent cells can NEVER bridge. A clean threshold with no case in
/// between.
pub const GROW: f64 = 1.16;

/// How far a press may travel and still be a SELECTION rather than a drag,
/// measured in CLIENT pixels — not user units, which scale with the viewport.
/// Without a threshold there is no way to select a tile without nudging it a
/// cell.
const PRESS_SLOP: f64 = 4.0;

// ----------------------------------------------------------------- props

#[derive(Properties, PartialEq)]
pub struct HoneycombProps {
    /// CONTROLLED. The component never mutates it; it asks.
    pub diagram: Rc<Diagram>,
    pub lattice: Lattice,
    #[prop_or(26.0)]
    pub pad: f64,
    /// How many empty rings beyond the content the frame callback is handed.
    #[prop_or(1)]
    pub frame_ring: i32,

    /// THE HOST DRAWS THE INSIDE OF A HEXAGON. Returns markup CENTRED ON (0,0)
    /// at `view.r`; this component wraps it and decides where it is.
    ///
    /// GOTCHA: build this with `use_callback` or `use_memo`. `Callback`'s
    /// `PartialEq` is `Rc::ptr_eq`, so a closure rebuilt on every render makes
    /// the props unequal and the editor re-renders forever.
    pub tile: Callback<TileView, Html>,
    /// Without it, groups get no region and no heading.
    #[prop_or_default]
    pub ground: Option<Callback<GroupView, Html>>,
    #[prop_or_default]
    pub frame: Option<Callback<FrameView, Html>>,

    /// Every accepted command, with the new document. The host owns the state.
    pub on_change: Callback<Change>,
    /// Every refusal, so the host can name the blockers. Never fires for a
    /// no-op.
    #[prop_or_default]
    pub on_reject: Callback<Rejection>,
    #[prop_or_default]
    pub on_status: Callback<Status>,
    #[prop_or_default]
    pub on_select: Callback<Option<TileId>>,

    #[prop_or_default]
    pub readonly: bool,
    #[prop_or_default]
    pub class: Classes,
    #[prop_or_default]
    pub aria_label: AttrValue,
}

// ------------------------------------------------------------- drag state

/// What a press has become. `Pressed` is not yet a drag: below [`PRESS_SLOP`] a
/// release is a selection.
#[derive(Clone, PartialEq)]
struct Press {
    grabbed: TileId,
    origin: Cell,
    detach: bool,
    from_client: (f64, f64),
    /// None until the press crosses the slop and becomes a drag.
    drag: Option<Drag>,
}

#[derive(Clone, PartialEq)]
struct Drag {
    candidate: Cell,
    blocked: Vec<(Cell, TileId)>,
}

impl Press {
    fn delta(&self, candidate: Cell) -> Axial {
        candidate.to_axial().minus(self.origin.to_axial())
    }
}

/// The one function that turns a verdict into words, so the status line and the
/// aria-live region cannot say different things about the same drop.
fn describe(grabbed: &TileId, candidate: Cell, verdict: &Result<Plan, Rejection>) -> Status {
    let who = grabbed.0.as_str();
    let Cell { col, row } = candidate;
    match verdict {
        Ok(_) => Status {
            text: format!("{who}, column {col} row {row}. Free."),
            kind: StatusKind::Info,
        },
        Err(Rejection::Occupied { blocked }) => {
            let names: Vec<&str> = blocked.iter().map(|(_, id)| id.0.as_str()).collect();
            Status {
                text: format!(
                    "{who}, column {col} row {row}. Blocked by {}.",
                    join(&names)
                ),
                kind: StatusKind::Refused,
            }
        }
        Err(Rejection::NoMove) => Status {
            text: format!("{who}, column {col} row {row}. Where it already is."),
            kind: StatusKind::Info,
        },
        Err(other) => Status {
            text: format!("{who} cannot move: {other:?}."),
            kind: StatusKind::Refused,
        },
    }
}

fn join(names: &[&str]) -> String {
    match names {
        [] => "nothing".to_string(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

// ------------------------------------------------------------- the component

#[function_component]
pub fn Honeycomb(props: &HoneycombProps) -> Html {
    let press = use_state(|| Option::<Press>::None);
    let selected = use_state(|| Option::<TileId>::None);
    let live = use_state(String::new);
    let root = use_node_ref();

    let d = &props.diagram;
    let l = props.lattice;

    let frame = Frame::around(d.cells().map(|(c, _)| c), l, props.pad)
        .expect("a Diagram always holds at least one placement, so a frame around it exists");

    // Board -> user units through the SVG's own screen CTM, never by hand: the
    // element is width:100% inside a scrolling box on a real page, and manual
    // arithmetic against a bounding rect is right only while nothing has
    // scrolled or transformed it.
    let to_user = {
        let root = root.clone();
        move |cx: f64, cy: f64| -> Option<(f64, f64)> {
            let svg = root.cast::<web_sys::SvgsvgElement>()?;
            let inv = svg.get_screen_ctm()?.inverse().ok()?;
            let (a, b, c, dd, e, f) = (
                inv.a() as f64,
                inv.b() as f64,
                inv.c() as f64,
                inv.d() as f64,
                inv.e() as f64,
                inv.f() as f64,
            );
            Some((a * cx + c * cy + e, b * cx + dd * cy + f))
        }
    };

    let commit = {
        let diagram = d.clone();
        let on_change = props.on_change.clone();
        let on_reject = props.on_reject.clone();
        let on_status = props.on_status.clone();
        let live = live.clone();
        move |cmd: Command, status: Status| {
            let mut next = (*diagram).clone();
            match next.apply(cmd.clone()) {
                Ok(inverse) => {
                    on_change.emit(Change {
                        diagram: Rc::new(next),
                        applied: cmd,
                        inverse,
                    });
                }
                Err(r) => on_reject.emit(r),
            }
            live.set(status.text.clone());
            on_status.emit(status);
        }
    };

    let onpointerdown = {
        let press = press.clone();
        let diagram = d.clone();
        let readonly = props.readonly;
        Callback::from(move |(id, ev): (TileId, PointerEvent)| {
            if readonly {
                return;
            }
            ev.prevent_default();
            // Pointer capture on the SVG, not the tile: a fast drag that leaves
            // the element loses pointermove otherwise, and the tile freezes in
            // mid-air with the pointer somewhere else entirely.
            if let Some(t) = ev.target_dyn_into::<web_sys::Element>() {
                let _ = t.set_pointer_capture(ev.pointer_id());
            }
            let Some(origin) = diagram.cell_of(&id) else {
                return;
            };
            press.set(Some(Press {
                grabbed: id,
                origin,
                // ALT = DETACH: the moving set is the grabbed tile alone even
                // when it has a group. A general editor needs it; a host whose
                // grouping is somebody else's truth simply does not document it.
                detach: ev.alt_key(),
                from_client: (ev.client_x() as f64, ev.client_y() as f64),
                drag: None,
            }));
        })
    };

    let onpointermove = {
        let press = press.clone();
        let diagram = d.clone();
        let to_user = to_user.clone();
        let on_status = props.on_status.clone();
        let live = live.clone();
        Callback::from(move |ev: PointerEvent| {
            let Some(p) = (*press).clone() else { return };
            let (cx, cy) = (ev.client_x() as f64, ev.client_y() as f64);
            let travelled = (cx - p.from_client.0).hypot(cy - p.from_client.1);
            if p.drag.is_none() && travelled < PRESS_SLOP {
                return;
            }
            let Some((x, y)) = to_user(cx, cy) else {
                return;
            };
            let current = p.drag.as_ref().map(|d| d.candidate).unwrap_or(p.origin);
            // Hysteresis lives in the lattice, not here: cube rounding resolves
            // an exactly-equidistant point arbitrarily, so a pointer resting on
            // an edge flickers between two cells as the last float bit moves.
            let candidate = l.next_candidate(current, x - frame.origin_x, y - frame.origin_y);
            let verdict = diagram.check(&Command::Translate {
                grabbed: p.grabbed.clone(),
                delta: p.delta(candidate),
                detach: p.detach,
            });
            let blocked = match &verdict {
                Err(Rejection::Occupied { blocked }) => blocked.clone(),
                _ => Vec::new(),
            };
            let changed = p.drag.as_ref().map(|d| d.candidate) != Some(candidate)
                || p.drag.as_ref().map(|d| &d.blocked) != Some(&blocked);
            if !changed {
                return;
            }
            let status = describe(&p.grabbed, candidate, &verdict);
            live.set(status.text.clone());
            on_status.emit(status);
            press.set(Some(Press {
                drag: Some(Drag { candidate, blocked }),
                ..p
            }));
        })
    };

    let onpointerup = {
        let press = press.clone();
        let diagram = d.clone();
        let selected = selected.clone();
        let on_select = props.on_select.clone();
        let commit = commit.clone();
        Callback::from(move |_: PointerEvent| {
            let Some(p) = (*press).clone() else { return };
            press.set(None);
            let Some(drag) = p.drag.clone() else {
                // Under the slop: a press is a selection, and firing an edit
                // here would put an undo entry on the stack for every click.
                selected.set(Some(p.grabbed.clone()));
                on_select.emit(Some(p.grabbed));
                return;
            };
            let cmd = Command::Translate {
                grabbed: p.grabbed.clone(),
                delta: p.delta(drag.candidate),
                detach: p.detach,
            };
            let verdict = diagram.check(&cmd);
            let status = describe(&p.grabbed, drag.candidate, &verdict);
            match verdict {
                // NoMove is neither a change nor a refusal: the host turns it
                // into a selection rather than flashing an error.
                Err(Rejection::NoMove) => {
                    selected.set(Some(p.grabbed.clone()));
                    on_select.emit(Some(p.grabbed));
                }
                _ => commit(cmd, status),
            }
        })
    };

    let oncancel = {
        let press = press.clone();
        Callback::from(move |_: PointerEvent| press.set(None))
    };

    // The keyboard path runs the IDENTICAL check/apply as the pointer path, so
    // the two cannot drift. A drag-only editor fails WCAG 2.1 SC 2.1.1 outright.
    let onkeydown = {
        let press = press.clone();
        let selected = selected.clone();
        let diagram = d.clone();
        let on_select = props.on_select.clone();
        let on_status = props.on_status.clone();
        let live = live.clone();
        let readonly = props.readonly;
        Callback::from(move |ev: KeyboardEvent| {
            let ids: Vec<TileId> = diagram.cells().map(|(_, id)| id.clone()).collect();
            if ids.is_empty() {
                return;
            }
            let key = ev.key();
            let held = (*press).clone();

            // Escape restores rather than applies, which it can only do because
            // the real tile never moved in the first place.
            if key == "Escape" {
                if held.is_some() {
                    ev.prevent_default();
                    press.set(None);
                }
                return;
            }

            if let Some(p) = held {
                let Some(drag) = p.drag.clone() else { return };
                let step = match key.as_str() {
                    // The six axial neighbours are (+-1,0), (0,+-1), (+1,-1) and
                    // (-1,+1), so four are one keypress away and the other two
                    // are two — all six reachable with four keys.
                    "ArrowLeft" => Axial { q: -1, r: 0 },
                    "ArrowRight" => Axial { q: 1, r: 0 },
                    "ArrowUp" => Axial { q: 0, r: -1 },
                    "ArrowDown" => Axial { q: 0, r: 1 },
                    " " | "Enter" => {
                        ev.prevent_default();
                        let cmd = Command::Translate {
                            grabbed: p.grabbed.clone(),
                            delta: p.delta(drag.candidate),
                            detach: p.detach,
                        };
                        let verdict = diagram.check(&cmd);
                        let status = describe(&p.grabbed, drag.candidate, &verdict);
                        press.set(None);
                        if !matches!(verdict, Err(Rejection::NoMove)) {
                            commit(cmd, status);
                        }
                        return;
                    }
                    _ => return,
                };
                ev.prevent_default();
                let candidate = Cell::from_axial(drag.candidate.to_axial().plus(step));
                let verdict = diagram.check(&Command::Translate {
                    grabbed: p.grabbed.clone(),
                    delta: p.delta(candidate),
                    detach: p.detach,
                });
                let blocked = match &verdict {
                    Err(Rejection::Occupied { blocked }) => blocked.clone(),
                    _ => Vec::new(),
                };
                let status = describe(&p.grabbed, candidate, &verdict);
                live.set(status.text.clone());
                on_status.emit(status);
                press.set(Some(Press {
                    drag: Some(Drag { candidate, blocked }),
                    ..p
                }));
                return;
            }

            // Roving selection over TILES rather than cells: there are a couple
            // of dozen tiles and hundreds of cells, and a reader arrowing
            // through empty lattice learns nothing.
            let here = selected
                .as_ref()
                .and_then(|s| ids.iter().position(|i| i == s));
            let pick = |n: usize| {
                let id = ids[n].clone();
                selected.set(Some(id.clone()));
                on_select.emit(Some(id.clone()));
                if let Some(cell) = diagram.cell_of(&id) {
                    let text = format!("{}, column {} row {}.", id.0.as_str(), cell.col, cell.row);
                    live.set(text.clone());
                    on_status.emit(Status {
                        text,
                        kind: StatusKind::Info,
                    });
                }
            };
            match key.as_str() {
                "ArrowRight" | "ArrowDown" => {
                    ev.prevent_default();
                    pick(here.map(|i| (i + 1) % ids.len()).unwrap_or(0));
                }
                "ArrowLeft" | "ArrowUp" => {
                    ev.prevent_default();
                    pick(here.map(|i| (i + ids.len() - 1) % ids.len()).unwrap_or(0));
                }
                "Home" => {
                    ev.prevent_default();
                    pick(0);
                }
                "End" => {
                    ev.prevent_default();
                    pick(ids.len() - 1);
                }
                " " | "Enter" => {
                    if readonly {
                        return;
                    }
                    let Some(id) = (*selected).clone() else {
                        return;
                    };
                    let Some(origin) = diagram.cell_of(&id) else {
                        return;
                    };
                    ev.prevent_default();
                    press.set(Some(Press {
                        grabbed: id,
                        origin,
                        detach: ev.alt_key(),
                        from_client: (0.0, 0.0),
                        drag: Some(Drag {
                            candidate: origin,
                            blocked: Vec::new(),
                        }),
                    }));
                }
                _ => {}
            }
        })
    };

    // ------------------------------------------------------------- painting

    let p = (*press).clone();
    let moving: BTreeSet<TileId> = p
        .as_ref()
        .filter(|p| p.drag.is_some())
        .map(|p| d.moving_set(&p.grabbed, p.detach))
        .unwrap_or_default();
    let blocked_ids: BTreeSet<TileId> = p
        .as_ref()
        .and_then(|p| p.drag.as_ref())
        .map(|dr| dr.blocked.iter().map(|(_, id)| id.clone()).collect())
        .unwrap_or_default();
    let refused = !blocked_ids.is_empty();

    let (vx, vy, vw, vh) = frame.viewbox(l);

    let state_of = |id: &TileId| -> TileState {
        if blocked_ids.contains(id) {
            TileState::Blocking
        } else if moving.contains(id) {
            TileState::Dragging
        } else if selected.as_ref() == Some(id) {
            TileState::Selected
        } else {
            TileState::Resting
        }
    };

    let draw = |id: &TileId, cell: Cell, state: TileState, ghost: bool| -> Html {
        let (x, y) = frame.at(cell, l);
        let inner = props.tile.emit(TileView {
            id: id.clone(),
            cell,
            r: l.r,
            group: d.group_of(id).cloned(),
            state,
        });
        let handler = {
            let cb = onpointerdown.clone();
            let id = id.clone();
            Callback::from(move |ev: PointerEvent| cb.emit((id.clone(), ev)))
        };
        html! {
            <g
                class={classes!(
                    "hc-tile",
                    ghost.then_some("hc-tile--ghost"),
                    matches!(state, TileState::Dragging).then_some("is-moving"),
                    matches!(state, TileState::Blocking).then_some("is-blocking"),
                    matches!(state, TileState::GhostRefused).then_some("is-refused"),
                    matches!(state, TileState::Selected).then_some("is-selected"),
                )}
                data-tile={id.0.as_str().to_string()}
                transform={format!("translate({x:.3} {y:.3})")}
                role="img"
                aria-label={format!("{}, column {} row {}{}",
                    id.0.as_str(), cell.col, cell.row,
                    d.group_of(id).map(|g| format!(", in {}", g.0.as_str())).unwrap_or_default())}
                onpointerdown={(!ghost).then_some(handler)}
            >{ inner }</g>
        }
    };

    let touching: BTreeSet<GroupId> = d
        .touching_groups()
        .into_iter()
        .flat_map(|(a, b)| [a, b])
        .collect();

    html! {
        <svg
            ref={root}
            class={classes!("hc-board", props.class.clone(), refused.then_some("is-refused"))}
            viewBox={format!("{vx:.3} {vy:.3} {vw:.3} {vh:.3}")}
            role="application"
            tabindex="0"
            aria-roledescription="hexagon lattice diagram editor"
            aria-label={props.aria_label.clone()}
            onpointermove={onpointermove}
            onpointerup={onpointerup.clone()}
            onpointercancel={oncancel}
            onpointerleave={onpointerup}
            onkeydown={onkeydown}
        >
            // NO ANIMATION ANYWHERE — the ghost snaps and nothing tweens — so
            // prefers-reduced-motion needs no branch. Said here so a reader is
            // not left wondering where it went.
            { props.frame.as_ref().map(|cb| cb.emit(FrameView {
                frame,
                cells: ring(&frame, props.frame_ring),
            })).unwrap_or_default() }

            { for props.ground.iter().flat_map(|cb| grounds(d, l, frame, &touching).into_iter().map(move |v| cb.emit(v))) }

            { for d.cells().map(|(cell, id)| draw(id, cell, state_of(id), false)) }

            { for p.as_ref().and_then(|p| p.drag.as_ref().map(|dr| (p, dr))).into_iter().flat_map(|(p, dr)| {
                let delta = p.delta(dr.candidate);
                let state = if refused { TileState::GhostRefused } else { TileState::Ghost };
                moving.iter().map(move |id| {
                    let at = d.cell_of(id).unwrap_or(dr.candidate);
                    draw(id, Cell::from_axial(at.to_axial().plus(delta)), state, true)
                }).collect::<Vec<Html>>()
            }) }

            // The announcement and the host's status line are the same string,
            // produced by `describe`. A second wording here is a second thing to
            // keep true.
            // fill="none" rather than display:none or a zero size: a hidden
            // live region is not announced at all, which would leave a
            // keyboard user with no feedback whatsoever.
            <text class="hc-live" aria-live="polite" x="0" y="0" fill="none">{ (*live).clone() }</text>
        </svg>
    }
}

/// The empty cells `n` rings beyond the content, for the frame callback.
fn ring(f: &Frame, n: i32) -> Vec<Cell> {
    let mut out = Vec::new();
    for row in (f.min.row - n)..=(f.max.row + n) {
        for col in (f.min.col - n)..=(f.max.col + n) {
            out.push(Cell { col, row });
        }
    }
    out
}

fn grounds(d: &Diagram, l: Lattice, f: Frame, touching: &BTreeSet<GroupId>) -> Vec<GroupView> {
    let mut by_group: BTreeMap<GroupId, Vec<Cell>> = BTreeMap::new();
    for (cell, id) in d.cells() {
        if let Some(g) = d.group_of(id) {
            by_group.entry(g.clone()).or_default().push(cell);
        }
    }
    by_group
        .into_iter()
        .filter_map(|(id, cells)| {
            let group = d.group(&id)?.clone();
            let paths = cells
                .iter()
                .map(|c| {
                    let (x, y) = f.at(*c, l);
                    l.hex_path(x, y, l.r * GROW)
                })
                .collect();
            // The anchor is the mean of the members' centres, RECOMPUTED here on
            // every render rather than stored on the group: an anchor written
            // beside the members disagrees with them the moment one is dragged.
            let n = cells.len() as f64;
            let (sx, sy) = cells.iter().fold((0.0, 0.0), |(ax, ay), c| {
                let (x, y) = f.at(*c, l);
                (ax + x, ay + y)
            });
            let fractured = d.group_components(&id).len() > 1;
            Some(GroupView {
                touching: touching.contains(&id),
                id,
                group,
                paths,
                anchor: (sx / n, sy / n),
                fractured,
                cells,
            })
        })
        .collect()
}
