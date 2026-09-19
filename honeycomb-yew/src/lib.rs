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
    /// This tile is the keyboard's roving selection AND the board currently has
    /// focus. Separate from `Selected` because a selection made with a click
    /// must not paint a focus ring — that is what `:focus-visible` is for in a
    /// world with stylesheets, and this component cannot assume it has one: its
    /// host may draw entirely in presentation attributes.
    pub focused: bool,
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
    /// About to be displaced by a swap: a tile the user did NOT grab that WILL
    /// move when they release.
    ///
    /// DELIBERATELY NOT `Blocking`, which both known hosts paint in a refusal
    /// red. This drop is being ACCEPTED, and a second tile moving is the single
    /// moment a user is most likely to think the editor has malfunctioned — so
    /// it is the one state that most needs its own paint rather than a reused
    /// one.
    Displacing,
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
    /// WHERE THE HANDLE GOES, and it is not the anchor.
    ///
    /// A heading placed above the CENTROID of a two-row group lands on that
    /// group's own top row — and since the heading is now a grab handle, a
    /// pointer aimed at it reaches the hexagon underneath instead and drags one
    /// tile out of the cluster the user was trying to move. That is not a
    /// styling nitpick; it is the group gesture silently not working, and it is
    /// what the demo did the first time it was driven.
    ///
    /// So the component computes a point clear of its own cells: horizontally
    /// centred on the group's topmost row, one hexagon-half plus a little above
    /// it.
    ///
    /// IT ONLY DODGES THIS GROUP, AND A HOST WITH A BUSY BOARD MUST DO THE REST.
    /// The ground layer is drawn BEFORE the tiles — it has to be, or a region
    /// would cover the hexagons it describes and take every press aimed at them
    /// — so a heading that lands on a NEIGHBOURING group's tile is painted
    /// under it and is unpressable there. This callback is handed one group at a
    /// time and cannot see the others, so the fix belongs to the host: read
    /// every group's cells, and lift a colliding heading by a row-pitch until it
    /// is clear. `developer.eona-x.eu` does exactly that, and its `headings()`
    /// starts from this point.
    pub heading: (f64, f64),
    /// How many connected pieces this group is in RIGHT NOW, including while a
    /// drag is in flight — the cells above are the previewed ones, so this
    /// counts what the user is about to get rather than what they still have.
    ///
    /// A COUNT AND NOT A BOOLEAN, because the boolean was already being used to
    /// print a number: the demo said "(2 pieces)" unconditionally, which is
    /// wrong the moment a group is in three — and now that a plain tile drag
    /// detaches, three is two gestures away. One field, so a host cannot print a
    /// count that disagrees with the flag beside it.
    pub pieces: usize,
    /// This group's cells touch another group's, so the two grounds will merge
    /// when drawn. A warning, because the arrangement is legal and refusing it
    /// would refuse diagrams that already exist.
    pub touching: bool,
    /// The pointer is over this group's region. The host paints the affordance;
    /// the component only knows where the pointer is.
    pub hovered: bool,
    /// This group is the one being dragged.
    pub grabbed: bool,
}

impl GroupView {
    /// Reported, never refused: an editor that permits regrouping must permit
    /// the split state between pulling a member out and putting it back.
    pub fn fractured(&self) -> bool {
        self.pieces > 1
    }
}

/// An empty lattice behind the content, for a host that wants to show where the
/// cells are.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameView {
    pub frame: Frame,
    pub cells: Vec<Cell>,
    /// A palette item is armed, so these empty cells are TARGETS.
    ///
    /// Without it "you may now drop something" has no affordance whatsoever: the
    /// board looks identical whether or not the user has armed a chip, and the
    /// only feedback is the status line. The host paints it.
    pub armed: bool,
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
    /// How many empty rings beyond the content the frame callback is handed, and
    /// therefore how much empty comb is inside the viewBox.
    ///
    /// NEGATIVE VALUES ARE CLAMPED TO ZERO rather than refused: at -2 on a small
    /// board the grown rectangle is empty, `Frame::around` returns None for an
    /// empty iterator, and the component panicked on its first render on an
    /// `expect`. A host passing a negative ring means "none".
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

    /// THE ARMED PALETTE ITEM. Controlled, like `diagram`: this component never
    /// sets it, it reports through `on_pending` and asks.
    ///
    /// TWO HOST CONTRACTS NO TYPE CAN ENFORCE. The host must NOT call
    /// `setPointerCapture` on its palette chip — captured, the board receives no
    /// pointermove at all and the drag is silently dead. And a chip meant to be
    /// dragged needs `touch-action: none`, while a chip in a scrolling drawer
    /// needs `touch-action: manipulation` or a finger cannot scroll past it.
    #[prop_or_default]
    pub pending: Option<Pending>,
    /// Why the pending item stopped being pending, so the host can put focus
    /// back where the user left it.
    #[prop_or_default]
    pub on_pending: Callback<PendingEnd>,
    /// Whether a tile may be taken OFF the board — by dragging it clear, or by
    /// pressing Delete or Backspace on the selection.
    ///
    /// DEFAULTS TO FALSE, so every host that bumps this crate keeps exactly the
    /// editor it had. A diagram with no palette has no way to put a tile back.
    #[prop_or_default]
    pub removable: bool,

    #[prop_or_default]
    pub readonly: bool,
    #[prop_or_default]
    pub class: Classes,
    #[prop_or_default]
    pub aria_label: AttrValue,
}

// ------------------------------------------------------------- drag state

/// A TILE THE HOST IS OFFERING, waiting for somewhere to go.
///
/// THE HOST OWNS THE PALETTE AND THIS COMPONENT OWNS THE BOARD, and this is the
/// whole of the contract between them. The host draws its own chips, decides
/// what a chip means and arms one; the component works out which cell the
/// pointer is over, whether the drop is legal, paints the preview and reports
/// what happened. Nothing here knows what a palette looks like.
///
/// `at` IS THE ONLY CONSTRUCTOR OF A [`Command::Add`] ANYWHERE. A host that
/// built its own could arm one tile and add another; that state is not reachable
/// through this type.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: TileId,
    pub what: NewTile,
}

impl Pending {
    pub fn at(&self, cell: Cell) -> Command {
        Command::Add {
            tile: self.id.clone(),
            at: cell,
            what: self.what.clone(),
        }
    }
}

/// Why a pending tile stopped being pending.
///
/// TWO VARIANTS AND NOT `Option<Pending>`, because the host has to move focus and
/// cannot decide where without knowing which happened. `Placed` leaves focus on
/// the board, which is where the user is looking. `Cancelled` has to send it back
/// to the chip that armed it — and if the host guesses wrong there, focus lands
/// on `<body>` and a keyboard user is returned to the top of the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingEnd {
    Placed,
    Cancelled,
}

/// WHAT THE POINTER WENT DOWN ON, and therefore what moves.
///
/// THIS IS THE WHOLE GESTURE, AND IT USED TO BE A MODIFIER KEY. Pressing a
/// hexagon moved its entire group and Alt narrowed that to one tile. Two things
/// were wrong with it: Alt+drag is claimed by the window manager on GNOME and
/// never reaches the page at all, and a rule you can only discover by holding a
/// key is a rule nobody discovers. So the thing under the pointer is now the
/// thing that moves — a tile drags alone, a group's ground or heading drags the
/// cluster — and `detach` is derived from this rather than read from an event.
#[derive(Clone, PartialEq)]
enum Grip {
    Tile(TileId),
    Group(GroupId),
    /// A tile that is not on the board yet. It carries its payload so that
    /// `command_for` is a pure function of the press and a cell.
    New(Pending),
}

/// The gesture rule, as one total function over the grip.
///
/// A FUNCTION AND NOT TWO LITERALS AT THE CALL SITES, because those call sites
/// are pointer handlers and this crate has no browser to test them in — which
/// leaves the single most important rule in the component covered by reading.
/// Pulled out here, it is a pure function the host-target test module below
/// pins, and the handlers become too short to get wrong.
fn detach_for(grip: &Grip) -> bool {
    match grip {
        Grip::Tile(_) => true,
        Grip::Group(_) => false,
        // A tile that is not on the board has no group to carry with it.
        Grip::New(_) => true,
    }
}

