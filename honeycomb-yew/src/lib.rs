//! The Yew component: dragging, dropping, and refusing.
//!
//! Everything that is not a browser lives in `honeycomb-core`, which this crate
//! re-exports so a consumer needs one dependency and not two.
//!
//! WHAT THE HOST SUPPLIES AND WHAT THIS SUPPLIES. The host owns content and
//! appearance: what a tile is called, which group it belongs to, and the class
//! names that colour it. This owns the lattice, the pointer arithmetic, the rule
//! that two tiles may never occupy one cell, and the rule that a group moves
//! with its members. Nothing here knows what a tile represents, and that is the
//! line that makes the component reusable rather than one site's widget.

// The whole of the pure crate, re-exported: a consumer adds one dependency and
// gets the model, the geometry and the serialiser with the component. A private
// `use` of the same names here would SHADOW this glob and make them private
// again — which is exactly what happened on the first attempt, and the compiler
// reported it as "struct `Lattice` is private" pointing at core, which reads
// like core is at fault when the import here is.
pub use honeycomb_core::*;

use yew::prelude::*;

// ---------------------------------------------------------------- component

#[derive(Properties, PartialEq)]
pub struct EditorProps {
    pub lattice: Lattice,
    /// Fired on a committed move, with the new lattice.
    #[prop_or_default]
    pub on_change: Callback<Lattice>,
    /// Fired when a drop is refused, naming the blocker.
    #[prop_or_default]
    pub on_refused: Callback<Refusal>,
}

#[derive(Clone, PartialEq)]
struct Drag {
    tile: String,
    moving: Vec<String>,
    dq: i32,
    dr: i32,
    blocked: bool,
}