/// THE COMMAND A PRESS BECOMES, in one place.
///
/// The pointer path and the keyboard path used to build their own `Translate`,
/// three lines each, four call sites. One function instead, so "what does
/// releasing here do" has exactly one answer and the host-target tests can ask
/// it.
///
/// IT WAS INTRODUCED AND THEN NOT USED IN THREE OF THE FIVE PLACES, which cost
/// exactly what this doc predicted. For a `Grip::New` the grabbed tile is not on
/// the board, so a hand-rolled `Translate` hits `check`'s first line and is
/// refused with `UnknownTile` unconditionally — so dragging a palette chip
/// narrated "… is not on this board" on every pointer move, drew its ghost at
/// full strength over occupied cells because the verdict was never `Occupied`,
/// and placing with the keyboard did not work at all. Every site that turns a
/// press into a command goes through here now; `every_press_site_uses_command_for`
/// is the test.
fn command_for(p: &Press, candidate: Cell) -> Command {
    match &p.grip {
        Grip::New(w) => w.at(candidate),
        _ => Command::Translate {
            grabbed: p.grabbed.clone(),
            delta: p.delta(candidate),
            detach: p.detach,
        },
    }
}

/// What a press has become. `Pressed` is not yet a drag: below [`PRESS_SLOP`] a
/// release is a selection.
#[derive(Clone, PartialEq)]
struct Press {
    grabbed: TileId,
    grip: Grip,
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
    /// What `check` approved, verbatim — including the second tile of a swap,
    /// which is why the ghosts are drawn from this rather than from the grabbed
    /// tile's delta. A ghost layer computed from the delta cannot show a tile
    /// that is moving the OTHER way.
    moves: Vec<(TileId, Cell)>,
    /// The pointer is off the board entirely.
    ///
    /// THIS COMPONENT HAS TO WORK IT OUT ITSELF, and that is forced rather than
    /// chosen: `begin` captures the pointer on the SVG root, so from that moment
    /// no element outside it — including the host's palette — receives a single
    /// pointer event. "Drag a tile off the board to remove it" can therefore
    /// only mean "release outside the board's own rectangle", which this tests
    /// against the root's bounding box on every move.
    outside: bool,
}

impl Press {
    fn delta(&self, candidate: Cell) -> Axial {
        candidate.to_axial().minus(self.origin.to_axial())
    }
}

/// The one function that turns a verdict into words, so the status line and the
/// aria-live region cannot say different things about the same drop.
///
/// IT NAMES SLUGS, CELLS AND COUNTS, NEVER TILE LABELS. In a pinned diagram
/// there are no tile labels here to name — the host holds them — so a wording
/// that reached for one would be right in one mode and empty in the other. A
/// host that wants prettier words rewrites the sentence from the ids it already
/// knows. Group slugs and member counts ARE in the document in both modes, so
/// naming those breaks nothing.
fn describe(d: &Diagram, grip: &Grip, candidate: Cell, verdict: &Result<Plan, Rejection>) -> Status {
    let Cell { col, row } = candidate;
    match grip {
        // A GROUP DRAG NAMES NO CELL. The cell under the pointer belongs to
        // whichever member happens to be the representative, which is an
        // implementation detail of the grab and means nothing to the reader.
        Grip::Group(g) => {
            let who = who_group(d, g);
            match verdict {
                Ok(_) => Status {
                    text: format!("{who}. They move together."),
                    kind: StatusKind::Info,
                },
                Err(Rejection::Occupied { blocked }) => {
                    let names: Vec<&str> = blocked.iter().map(|(_, id)| id.0.as_str()).collect();
                    Status {
                        text: format!("{who}. Blocked by {}.", join(&names)),
                        kind: StatusKind::Refused,
                    }
                }
                Err(Rejection::NoMove) => Status {
                    text: format!("{who}. Where they already are."),
                    kind: StatusKind::Info,
                },
                Err(other) => Status {
                    text: unknown(other),
                    kind: StatusKind::Refused,
                },
            }
        }
        // A NEW TILE NAMES ITS CELL AND WHAT IT WILL JOIN, and never offers a
        // trade: a tile that is not on the board has nothing to trade WITH, and
        // `check`'s Add arm cannot return an Exchange.
        Grip::New(w) => {
            let who = w.id.0.as_str();
            let joining = w
                .what
                .group()
                .map(|g| format!(", joining {}", g.0.as_str()))
                .unwrap_or_default();
            match verdict {
                Ok(_) => Status {
                    text: format!("Add {who} at column {col} row {row}{joining}."),
                    kind: StatusKind::Info,
                },
                Err(Rejection::Occupied { blocked }) => {
                    let names: Vec<&str> = blocked.iter().map(|(_, id)| id.0.as_str()).collect();
                    Status {
                        text: format!(
                            "{who} cannot go at column {col} row {row}: {} is there.",
                            join(&names)
                        ),
                        kind: StatusKind::Refused,
                    }
                }
                Err(other) => Status {
                    text: unknown(other),
                    kind: StatusKind::Refused,
                },
            }
        }
        Grip::Tile(id) => {
            let who = id.0.as_str();
            match verdict {
                // A SECOND TILE IS ABOUT TO MOVE, AND THE STATUS LINE HAS TO SAY
                // SO. `Warning` rather than `Info` because the drop is being
                // accepted with a consequence the user did not ask for, which is
                // exactly the gap between the two kinds and the reason the enum
                // has three variants rather than two.
                Ok(plan) => match plan.displaced() {
                    Some(other) => Status {
                        text: format!(
                            "{who}, column {col} row {row}. Trades places with {}.",
                            other.0.as_str()
                        ),
                        kind: StatusKind::Warning,
                    },
                    None => Status {
                        text: format!("{who}, column {col} row {row}. Free."),
                        kind: StatusKind::Info,
                    },
                },
                // A TILE DRAG HAS AT MOST ONE BLOCKER, by construction: its
                // moving set is one cell. So this teaches the rule at the moment
                // it bites, rather than reporting a list.
                Err(Rejection::Occupied { blocked }) => {
                    let text = match blocked.as_slice() {
                        [(_, other)] => format!(
                            "{who}, column {col} row {row}. {} is there, and they are not in one \
                             group, so they cannot trade places.",
                            other.0.as_str()
                        ),
                        _ => {
                            let names: Vec<&str> =
                                blocked.iter().map(|(_, id)| id.0.as_str()).collect();
                            format!("{who}, column {col} row {row}. Blocked by {}.", join(&names))
                        }
                    };
                    Status {
                        text,
                        kind: StatusKind::Refused,
                    }
                }
                Err(Rejection::NoMove) => Status {
                    text: format!("{who}, column {col} row {row}. Where it already is."),
                    kind: StatusKind::Info,
                },
                Err(other) => Status {
                    text: unknown(other),
                    kind: StatusKind::Refused,
                },
            }
        }
    }
}

/// "north, 4 tiles" — slug and count, both of which a pinned document carries.
fn who_group(d: &Diagram, g: &GroupId) -> String {
    let n = d.members(g).len();
    format!(
        "{}, {n} {}",
        g.0.as_str(),
        if n == 1 { "tile" } else { "tiles" }
    )
}

/// The two rejections that mean the diagram and the gesture disagree about what
/// exists. Prose rather than `{other:?}`, which leaked a Rust enum into an
/// aria-live region.
fn unknown(r: &Rejection) -> String {
    match r {
        Rejection::UnknownTile(t) => format!("{} is not on this board.", t.0.as_str()),
        Rejection::UnknownGroup(g) => format!("{} is not a group in this diagram.", g.0.as_str()),
        Rejection::AlreadyPlaced(t) => {
            format!("{} is already on this board.", t.0.as_str())
        }
        // Names BOTH modes. "Wrong mode" alone leaves the reader to work out
        // which of the two they have and which they were offered.
        Rejection::WrongMode { diagram, offered } => format!(
            "This diagram is {}, and that tile is {}, so it cannot go here.",
            mode_word(*diagram),
            mode_word(*offered)
        ),
        Rejection::EmptyLabel(t) => {
            format!("{} needs a label before it can go on the board.", t.0.as_str())
        }
        Rejection::LastPlacement => {
            "This is the last tile. A diagram with nothing on it is a file that lost its \
             contents, not a blank board."
                .to_string()
        }
        // Both handled by every caller above; kept total rather than
        // unreachable!() so a new variant is a compile-time nudge, not a panic
        // in somebody's browser.
        other => format!("That move was refused: {other:?}."),
    }
}

/// Where every moving tile will be, as the plan says — or, when the plan is a
/// refusal, where the grabbed set WOULD be.
///
/// THE REFUSED BRANCH IS NOT A FALLBACK, IT IS THE POINT. A ghost that vanishes
/// on an illegal candidate leaves the user dragging nothing, so the refusal has
/// to be drawn AS a refusal, in place, before they release. And the accepted
/// branch has to come from the plan rather than from the delta, because a swap
/// moves a second tile the other way and no arithmetic on the grabbed tile's
/// delta can produce it.
fn preview(
    d: &Diagram,
    p: &Press,
    candidate: Cell,
    verdict: &Result<Plan, Rejection>,
) -> Vec<(TileId, Cell)> {
    // A TILE THAT IS NOT ON THE BOARD IS ITS OWN PREVIEW, accepted or refused.
    // `Plan::moves` cannot produce it — an Add's plan is `Nothing`, because
    // nothing MOVES — and the refused branch below reads `cell_of`, which for a
    // pending tile is None. Both paths would draw no ghost at all, and a palette
    // drag with no ghost is a drag with nothing in your hand.
    if let Grip::New(w) = &p.grip {
        return vec![(w.id.clone(), candidate)];
    }
    let delta = p.delta(candidate);
    match verdict {
        Ok(plan) => plan.moves(d),
        Err(_) => d
            .moving_set(&p.grabbed, p.detach)
            .iter()
            .filter_map(|id| Some((id.clone(), d.cell_of(id)?)))
            .map(|(id, at)| (id, Cell::from_axial(at.to_axial().plus(delta))))
            .collect(),
    }
}

/// Is this client point outside the board's own rectangle?
///
/// Measured against the root element rather than against the frame's cells: a
/// pointer inside the SVG but past the last hexagon is still ON the board, and
/// dropping there is a move to an empty cell, not a removal.
fn off_board(root: &NodeRef, cx: f64, cy: f64) -> bool {
    let Some(el) = root.cast::<web_sys::Element>() else {
        return false;
    };
    let r = el.get_bounding_client_rect();
    cx < r.left() || cx > r.right() || cy < r.top() || cy > r.bottom()
}

fn mode_word(m: Mode) -> &'static str {
    match m {
        Mode::Pinned => "pinned to an external source",
        Mode::Standalone => "self-contained",
    }
}

/// A removal, in words. Separate from `describe` because a removal has no
/// candidate cell to name — the tile is leaving, not arriving somewhere.
fn removal(d: &Diagram, id: &TileId, verdict: &Result<Plan, Rejection>) -> Status {
    match verdict {
        Ok(_) => Status {
            text: match d.group_of(id) {
                Some(g) => format!("Removed {}, out of {}.", id.0.as_str(), g.0.as_str()),
                None => format!("Removed {}.", id.0.as_str()),
            },
            kind: StatusKind::Warning,
        },
        Err(other) => Status {
            text: unknown(other),
            kind: StatusKind::Refused,
        },
    }
}