#[function_component(HoneycombEditor)]
pub fn honeycomb_yew(props: &EditorProps) -> Html {
    let drag = use_state(|| Option::<Drag>::None);
    let board = use_node_ref();
    let lat = &props.lattice;

    // Board -> viewBox coordinates. Only ever called from a pointer callback,
    // never during render, which is what keeps the component SSR-safe.
    let to_board = {
        let board = board.clone();
        move |cx: f64, cy: f64| -> Option<(f64, f64)> {
            let el = board.cast::<web_sys::Element>()?;
            let rect = el.get_bounding_client_rect();
            if rect.width() <= 0.0 {
                return None;
            }
            let scale = VIEW_W / rect.width();
            Some(((cx - rect.left()) * scale, (cy - rect.top()) * scale))
        }
    };

    let onpointerdown = {
        let drag = drag.clone();
        let lat = lat.clone();
        Callback::from(move |(id, ev): (String, PointerEvent)| {
            ev.prevent_default();
            if let Some(t) = ev.target_dyn_into::<web_sys::Element>() {
                let _ = t.set_pointer_capture(ev.pointer_id());
            }
            drag.set(Some(Drag {
                moving: lat.moving_set(&id),
                tile: id,
                dq: 0,
                dr: 0,
                blocked: false,
            }));
        })
    };

    let onpointermove = {
        let drag = drag.clone();
        let lat = lat.clone();
        let to_board = to_board.clone();
        Callback::from(move |ev: PointerEvent| {
            let Some(d) = (*drag).clone() else { return };
            let Some((x, y)) = to_board(ev.client_x() as f64, ev.client_y() as f64) else {
                return;
            };
            let Some(t) = lat.tile(&d.tile) else { return };
            let (tq, tr) = nearest_cell(x, y);
            let (dq, dr) = (tq - t.q, tr - t.r);
            let blocked = lat.check_move(&d.moving, dq, dr).is_err();
            if d.dq != dq || d.dr != dr || d.blocked != blocked {
                drag.set(Some(Drag { dq, dr, blocked, ..d }));
            }
        })
    };

    let onpointerup = {
        let drag = drag.clone();
        let lat = lat.clone();
        let on_change = props.on_change.clone();
        let on_refused = props.on_refused.clone();
        Callback::from(move |_: PointerEvent| {
            let Some(d) = (*drag).clone() else { return };
            drag.set(None);
            if d.dq == 0 && d.dr == 0 {
                return;
            }
            match lat.check_move(&d.moving, d.dq, d.dr) {
                Ok(()) => on_change.emit(lat.apply_move(&d.moving, d.dq, d.dr)),
                Err(r) => on_refused.emit(r),
            }
        })
    };

    let d = (*drag).clone();
    let ghost_target = d.as_ref().map(|d| {
        let t = lat.tile(&d.tile).unwrap();
        (t.q + d.dq, t.r + d.dr, d.blocked)
    });

    html! {
        <svg
            ref={board}
            class={classes!("hc-board", d.as_ref().map(|d| if d.blocked { "is-blocked" } else { "is-dragging" }))}
            viewBox={format!("0 0 {VIEW_W} {VIEW_H}")}
            role="application"
            aria-roledescription="honeycomb lattice"
            aria-label={format!("{}, {} tiles on {} cells", lat.label, lat.tiles.len(), lat.cells.len())}
            onpointermove={onpointermove.clone()}
            onpointerup={onpointerup.clone()}
            onpointercancel={onpointerup.clone()}
        >
            <defs>
                // The refusal hatch. Colour alone cannot carry a refusal -- the
                // status red against the categorical orange measures below the
                // perceptual floor -- so texture does the work and red only
                // reinforces it.
                <pattern id="hc-hatch-blocked" width="8" height="8"
                         patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
                    // The wash travels inside the pattern so one polygon can
                    // carry both it and the hatch; a tile cannot have two fills.
                    <rect class="hc-hatch-bg" width="8" height="8" />
                    <line x1="0" y1="0" x2="0" y2="8" class="hc-hatch-line" />
                </pattern>
            </defs>

            { for lat.cells.iter().map(|c| {
                let (x, y) = axial(c.q, c.r);
                let blocked = ghost_target
                    .as_ref()
                    .is_some_and(|(gq, gr, b)| *b && *gq == c.q && *gr == c.r);
                html! {
                    <g class="hc-cell-g">
                        <polygon
                            class={classes!("hc-cell", blocked.then_some("hc-cell--blocked"))}
                            points={hex_points(x, y, R)} />
                        <text class="hc-cell__ref" x={fmt(x)} y={fmt(y + R - 12.0)}>
                            { cell_ref(c.q, c.r) }
                        </text>
                    </g>
                }
            }) }

            { for lat.tiles.iter().map(|t| {
                let (x, y) = axial(t.q, t.r);
                let moving = d.as_ref().is_some_and(|d| d.moving.contains(&t.id));
                let (ox, oy) = match (&d, moving) {
                    (Some(d), true) => {
                        let (nx, ny) = axial(t.q + d.dq, t.r + d.dr);
                        (nx - x, ny - y)
                    }
                    _ => (0.0, 0.0),
                };
                let slot = t.group.as_deref()
                    .and_then(|g| lat.group(g))
                    .map(|g| g.slot.to_string())
                    .unwrap_or_else(|| "0".into());
                let onpointerdown = {
                    let cb = onpointerdown.clone();
                    let id = t.id.clone();
                    Callback::from(move |ev: PointerEvent| cb.emit((id.clone(), ev)))
                };
                html! {
                    <g
                        class={classes!("hc-tile",
                            moving.then_some("is-moving"),
                            (moving && d.as_ref().is_some_and(|d| d.blocked)).then_some("is-blocked"))}
                        data-tile={t.id.clone()}
                        data-slot={slot}
                        transform={format!("translate({} {})", fmt(ox), fmt(oy))}
                        {onpointerdown}
                    >
                        <polygon class="hc-tile__hex" points={hex_points(x, y, R - 4.0)} />
                        <text class="hc-tile__label" x={fmt(x)} y={fmt(y - 2.0)}>{ &t.label }</text>
                        <text class="hc-tile__group" x={fmt(x)} y={fmt(y + 16.0)}>
                            { t.group.as_deref().and_then(|g| lat.group(g)).map(|g| g.label.clone()).unwrap_or_default() }
                        </text>
                    </g>
                }
            }) }
        </svg>
    }
}

pub const VIEW_W: f64 = 760.0;
pub const VIEW_H: f64 = 400.0;

fn fmt(v: f64) -> String {
    format!("{v:.2}")
}