/// What a grab announces the moment it starts, so a keyboard user knows what
/// they are holding before they move it. There was no announcement here at all.
fn holding(d: &Diagram, grip: &Grip) -> Status {
    let text = match grip {
        // ARMED, NOT HELD. Nothing has been picked up — a cell has to be chosen
        // before anything exists on the board — and saying "holding" would tell
        // a keyboard user they are carrying something they are not.
        Grip::New(w) => match w.what.group() {
            Some(g) => format!(
                "{} is ready to place, joining {}. Choose a cell.",
                w.id.0.as_str(),
                g.0.as_str()
            ),
            None => format!("{} is ready to place. Choose a cell.", w.id.0.as_str()),
        },
        Grip::Group(g) => format!("Holding {}. They move together.", who_group(d, g)),
        Grip::Tile(id) => match d.group_of(id) {
            Some(g) => format!(
                "Holding {}, alone. The rest of {} stays where it is.",
                id.0.as_str(),
                g.0.as_str()
            ),
            None => format!("Holding {}.", id.0.as_str()),
        },
    };
    Status {
        text,
        kind: StatusKind::Info,
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
    // Which group's region currently holds keyboard focus, if any.
    let focused_group = use_state(|| Option::<GroupId>::None);
    // Which group the pointer is over. Hover is the only affordance that can
    // tell someone a region is draggable BEFORE they press it, and with two
    // groups whose grown regions merge it is also the only warning that the
    // press will go to the one they did not mean.
    let hovered_group = use_state(|| Option::<GroupId>::None);
    // Whether the board itself has focus, which is what separates "this tile is
    // selected" from "this tile is where the keyboard is".
    let board_focused = use_state(|| false);

    let d = &props.diagram;
    let l = props.lattice;

    // THE VIEWBOX INCLUDES THE EMPTY RING, and it did not used to.
    //
    // `Frame::around` measures the CONTENT and `viewbox` adds only `pad`, so the
    // ring handed to the frame callback was drawn outside the picture and simply
    // clipped. That was survivable while every legal drop target was a cell some
    // tile already touched. It is not survivable with a palette: the cells a new
    // tile can go in are, by definition, the empty ones — so half of them were
    // off screen, and the first cell an armed keyboard user was offered was the
    // top-left corner of the ring, which is outside the picture on both axes.
    let content = Frame::around(d.cells().map(|(c, _)| c), l, props.pad)
        .expect("a Diagram always holds at least one placement, so a frame around it exists");
    let frame = Frame::around(ring(&content, props.frame_ring.max(0)).into_iter(), l, props.pad)
        .expect("the ring around a non-empty frame is non-empty");

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
            // STATUS FIRST, THEN THE CHANGE, and the order is the whole point.
            // It used to be the other way round, which meant a host that wrote
            // its own sentence in `on_change` — "Moved. Civic Quarter is now in
            // two parts." — had it overwritten a microsecond later by this
            // component's description of the CANDIDATE. The host could never win
            // its own status line and nothing said why. Emitting first makes
            // this the fallback rather than the last word, which is what a
            // component owes a host: say something useful if nobody else does,
            // and get out of the way if somebody does.
            //
            // The aria-live region keeps the component's wording either way,
            // because that region belongs to the component and a host cannot
            // reach it.
            live.set(status.text.clone());
            on_status.emit(status);
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
        }
    };

    // ONE TAIL, TWO DOORS. Everything a press has to do — refuse the wrong
    // button, take pointer capture, announce what is now held, record the press
    // — is identical whether a hexagon or a ground was pressed. Only the grip
    // and the origin differ, so only those are arguments.
    let begin = {
        let press = press.clone();
        let root = root.clone();
        let diagram = d.clone();
        let on_status = props.on_status.clone();
        let live = live.clone();
        let readonly = props.readonly;
        Rc::new(move |grip: Grip, grabbed: TileId, origin: Cell, ev: PointerEvent| {
            // A RIGHT-CLICK IS NOT A DRAG. Without this guard the context menu
            // opens over a board that now believes a press is in flight, and the
            // pointerup that would have ended it goes to the menu.
            if readonly || ev.button() != 0 {
                return;
            }
            ev.prevent_default();
            // AND THEREFORE FOCUS THE BOARD BY HAND. Cancelling the pointerdown
            // also cancels the compatibility mousedown, and with it the focus
            // that mousedown would have moved to the nearest focusable
            // ancestor. Nothing else in this crate calls `focus()`, so without
            // this a user who arrives by clicking never focuses the root — and
            // the root is where `onkeydown` lives, so every keyboard equivalent
            // is unreachable for the rest of the session. The drag-only editor
            // this file argues against at SC 2.1.1 is exactly what a click
            // produced.
            if let Some(el) = root.cast::<web_sys::HtmlElement>() {
                let _ = el.focus();
            }
            // CAPTURE ON THE SVG ROOT, NOT ON THE TARGET. A fast drag that
            // leaves the pressed element loses pointermove otherwise, and the
            // tile freezes in mid-air with the pointer somewhere else. Capturing
            // on the root is also what lets `onpointerleave` go: with capture,
            // the pointer cannot leave, so aliasing leave to up — which COMMITTED
            // a drag whenever the pointer crossed the board's edge, and a group
            // drag starts near that edge far more often than a tile drag — is no
            // longer needed to avoid a stuck press.
            if let Some(el) = root.cast::<web_sys::Element>() {
                let _ = el.set_pointer_capture(ev.pointer_id());
            }
            let status = holding(&diagram, &grip);
            live.set(status.text.clone());
            on_status.emit(status);
            press.set(Some(Press {
                grabbed,
                detach: detach_for(&grip),
                grip,
                origin,
                from_client: (ev.client_x() as f64, ev.client_y() as f64),
                drag: None,
            }));
        })
    };

    let ontiledown = {
        let begin = begin.clone();
        let diagram = d.clone();
        let press = press.clone();
        Callback::from(move |(id, ev): (TileId, PointerEvent)| {
            // A PRESS ON A HEXAGON WHILE SOMETHING IS ARMED IS A DROP, NOT A GRAB.
            // This handler sits on the tile and the board's own sits on the root,
            // so both run and this one runs FIRST — it was overwriting the armed
            // press with a `Grip::Tile` and the root handler then bailed out
            // because the grip was no longer `New`. The user aimed a component at
            // an occupied cell and got "Holding hall" instead of the refusal that
            // names what is in the way, with their chip silently disarmed.
            if matches!((*press).as_ref().map(|p| &p.grip), Some(Grip::New(_))) {
                return;
            }
            let Some(origin) = diagram.cell_of(&id) else {
                return;
            };
            begin(Grip::Tile(id.clone()), id, origin, ev);
        })
    };

    let ongrounddown = {
        let begin = begin.clone();
        let diagram = d.clone();
        let to_user = to_user.clone();
        Callback::from(move |(gid, ev): (GroupId, PointerEvent)| {
            // ANY MEMBER WILL DO AS THE REPRESENTATIVE, because a Translate is a
            // rigid delta: which tile carries the grab changes nothing about
            // where the group lands. What it DOES change is the origin the delta
            // is measured from — so the origin is the cell under the POINTER,
            // not the representative's cell, or the whole cluster jumps by the
            // offset between them on the first pointermove.
            let Some(rep) = diagram.members(&gid).into_iter().next() else {
                return;
            };
            let Some((x, y)) = to_user(ev.client_x() as f64, ev.client_y() as f64) else {
                return;
            };
            let origin = l.cell_at(x - frame.origin_x, y - frame.origin_y);
            begin(Grip::Group(gid), rep, origin, ev);
        })
    };

    // ARMING, AS AN EFFECT ON A CONTROLLED PROP — one seeding site, and then both
    // gestures run through handlers that already exist. The alternative is a
    // second press lifecycle beside the first, which is how the pointer path and
    // the keyboard path come to disagree about what a drop does.
    //
    // `drag: None` deliberately: nothing is painted until the user points
    // somewhere. Seeding a ghost at an arbitrary cell would put a hexagon on the
    // board at a position the user never chose.
    {
        let press = press.clone();
        let diagram = d.clone();
        let live = live.clone();
        let readonly = props.readonly;
        let on_status = props.on_status.clone();
        use_effect_with(props.pending.clone(), move |pending: &Option<Pending>| {
            match (pending, (*press).clone()) {
                // READONLY GATES THE PALETTE TOO. It was checked in `begin`, in
                // the keyboard grab and in the removal branch, and in none of the
                // three places on this path — so a board rendered `readonly` with
                // a chip armed could still be added to, which is an ordinary
                // combination for a page that shows a drawer it does not mean to
                // be editable.
                (Some(_), _) if readonly => {}
                (Some(w), None) => {
                    let grip = Grip::New(w.clone());
                    let status = holding(&diagram, &grip);
                    live.set(status.text.clone());
                    on_status.emit(status);
                    press.set(Some(Press {
                        grabbed: w.id.clone(),
                        detach: detach_for(&grip),
                        grip,
                        // Never read for a New grip: `command_for` takes the
                        // candidate, not a delta from here.
                        origin: Cell { col: 0, row: 0 },
                        from_client: (0.0, 0.0),
                        drag: None,
                    }));
                }
                // The host disarmed it (a second click on the chip, Escape in
                // the drawer): drop the press the arming created, and nothing
                // else — a press from a real pointer is not ours to cancel.
                (None, Some(held)) if matches!(held.grip, Grip::New(_)) => press.set(None),
                _ => {}
            }
            || ()
        });
    }

    let onpointermove = {
        let press = press.clone();
        let diagram = d.clone();
        let root = root.clone();
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
            let verdict = diagram.check(&command_for(&p, candidate));
            let next = Drag {
                candidate,
                blocked: match &verdict {
                    Err(Rejection::Occupied { blocked }) => blocked.clone(),
                    _ => Vec::new(),
                },
                moves: preview(&diagram, &p, candidate, &verdict),
                outside: off_board(&root, cx, cy),
            };
            // One comparison over the whole Drag, rather than one term per
            // field: a field added later is then covered by construction instead
            // of being silently left out of the change test.
            if p.drag.as_ref() == Some(&next) {
                return;
            }
            let status = describe(&diagram, &p.grip, candidate, &verdict);
            live.set(status.text.clone());
            on_status.emit(status);
            press.set(Some(Press {
                drag: Some(next),
                ..p
            }));
        })
    };

    let onpointerup = {
        let press = press.clone();
        let diagram = d.clone();
        let selected = selected.clone();
        let on_select = props.on_select.clone();
        let on_status = props.on_status.clone();
        let on_pending = props.on_pending.clone();
        let live = live.clone();
        let commit = commit.clone();
        let removable = props.removable;
        Callback::from(move |_: PointerEvent| {
            let Some(p) = (*press).clone() else { return };
            press.set(None);
            let Some(drag) = p.drag.clone() else {
                // Under the slop: a press is a selection, and firing an edit
                // here would put an undo entry on the stack for every click.
                match &p.grip {
                    Grip::Tile(id) => {
                        selected.set(Some(id.clone()));
                        on_select.emit(Some(id.clone()));
                    }
                    // A CLICK ON A GROUND SELECTS NOTHING. `on_select` carries a
                    // TileId, so the only thing it could report is the arbitrary
                    // representative — a tile the user never pointed at. Saying
                    // what was pressed is more useful than naming the wrong tile.
                    Grip::Group(_) => {
                        let status = holding(&diagram, &p.grip);
                        live.set(status.text.clone());
                        on_status.emit(status);
                    }
                    // An armed chip released without ever pointing at the board
                    // stays armed: the user tapped it and has not chosen a cell.
                    Grip::New(_) => {
                        press.set(Some(p.clone()));
                    }
                }
                return;
            };

            // RELEASED OFF THE BOARD. For a tile already on it, that is a
            // removal; for one that never arrived, it is a cancellation.
            if drag.outside {
                match &p.grip {
                    Grip::New(_) => {
                        let status = Status {
                            text: "Not placed.".to_string(),
                            kind: StatusKind::Info,
                        };
                        live.set(status.text.clone());
                        on_status.emit(status);
                        on_pending.emit(PendingEnd::Cancelled);
                    }
                    Grip::Tile(id) if removable => {
                        let cmd = Command::Remove { tile: id.clone() };
                        let status = removal(&diagram, id, &diagram.check(&cmd));
                        // THE SAME TWO LINES THE KEYBOARD REMOVAL RUNS. Without
                        // them the selection still names a tile that is gone, and
                        // the next Delete reports "… is not on this board" about a
                        // tile the user has just removed with the mouse. The
                        // handler's own doc says the two paths cannot drift.
                        if diagram.check(&cmd).is_ok() {
                            selected.set(None);
                            on_select.emit(None);
                        }
                        commit(cmd, status);
                    }
                    _ => {
                        let status = Status {
                            text: "Dropped off the board. Nothing moved.".to_string(),
                            kind: StatusKind::Info,
                        };
                        live.set(status.text.clone());
                        on_status.emit(status);
                    }
                }
                return;
            }

            let cmd = command_for(&p, drag.candidate);
            let verdict = diagram.check(&cmd);
            let status = describe(&diagram, &p.grip, drag.candidate, &verdict);
            match (&p.grip, &verdict) {
                // NoMove is neither a change nor a refusal: the host turns it
                // into a selection rather than flashing an error.
                (Grip::Tile(id), Err(Rejection::NoMove)) => {
                    selected.set(Some(id.clone()));
                    on_select.emit(Some(id.clone()));
                }
                (Grip::Group(_), Err(Rejection::NoMove)) => {
                    live.set(status.text.clone());
                    on_status.emit(status);
                }
                // A REFUSED ADD LEAVES THE CHIP ARMED. Disarming it would make a
                // mis-aimed drop cost the user their selection as well as their
                // drop, and the refusal already says what was in the way.
                (Grip::New(_), Err(_)) => {
                    live.set(status.text.clone());
                    on_status.emit(status);
                    press.set(Some(Press { drag: None, ..p.clone() }));
                }
                (Grip::New(_), Ok(_)) => {
                    commit(cmd, status);
                    on_pending.emit(PendingEnd::Placed);
                }
                _ => commit(cmd, status),
            }
        })
    };

    let onboarddown = {
        let press = press.clone();
        let diagram = d.clone();
        let readonly = props.readonly;
        let root = root.clone();
        let to_user = to_user.clone();
        let on_status = props.on_status.clone();
        let live = live.clone();
        Callback::from(move |ev: PointerEvent| {
            let Some(p) = (*press).clone() else { return };
            if readonly || !matches!(p.grip, Grip::New(_)) || p.drag.is_some() {
                return;
            }
            if let Some(el) = root.cast::<web_sys::Element>() {
                let _ = el.set_pointer_capture(ev.pointer_id());
            }
            let (cx, cy) = (ev.client_x() as f64, ev.client_y() as f64);
            let Some((x, y)) = to_user(cx, cy) else { return };
            let candidate = l.cell_at(x - frame.origin_x, y - frame.origin_y);
            let verdict = diagram.check(&command_for(&p, candidate));
            let status = describe(&diagram, &p.grip, candidate, &verdict);
            live.set(status.text.clone());
            on_status.emit(status);
            press.set(Some(Press {
                from_client: (cx, cy),
                drag: Some(Drag {
                    candidate,
                    blocked: match &verdict {
                        Err(Rejection::Occupied { blocked }) => blocked.clone(),
                        _ => Vec::new(),
                    },
                    moves: preview(&diagram, &p, candidate, &verdict),
                    outside: false,
                }),
                ..p
            }));
        })
    };

    let oncancel = {
        let press = press.clone();
        Callback::from(move |_: PointerEvent| press.set(None))
    };

    // The keyboard path runs the IDENTICAL check/apply as the pointer path,
    // through `command_for` and `commit`, so the two cannot drift about what a
    // drop DOES. They drifted twice anyway about what a drop leaves behind —
    // three sites building their own `Translate`, and a pointer removal that did
    // not clear the selection the keyboard one cleared — so the claim is worth
    // only as much as the tests under it. A drag-only editor fails WCAG 2.1
    // SC 2.1.1 outright, which is why there is a keyboard path at all.
    let onkeydown = {
        let press = press.clone();
        let selected = selected.clone();
        let focused_group = focused_group.clone();
        let diagram = d.clone();
        let on_pending = props.on_pending.clone();
        let removable = props.removable;
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
                if let Some(p) = &held {
                    ev.prevent_default();
                    // A cancelled ARMING has to be reported, or the host's chip
                    // stays pressed for a placement that will never happen.
                    if matches!(p.grip, Grip::New(_)) {
                        on_pending.emit(PendingEnd::Cancelled);
                    }
                    press.set(None);
                    // ANNOUNCED, because cancelling is the one action whose
                    // whole effect is that nothing happened. Silence here is
                    // indistinguishable from the key not working.
                    let status = Status {
                        text: "Cancelled. Nothing moved.".to_string(),
                        kind: StatusKind::Info,
                    };
                    live.set(status.text.clone());
                    on_status.emit(status);
                }
                return;
            }

            if let Some(p) = held {
                // AN ARMED CHIP HAS NO CANDIDATE UNTIL SOMETHING CHOOSES ONE, and
                // for a keyboard user nothing has pointed anywhere. The first
                // arrow seeds it — at the first free cell in reading order
                // INSIDE THE FRAME, which is on screen because the frame now
                // includes the ring. Seeding at the ring's bounding-box corner,
                // which is what `ring()` returns first, put the ghost outside the
                // picture on both axes every single time.
                if p.drag.is_none() && matches!(p.grip, Grip::New(_)) {
                    if !matches!(key.as_str(), "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown")
                    {
                        return;
                    }
                    ev.prevent_default();
                    let Some(seed) = first_free(&diagram, &frame) else { return };
                    let verdict = diagram.check(&command_for(&p, seed));
                    let status = describe(&diagram, &p.grip, seed, &verdict);
                    live.set(status.text.clone());
                    on_status.emit(status);
                    press.set(Some(Press {
                        drag: Some(Drag {
                            candidate: seed,
                            blocked: Vec::new(),
                            moves: preview(&diagram, &p, seed, &verdict),
                            outside: false,
                        }),
                        ..p
                    }));
                    return;
                }
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
                        let cmd = command_for(&p, drag.candidate);
                        let verdict = diagram.check(&cmd);
                        let status = describe(&diagram, &p.grip, drag.candidate, &verdict);
                        press.set(None);
                        if matches!(p.grip, Grip::New(_)) {
                            if verdict.is_ok() {
                                commit(cmd, status);
                                on_pending.emit(PendingEnd::Placed);
                            } else {
                                live.set(status.text.clone());
                                on_status.emit(status);
                            }
                            return;
                        }
                        match verdict {
                            // DROPPING WHERE IT STARTED IS NOT NOTHING TO SAY.
                            // This branch used to compute the status and discard
                            // it, so ending a keyboard grab in place was
                            // completely silent and the live region still
                            // announced the grab — leaving a reader holding a
                            // tile they had already put down.
                            Err(Rejection::NoMove) => {
                                live.set(status.text.clone());
                                on_status.emit(status);
                            }
                            _ => commit(cmd, status),
                        }
                        return;
                    }
                    _ => return,
                };
                ev.prevent_default();
                let candidate = Cell::from_axial(drag.candidate.to_axial().plus(step));
                let verdict = diagram.check(&command_for(&p, candidate));
                let status = describe(&diagram, &p.grip, candidate, &verdict);
                live.set(status.text.clone());
                on_status.emit(status);
                let next = Drag {
                    candidate,
                    blocked: match &verdict {
                        Err(Rejection::Occupied { blocked }) => blocked.clone(),
                        _ => Vec::new(),
                    },
                    moves: preview(&diagram, &p, candidate, &verdict),
                    // The keyboard never leaves the board.
                    outside: false,
                };
                press.set(Some(Press {
                    drag: Some(next),
                    ..p
                }));
                return;
            }

            // TAKING A TILE OFF THE BOARD FROM THE KEYBOARD, which is the
            // equivalent of dragging it clear — and the only equivalent, since a
            // keyboard cannot leave the board's rectangle.
            //
            // Both keys: Delete is the ordinary one and Backspace is what a
            // laptop without a Delete key has. Gated on `removable` AND on a
            // selection, so a diagram with no palette is unchanged.
            if matches!(key.as_str(), "Delete" | "Backspace") {
                // THE SAME GUARD THE ARROWS GET, and this key needs it more. The
                // argument below for the arrows — "a highlight the user cannot
                // see, on a thing they are not pointing at" — is worse for a
                // destructive key: with the focus ring on a group's region,
                // Delete was removing whichever tile had last been clicked.
                if !removable || readonly || focused_group.is_some() {
                    return;
                }
                let Some(id) = (*selected).clone() else { return };
                ev.prevent_default();
                let cmd = Command::Remove { tile: id.clone() };
                let verdict = diagram.check(&cmd);
                let status = removal(&diagram, &id, &verdict);
                match verdict {
                    Ok(_) => {
                        selected.set(None);
                        on_select.emit(None);
                        commit(cmd, status);
                    }
                    Err(_) => {
                        live.set(status.text.clone());
                        on_status.emit(status);
                    }
                }
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
            // A FOCUSED GROUP OWNS THE ARROWS ONLY ONCE IT IS HELD. Roving the
            // tile selection while the focus ring is on a group region would
            // move a highlight the user cannot see from a thing they are not
            // pointing at.
            if focused_group.is_some() && !matches!(key.as_str(), " " | "Enter") {
                return;
            }
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
                    // THE KEYBOARD EQUIVALENT OF PRESSING A GROUND IS FOCUSING
                    // ONE. Each group's region is a real tab stop with a real
                    // accessible name (see `grounds`), so "grab what is focused"
                    // is the same sentence for both gestures and needs no
                    // modifier — which matters, because the modifier this
                    // component used to rely on is the one the window manager
                    // takes.
                    let grip = match (*focused_group).clone() {
                        Some(g) => Grip::Group(g),
                        None => match (*selected).clone() {
                            Some(id) => Grip::Tile(id),
                            None => return,
                        },
                    };
                    let Some(grabbed) = (match &grip {
                        Grip::Tile(id) => Some(id.clone()),
                        Grip::Group(g) => diagram.members(g).into_iter().next(),
                        // Unreachable: an armed chip is seeded by the effect
                        // below, never by this branch, so `grip` here is never
                        // `New`. Total rather than unreachable!().
                        Grip::New(w) => Some(w.id.clone()),
                    }) else {
                        return;
                    };
                    let Some(origin) = diagram.cell_of(&grabbed) else {
                        return;
                    };
                    ev.prevent_default();
                    let status = holding(&diagram, &grip);
                    live.set(status.text.clone());
                    on_status.emit(status);
                    let (grabbed_for_preview, grip_for_preview) = (grabbed.clone(), grip.clone());
                    press.set(Some(Press {
                        grabbed,
                        detach: detach_for(&grip),
                        grip,
                        origin,
                        from_client: (0.0, 0.0),
                        // SEEDED WITH THE MOVING SET WHERE IT ALREADY IS, not
                        // empty. The ghost layer is drawn from `Drag.moves`, and
                        // `moving` is non-empty the moment a drag exists — so an
                        // empty `moves` paints every grabbed tile at the
                        // `Dragging` opacity with nothing on top of it. Pressing
                        // Space made the tile fade out and put no ghost anywhere:
                        // a keyboard user's first impression of the grab was the
                        // thing they grabbed disappearing.
                        drag: Some(Drag {
                            candidate: origin,
                            outside: false,
                            blocked: Vec::new(),
                            moves: diagram
                                .moving_set(&grabbed_for_preview, detach_for(&grip_for_preview))
                                .iter()
                                .filter_map(|id| Some((id.clone(), diagram.cell_of(id)?)))
                                .collect(),
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

    // THE ARRANGEMENT THE DRAG IS PROPOSING, which is not the one in the
    // diagram. Everything painted below reads from here, so the ghosts, the
    // group regions and the fracture count all describe the same board — the
    // one the user is about to get.
    let plan_moves: Vec<(TileId, Cell)> = p
        .as_ref()
        .and_then(|p| p.drag.as_ref())
        .map(|dr| dr.moves.clone())
        .unwrap_or_default();
    let preview_cells: BTreeMap<TileId, Cell> = plan_moves.iter().cloned().collect();
    // A tile the plan moves that the user did not grab: the other half of a
    // swap. Painted as its own state because "a second hexagon moved" is the
    // moment a user decides the editor is broken.
    let displaced: BTreeSet<TileId> = plan_moves
        .iter()
        .map(|(id, _)| id)
        .filter(|id| !moving.contains(*id))
        .cloned()
        .collect();
    let held_group = p.as_ref().and_then(|p| match &p.grip {
        Grip::Group(g) => Some(g.clone()),
        Grip::Tile(_) | Grip::New(_) => None,
    });

    let (vx, vy, vw, vh) = frame.viewbox(l);

    let state_of = |id: &TileId| -> TileState {
        if blocked_ids.contains(id) {
            TileState::Blocking
        } else if displaced.contains(id) {
            TileState::Displacing
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
            focused: *board_focused && !ghost && selected.as_ref() == Some(id),
        });
        let handler = {
            let cb = ontiledown.clone();
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
                    matches!(state, TileState::Displacing).then_some("is-displacing"),
                )}
                data-tile={id.0.as_str().to_string()}
                transform={format!("translate({x:.3} {y:.3})")}
                // INLINE, NOT IN A STYLESHEET. This crate's markup reaches hosts
                // that do not serve its demo's CSS — one of them shipped a board
                // of black hexagons for exactly that reason — so the two
                // properties without which a drag is not a drag travel with the
                // element: the grab affordance, and the touch-action that stops
                // a finger drag scrolling the page instead.
                style={(!ghost).then_some("cursor:grab;touch-action:none")}
                // A GHOST IS SCENERY. Without this it sits under the pointer and
                // takes the press that should reach the tile beneath it, and a
                // screen reader reads every moving tile twice.
                pointer-events={ghost.then_some("none")}
                aria-hidden={ghost.then_some("true")}
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
            // touch-action inline for the same reason as on the tiles: without
            // it a touch drag scrolls the page and the board never sees the
            // move, on any host that does not happen to have the demo's CSS.
            style="touch-action:none"
            // A TAP IS DOWN-THEN-UP WITH NO MOVE, so an armed chip placed by
            // touch produces no pointermove and therefore no candidate at all.
            // Without this the accessible gesture is mouse-only, which is the
            // opposite of why it exists.
            onpointerdown={onboarddown}
            onpointermove={onpointermove}
            onpointerup={onpointerup}
            onpointercancel={oncancel}
            // NO `onpointerleave`. It used to be aliased to `onpointerup`, which
            // COMMITTED a drag whenever the pointer crossed the board's edge —
            // and a board is routinely inside a narrow horizontal scroller, so
            // the edge is close. Pointer capture on the root (see `begin`) means
            // the pointer cannot leave while a press is live, so the alias was
            // buying nothing and costing an unintended drop.
            // CLEARING `focused_group` HERE IS NOT BELT AND BRACES, IT IS THE ONLY
            // PLACE IT CAN HAPPEN. `focus` and `blur` do not bubble, and yew
            // dispatches a non-bubbling event to its target alone — so a group
            // wrapper losing focus never reaches this root. Without this line,
            // one Tab through a group region leaves `focused_group` set for the
            // rest of the session, and the guard in the key handler then
            // swallows every arrow, Home and End with no announcement: the
            // roving selection is simply dead and nothing says why.
            //
            // The root only receives `focus` when the root ITSELF is focused, so
            // any group recorded at that moment is stale by definition.
            onfocus={ {
                let board_focused = board_focused.clone();
                let focused_group = focused_group.clone();
                Callback::from(move |_: FocusEvent| {
                    board_focused.set(true);
                    focused_group.set(None);
                })
            } }
            onblur={ {
                let board_focused = board_focused.clone();
                let focused_group = focused_group.clone();
                Callback::from(move |_: FocusEvent| {
                    board_focused.set(false);
                    focused_group.set(None);
                })
            } }
            onkeydown={onkeydown}
        >
            // NO ANIMATION ANYWHERE — the ghost snaps and nothing tweens — so
            // prefers-reduced-motion needs no branch. Said here so a reader is
            // not left wondering where it went.
            { props.frame.as_ref().map(|cb| cb.emit(FrameView {
                frame,
                cells: ring(&content, props.frame_ring.max(0)),
                armed: matches!(p.as_ref().map(|p| &p.grip), Some(Grip::New(_))),
            })).unwrap_or_default() }

            // THE GROUP LAYER, AND THE COMPONENT OWNS THE WRAPPER. It used to
            // splice the host's markup straight in, which was fine while a
            // ground was decoration. It is now a press target, so the wrapper —
            // the handlers, the hit pad, the role, the name, the tab stop — is
            // this crate's responsibility, and the host keeps drawing only
            // pixels.
            //
            // EMITTED EVEN WHEN `props.ground` IS NONE, deliberately. That prop
            // is documented as optional decoration; hanging the only group
            // gesture off it would mean a host that draws no regions silently
            // loses the ability to move a cluster at all, with nothing to
            // explain why. The invisible hit pad is the gesture; the host's
            // markup is the picture.
            { for group_views(d, l, frame, &touching, &preview_cells,
                              (*hovered_group).as_ref(), held_group.as_ref())
                .into_iter().map(|v| {
                    let gid = v.id.clone();
                    let n = v.cells.len();
                    let name = format!(
                        "{}, {n} {}. Press space to move {} together.",
                        v.group.label,
                        if n == 1 { "tile" } else { "tiles" },
                        if n == 1 { "it" } else { "them" }
                    );
                    let down = {
                        let cb = ongrounddown.clone();
                        let gid = gid.clone();
                        Callback::from(move |ev: PointerEvent| cb.emit((gid.clone(), ev)))
                    };
                    let enter = {
                        let hovered = hovered_group.clone();
                        let gid = gid.clone();
                        Callback::from(move |_: PointerEvent| hovered.set(Some(gid.clone())))
                    };
                    let leave = {
                        let hovered = hovered_group.clone();
                        Callback::from(move |_: PointerEvent| hovered.set(None))
                    };
                    let gain = {
                        let focused = focused_group.clone();
                        let gid = gid.clone();
                        Callback::from(move |_: FocusEvent| focused.set(Some(gid.clone())))
                    };
                    let pads: Vec<String> = v.paths.clone();
                    let inner = props.ground.as_ref().map(|cb| cb.emit(v)).unwrap_or_default();
                    html! {
                        <g class="hc-group"
                           data-group={gid.0.as_str().to_string()}
                           role="button"
                           tabindex="0"
                           aria-label={name}
                           style="cursor:grab;touch-action:none"
                           onpointerdown={down}
                           onpointerenter={enter}
                           onpointerleave={leave}
                           onfocus={gain}
                        >
                            // THE HIT PAD, UNDER THE HOST'S MARKUP AND ABOVE
                            // NOTHING. `fill="none"` would not be pressable and
                            // a visible fill would paint over the host's own, so
                            // it is filled with transparency and told to take
                            // events anyway. Drawn first so the host's region
                            // sits on top of it and the tiles on top of that —
                            // which is what makes a press on a hexagon reach the
                            // hexagon rather than the cluster under it.
                            { for pads.iter().map(|dpath| html! {
                                <path d={dpath.clone()} fill="transparent" stroke="none"
                                      pointer-events="all" />
                            }) }
                            { inner }
                        </g>
                    }
                }) }

            { for d.cells().map(|(cell, id)| draw(id, cell, state_of(id), false)) }

            // GHOSTS FROM THE PLAN, NOT FROM THE DELTA. A swap sends the second
            // tile the OTHER way, and no arithmetic on the grabbed tile's delta
            // can produce that — so before this the accepted preview of a swap
            // showed one tile arriving and nothing leaving.
            { for plan_moves.iter().map(|(id, to)| {
                let state = if refused { TileState::GhostRefused } else { TileState::Ghost };
                draw(id, *to, state, true)
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

/// The first cell in the frame that nothing occupies, in reading order.
///
/// IN THE FRAME, NOT IN `ring()`'s RETURN. `ring` hands back the whole bounding
/// rectangle grown by n — occupied cells included — so scanning it for the first
/// free cell always answers with its top-left corner, which is the furthest
/// point from anything the user is looking at.
fn first_free(d: &Diagram, f: &Frame) -> Option<Cell> {
    for row in f.min.row..=f.max.row {
        for col in f.min.col..=f.max.col {
            let c = Cell { col, row };
            if d.at(c).is_none() {
                return Some(c);
            }
        }
    }
    None
}

/// Every cell in the bounding box grown by `n` rings — OCCUPIED CELLS INCLUDED.
///
/// The name says ring and the doc used to say "the empty cells n rings beyond
/// the content", and neither is true: it is a rectangle, and it contains the
/// content. That is what the frame callback wants (a host draws the lattice
/// behind everything, not a halo around it) but it is a trap for anything that
/// reads it looking for a free cell.
fn ring(f: &Frame, n: i32) -> Vec<Cell> {
    let mut out = Vec::new();
    for row in (f.min.row - n)..=(f.max.row + n) {
        for col in (f.min.col - n)..=(f.max.col + n) {
            out.push(Cell { col, row });
        }
    }
    out
}

/// One [`GroupView`] per group, over the arrangement CURRENTLY ON SCREEN.
///
/// `preview` is the plan's moves, so during a drag every region, anchor and
/// piece count describes where the tiles are going rather than where they still
/// are. That matters much more than it used to: now that a plain tile drag
/// detaches, pulling a member out of a cluster is the ordinary gesture, and a
/// group that only admits it has split AFTER the drop springs the news.
fn group_views(
    d: &Diagram,
    l: Lattice,
    f: Frame,
    touching: &BTreeSet<GroupId>,
    preview: &BTreeMap<TileId, Cell>,
    hovered: Option<&GroupId>,
    held: Option<&GroupId>,
) -> Vec<GroupView> {
    let mut by_group: BTreeMap<GroupId, Vec<Cell>> = BTreeMap::new();
    for (cell, id) in d.cells() {
        if let Some(g) = d.group_of(id) {
            // The previewed cell when the plan moves this tile, its own
            // otherwise. One lookup, and it is the whole of "the region follows
            // the drag".
            let shown = preview.get(id).copied().unwrap_or(cell);
            by_group.entry(g.clone()).or_default().push(shown);
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
            // Above the topmost row, centred on the part of it this group
            // occupies — not above the centroid, which for anything two rows
            // deep is inside the group.
            let top_row = cells.iter().map(|c| c.row).min().unwrap_or(0);
            let top: Vec<&Cell> = cells.iter().filter(|c| c.row == top_row).collect();
            let tn = top.len().max(1) as f64;
            let tx = top.iter().map(|c| f.at(**c, l).0).sum::<f64>() / tn;
            let ty = top
                .iter()
                .map(|c| f.at(**c, l).1)
                .fold(f64::INFINITY, f64::min);
            let heading = (tx, ty - l.h() / 2.0 - 10.0);
            // Over the SHOWN cells, through core's own flood fill — not a second
            // copy of it here, and not `Diagram::group_components`, which can
            // only ever answer about the committed arrangement.
            let pieces = honeycomb_core::components(cells.iter().copied().collect()).len();
            Some(GroupView {
                touching: touching.contains(&id),
                hovered: hovered == Some(&id),
                grabbed: held == Some(&id),
                pieces,
                id,
                group,
                paths,
                anchor: (sx / n, sy / n),
                heading,
                cells,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- tests

/// THE GESTURE RULE AND THE WORDING, ON THE HOST TARGET.
///
/// This crate had no tests at all, for an understandable reason: everything in
/// it is a browser. But the two things this feature most needs pinned are not —
/// `detach_for` is a total function over an enum, and `describe` is a pure
/// function of a diagram, a grip, a cell and a verdict. `demo-ssg` already
/// proves the crate compiles and runs on the host through `ServerRenderer`, so
/// there is nothing exotic about testing them here.
///
/// What is still uncovered, and worth saying rather than implying: nothing
/// asserts which `Grip` a pointerdown constructs. That is why `detach_for`
/// exists as a function at all — it makes the handlers short enough to read.
#[cfg(test)]
mod tests {
    use super::*;
    use honeycomb_core::{
        Content, DiagramSpec, Group, Iri, LatticeConvention, PinnedTile, Slug,
    };
    use std::collections::BTreeMap;

    fn tid(s: &str) -> TileId {
        TileId(Slug::parse(s).unwrap())
    }
    fn gid(s: &str) -> GroupId {
        GroupId(Slug::parse(s).unwrap())
    }

    /// ana and bea in "north", eve alone in "south".
    fn fixture() -> Diagram {
        let mut groups = BTreeMap::new();
        for g in ["north", "south"] {
            groups.insert(
                gid(g),
                Group {
                    label: format!("The {g}"),
                    style_key: None,
                    note: None,
                    extra: Vec::new(),
                },
            );
        }
        let mut tiles = BTreeMap::new();
        let mut cells = BTreeMap::new();
        for (name, col, row, g) in [
            ("ana", 0, 0, "north"),
            ("bea", 1, 0, "north"),
            ("eve", 2, 0, "south"),
        ] {
            tiles.insert(
                tid(name),
                PinnedTile {
                    group: Some(gid(g)),
                    represents: Iri(format!("https://example.org/{name}")),
                },
            );
            cells.insert(tid(name), Cell { col, row });
        }
        Diagram::try_new(DiagramSpec {
            slug: Slug::parse("fixture").unwrap(),
            label: "Fixture".into(),
            note: None,
            convention: LatticeConvention::OddRPointyTop,
            generator: None,
            generated_at: None,
            groups,
            content: Content::Pinned {
                source: Iri("https://example.org/source".into()),
                revision: None,
                tiles,
            },
            cells,
            extra: Vec::new(),
        })
        .unwrap()
    }

    /// The gesture, stated once: a hexagon drags alone, a ground drags the
    /// cluster. If this flips, every sentence on both demo pages is a lie.
    #[test]
    fn a_tile_grip_detaches_and_a_group_grip_does_not() {
        assert!(detach_for(&Grip::Tile(tid("ana"))));
        assert!(!detach_for(&Grip::Group(gid("north"))));
    }

    fn pending(id: &str, group: Option<&str>) -> Pending {
        Pending {
            id: tid(id),
            what: NewTile::Pinned(PinnedTile {
                group: group.map(gid),
                represents: Iri(format!("https://example.org/{id}")),
            }),
        }
    }

    fn press_for(grip: Grip) -> Press {
        Press {
            grabbed: match &grip {
                Grip::Tile(id) => id.clone(),
                Grip::New(w) => w.id.clone(),
                Grip::Group(_) => tid("ana"),
            },
            detach: detach_for(&grip),
            grip,
            origin: Cell { col: 0, row: 0 },
            from_client: (0.0, 0.0),
            drag: None,
        }
    }

    /// `Pending::at` is the only constructor of an Add anywhere, and this is
    /// what makes "the palette armed X and the board added Y" unrepresentable
    /// rather than merely untested.
    #[test]
    fn a_new_grip_detaches_and_aims_an_add_at_the_candidate() {
        let w = pending("hal", Some("north"));
        assert!(detach_for(&Grip::New(w.clone())));
        let p = press_for(Grip::New(w.clone()));
        let at = Cell { col: 5, row: 3 };
        assert_eq!(command_for(&p, at), w.at(at));
        match command_for(&p, at) {
            Command::Add { tile, at: c, what } => {
                assert_eq!(tile, tid("hal"));
                assert_eq!(c, at);
                assert_eq!(what.group(), Some(&gid("north")));
            }
            other => panic!("an armed chip must aim an Add, got {other:?}"),
        }
    }

    /// A tile that is not on the board has nothing to trade WITH, and `check`'s
    /// Add arm cannot return an Exchange — so the word must never appear.
    #[test]
    fn describe_names_the_cell_and_the_group_a_new_tile_will_join_and_never_offers_a_swap() {
        let d = fixture();
        let w = pending("hal", Some("north"));
        let grip = Grip::New(w.clone());

        let free = Cell { col: 6, row: 6 };
        let s = describe(&d, &grip, free, &d.check(&w.at(free)));
        assert_eq!(s.kind, StatusKind::Info);
        assert!(s.text.contains("hal") && s.text.contains("north"), "{}", s.text);
        assert!(s.text.contains("column 6 row 6"), "{}", s.text);

        let taken = Cell { col: 1, row: 0 };
        let s = describe(&d, &grip, taken, &d.check(&w.at(taken)));
        assert_eq!(s.kind, StatusKind::Refused);
        assert!(s.text.contains("bea"), "it names the blocker: {}", s.text);
        assert!(
            !s.text.contains("trade places"),
            "a tile not on the board cannot trade places: {}",
            s.text
        );

        // Armed, before any cell is chosen.
        let h = holding(&d, &grip);
        assert!(h.text.contains("ready to place"), "{}", h.text);
        assert!(!h.text.contains("Holding"), "nothing is held yet: {}", h.text);
    }

    /// The catch-all arm formats `{other:?}`, which would put a Rust enum into an
    /// aria-live region. Every rejection a palette can reach must be prose.
    #[test]
    fn every_rejection_has_prose_and_none_leaks_its_debug() {
        for r in [
            Rejection::AlreadyPlaced(tid("ana")),
            Rejection::WrongMode {
                diagram: Mode::Pinned,
                offered: Mode::Standalone,
            },
            Rejection::EmptyLabel(tid("hal")),
            Rejection::LastPlacement,
            Rejection::UnknownTile(tid("nobody")),
            Rejection::UnknownGroup(gid("nowhere")),
        ] {
            let text = unknown(&r);
            assert!(!text.is_empty(), "{r:?} has no prose");
            assert!(!text.contains('{'), "{r:?} leaked a struct: {text}");
            assert!(
                !text.contains("Rejection"),
                "{r:?} leaked its type name: {text}"
            );
        }
    }

    /// EVERY SITE THAT TURNS A PRESS INTO A COMMAND GOES THROUGH `command_for`.
    ///
    /// A GREP, AND IT EARNED ITS PLACE. `command_for` was introduced with a doc
    /// comment explaining that hand-rolled `Translate`s at four call sites were
    /// the thing it existed to stop — and three of the five sites kept building
    /// their own anyway. For a `Grip::New` the grabbed tile is not on the board,
    /// so those `Translate`s hit `check`'s first line and were refused
    /// `UnknownTile` unconditionally: dragging a palette chip narrated "… is not
    /// on this board" on every pointer move, drew its ghost at full strength over
    /// occupied cells, and placing with the keyboard did not work at all. None of
    /// that is reachable from a pure function, so nothing else here could catch
    /// it.
    #[test]
    fn every_press_site_uses_command_for() {
        let src = include_str!("lib.rs");
        // The component body only: not this module, and not `command_for`
        // itself, which is the one place the constructor is allowed to appear.
        let body = src.split("#[cfg(test)]").next().unwrap_or(src);
        let needle = concat!("Command", "::Translate {");
        let start = body.find("fn command_for").unwrap_or(0);
        let end = body[start..].find("\n}\n").map(|i| start + i).unwrap_or(start);
        let offenders: Vec<(usize, &str)> = body
            .lines()
            .enumerate()
            .filter(|(_, l)| l.contains(needle) && !l.trim_start().starts_with("//"))
            .filter(|(_, l)| {
                let at = body.find(*l).unwrap_or(0);
                at < start || at > end
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "a press site builds its own Translate instead of calling command_for, which is              always refused for an armed palette tile: {offenders:?}"
        );
    }

    /// Taking a tile off the board is an ACCEPTED change with a consequence, so
    /// it is a Warning — and it names what the tile was part of, because that is
    /// the part the user cannot see once it is gone.
    #[test]
    fn a_removal_names_what_the_tile_was_part_of() {
        let d = fixture();
        let s = removal(&d, &tid("ana"), &d.check(&Command::Remove { tile: tid("ana") }));
        assert_eq!(s.kind, StatusKind::Warning);
        assert!(s.text.contains("ana") && s.text.contains("north"), "{}", s.text);
    }

    /// The status line must never call a swap "Free.". A second hexagon moving
    /// is the moment a user decides the editor has malfunctioned, and the only
    /// thing that prevents it is this sentence arriving before they release.
    #[test]
    fn describe_never_says_free_when_a_second_tile_is_about_to_move() {
        let d = fixture();
        let onto_bea = Command::Translate {
            grabbed: tid("ana"),
            delta: Cell { col: 1, row: 0 }
                .to_axial()
                .minus(Cell { col: 0, row: 0 }.to_axial()),
            detach: true,
        };
        let verdict = d.check(&onto_bea);
        let s = describe(&d, &Grip::Tile(tid("ana")), Cell { col: 1, row: 0 }, &verdict);
        assert_eq!(s.kind, StatusKind::Warning, "an accepted swap is a warning");
        assert!(s.text.contains("bea"), "it names the other tile: {}", s.text);
        assert!(!s.text.contains("Free"), "it is not free: {}", s.text);
    }

    /// A refusal has to teach the rule, because the rule is invisible: the two
    /// tiles look identical and only their membership differs.
    #[test]
    fn a_cross_group_refusal_says_why_rather_than_just_no() {
        let d = fixture();
        let onto_eve = Command::Translate {
            grabbed: tid("bea"),
            delta: Cell { col: 2, row: 0 }
                .to_axial()
                .minus(Cell { col: 1, row: 0 }.to_axial()),
            detach: true,
        };
        let verdict = d.check(&onto_eve);
        let s = describe(&d, &Grip::Tile(tid("bea")), Cell { col: 2, row: 0 }, &verdict);
        assert_eq!(s.kind, StatusKind::Refused);
        assert!(s.text.contains("eve"));
        assert!(s.text.contains("group"), "it names the reason: {}", s.text);
    }

    /// A group drag names the group and a count, and NO cell — the cell under
    /// the pointer belongs to whichever member happened to be representative.
    #[test]
    fn a_group_status_names_the_group_and_never_a_cell() {
        let d = fixture();
        let s = describe(
            &d,
            &Grip::Group(gid("north")),
            Cell { col: 9, row: 9 },
            &Ok(Plan::Nothing),
        );
        assert!(s.text.contains("north"));
        assert!(s.text.contains("2 tiles"));
        assert!(!s.text.contains("column"), "no cell: {}", s.text);
    }

    /// And it never reaches for a tile label, which a pinned diagram does not
    /// have here.
    #[test]
    fn holding_a_tile_says_what_it_leaves_behind() {
        let d = fixture();
        let s = holding(&d, &Grip::Tile(tid("ana")));
        assert!(s.text.contains("ana"));
        assert!(s.text.contains("north"), "it says what stays: {}", s.text);
        assert!(!s.text.contains("The north"), "never a label: {}", s.text);
    }
}
