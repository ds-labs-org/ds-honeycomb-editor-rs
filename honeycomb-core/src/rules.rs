//! What a drag is allowed to do, as pure functions with no pointer in sight.
//!
//! TWO PROPERTIES EARN THIS MODULE ITS OWN FILE.
//!
//! [`Diagram::check`] is PURE. The view calls it on every pointer move so it can
//! paint the refusal BEFORE the user releases; a refusal discovered on release
//! is discovered too late, and a check that mutated would drag the document
//! along with the pointer, leaving Escape nothing to restore.
//!
//! [`Diagram::apply`] is ATOMIC and returns THE INVERSE COMMAND. Atomic because
//! the likeliest real bug in this feature is a loop that moves as it goes and
//! stops at the first collision, leaving five tiles moved and one behind — a
//! rearrangement nobody asked for, produced by a refusal. The inverse because
//! undo is then `apply(inverse)` rather than a second implementation of the
//! move, free to disagree with the first.

use std::collections::{BTreeMap, BTreeSet};

use crate::lattice::{Axial, Cell};
use crate::model::{Diagram, Group, GroupId, Link, LinkId, Mode, NewTile, TileId};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Rigid translation of a tile and, unless detached, everything in its
    /// group. The delta is AXIAL: translating in offset space breaks the shape
    /// for every delta with an odd row component, and the type is what makes
    /// that a compile error instead of a diagram that quietly loses its
    /// arrangement.
    Translate {
        grabbed: TileId,
        delta: Axial,
        detach: bool,
    },
    /// Membership, which a drag never changes. Present because without it this
    /// is a layout tool that can never fix a wrong grouping, and because it is
    /// what makes contiguity a reported property rather than an invariant.
    Attach {
        tile: TileId,
        group: GroupId,
    },
    Detach {
        tile: TileId,
    },
    /// Two tiles exchange cells. NOT REACHABLE FROM A GESTURE — nothing in
    /// `honeycomb-yew` constructs one — and it exists for exactly one reason:
    /// it is the inverse of a [`Command::Translate`] that swapped, and an
    /// inverse that has to be RE-DERIVED at undo time is an inverse that can be
    /// REFUSED at undo time.
    ///
    /// The tempting alternative is to keep returning the negated translate,
    /// which is correct today: a swap only happens when the moving set is one
    /// tile, so applying the opposite delta sends the grabbed tile back onto its
    /// partner and swaps them again. But that is a prediction about a FUTURE
    /// `check` call, not a property of the value being recorded — it holds only
    /// while the two tiles still share a group. Detach one of them and the
    /// recorded inverse becomes a plain refused move; `History::undo` pushes a
    /// refusal back onto its stack, so every later undo retries the same
    /// failure forever and the stack is wedged with nothing on screen to say
    /// why. Regrouping is explicitly the next feature, so that is one release
    /// away rather than hypothetical.
    ///
    /// A `Swap` needs no evidence from the diagram at all: it sends `a` to `b`'s
    /// cell and `b` to `a`'s, so running it again sends them back, for any two
    /// distinct placed tiles, whatever has happened to their membership.
    Swap {
        a: TileId,
        b: TileId,
    },
    /// A tile this board does not have yet.
    ///
    /// `what` carries the WHOLE payload, so the inverse of a [`Command::Remove`]
    /// needs no evidence from the diagram at undo time — the property
    /// [`Command::Swap`]'s note above demands of every recorded inverse, arriving
    /// again for the same reason.
    ///
    /// IT DOES NOT DECLARE A GROUP, and that is the decision this variant is
    /// really about. An `Add` that declared one is either not `Remove`'s exact
    /// inverse — the group survives the undo and the diagram does not come back
    /// to where it was — or it carries an "undeclare" that becomes WRONG LATER,
    /// once an [`Command::Attach`] has put a second tile in. So the set of
    /// groups a diagram declares is INVARIANT under this entire enum, and an
    /// `Add` naming a group the diagram has not declared is refused exactly the
    /// way an `Attach` is. A host that wants a group to be joinable declares it
    /// when it builds the diagram, empty if need be — nothing in the vocabulary
    /// or the shapes requires a group to have members.
    Add {
        tile: TileId,
        at: Cell,
        what: NewTile,
    },
    /// Puts a named set of tiles back on named cells.
    ///
    /// NOT REACHABLE FROM A GESTURE, and it exists for the reason
    /// [`Command::Swap`] does, one step further. The inverse of a
    /// [`Command::Translate`] used to be the opposite translate — which is not a
    /// value, it is a RULE FOR RE-DERIVING one, evaluated against whatever the
    /// diagram looks like at undo time. `moving_set` reads membership, so a tile
    /// ATTACHED to the group afterwards is dragged by the undo of a command
    /// applied before it existed; and a tile ADDED to the group afterwards is
    /// dragged to a cell it has never occupied. Both are reachable: `Attach` is
    /// public, and a palette hands out grouped tiles.
    ///
    /// A `Restore` names the tiles and the cells outright, so it moves exactly
    /// what the command it undoes moved, and nothing else, whatever has happened
    /// in between. It can still be REFUSED — if something else is standing on a
    /// cell it needs — but that is a true statement about the board rather than
    /// a quiet rearrangement of the wrong tiles.
    Restore {
        cells: BTreeMap<TileId, Cell>,
    },
    /// Draws a connection between two placements.
    ///
    /// A LINK CHANGES NO CELL, which is why it has no `Plan` of its own: the
    /// arrangement is untouched and the only thing that moves is what the
    /// drawing asserts.
    Connect {
        id: LinkId,
        link: Link,
    },
    /// Rubs one out. Its inverse is the `Connect` that restores it, carrying the
    /// whole link, so it needs no evidence from a diagram that no longer has it.
    Disconnect {
        id: LinkId,
    },
    /// Declares a group this diagram did not have.
    ///
    /// THE DECLARED GROUPS ARE NO LONGER INVARIANT UNDER THE WHOLE ENUM, and
    /// that property was load-bearing: it is what lets [`Command::Add`] refuse an
    /// undeclared group without an Add ever needing to declare one. It narrows
    /// rather than disappears — Add, Remove, Translate, Swap, Attach and Detach
    /// still never change the set, and these three exist to change it and
    /// nothing else. A host that wants a group joinable still declares it; it can
    /// now do so after construction, undoably.
    DeclareGroup {
        id: GroupId,
        group: Group,
    },
    /// Undeclares an EMPTY group. Refused while anything is in it: the tiles
    /// would be left naming a group the diagram does not have, which is exactly
    /// what `hsh:GroupBelongsToItsDiagram` catches in a file.
    RemoveGroup {
        id: GroupId,
    },
    /// Replaces a group's whole value — label, style key, note, extras.
    ///
    /// ONE COMMAND AND NOT THREE. A `SetLabel`/`SetStyle`/`SetNote` trio is three
    /// inverses to get right and three ways for a form to write one field and
    /// silently drop the other two; carrying the whole value means the inverse is
    /// just the previous whole value, which cannot be partially wrong.
    EditGroup {
        id: GroupId,
        group: Group,
    },
    /// Takes a tile off the board, content and all. Its inverse is the `Add`
    /// that puts it back, which `apply` can build because `take` hands back what
    /// it removed.
    Remove {
        tile: TileId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rejection {
    /// Carries its EVIDENCE. "You can't drop here" is useless; "these two are in
    /// the way" is actionable, and a host can outline exactly those tiles.
    Occupied {
        blocked: Vec<(Cell, TileId)>,
    },
    /// Delta is zero. Neither a change nor a refusal — a host turns this into a
    /// selection, and firing a refusal for it would flash an error on every
    /// click.
    NoMove,
    UnknownTile(TileId),
    UnknownGroup(GroupId),
    /// An `Add` for an id the board already has. NOT `Occupied`, which is about
    /// a CELL: the two are different mistakes with different fixes — rename the
    /// thing, or drop it somewhere else — and a host that conflated them would
    /// outline a hexagon that has nothing to do with the problem.
    AlreadyPlaced(TileId),
    /// A pinned tile offered to a standalone diagram, or the reverse. `Content`
    /// makes the mixed state unrepresentable at rest; this is the same rule at
    /// the doorway.
    WrongMode {
        diagram: Mode,
        offered: Mode,
    },
    /// A standalone tile with a blank label. `Diagram::try_new` refuses one at
    /// construction; an `Add` is the other way in, so it refuses one too.
    EmptyLabel(TileId),
    /// A group already declared, or one being removed while tiles are still in
    /// it — with those tiles named, because "not empty" without saying what is in
    /// it leaves a host nothing to point at.
    GroupInUse {
        group: GroupId,
        members: Vec<TileId>,
    },
    AlreadyDeclared(GroupId),
    /// A link id the diagram already uses.
    AlreadyConnected(LinkId),
    UnknownLink(LinkId),
    /// A link from a tile to itself: no direction and no length, so there is
    /// nothing for the renderer to draw.
    ///
    /// NOT "a link to a tile this diagram does not place" — this doc used to
    /// claim that too, and the code has never agreed: `check`'s `Connect` arm
    /// tests `link.from == link.to` first and returns THIS variant, then
    /// tests each end against `cell_of` and returns [`Rejection::UnknownTile`]
    /// for that case instead. `try_new`'s two matching `ModelError` variants
    /// already keep the same split — [`crate::model::ModelError::LinkToItself`]
    /// beside [`crate::model::ModelError::LinkToNowhere`] — so a document-time
    /// error and a live-edit error name the two failures the same way. The
    /// distinction is worth keeping rather than merging: "you typed the wrong
    /// tile" and "a line cannot point at itself" are different mistakes with
    /// different fixes, and a host that conflated them would have one fewer
    /// thing to tell the user. `a_link_needs_two_different_placed_tiles_and_an_unused_id`
    /// in `honeycomb-core/tests/rules.rs` pins the `UnknownTile` half.
    NotDrawable(LinkId),
    /// A tile cannot be removed while links still reach it: the links would name
    /// a placement the diagram no longer has, which `hsh:LinkEndsBelongToItsDiagram`
    /// catches in a file. Named, so a host can offer to remove them too.
    StillLinked {
        tile: TileId,
        links: Vec<LinkId>,
    },
    /// A `Remove` that would empty the board. `ModelError::NoPlacements` refuses
    /// an empty diagram at construction, and this is what keeps that true now
    /// that placements can leave: a diagram with nothing on it is a file that
    /// lost its contents, not a blank canvas.
    LastPlacement,
    /// Two of this diagram's subjects would be written with ONE IRI, mirroring
    /// `ModelError::SubjectCollision` — the same failure, caught here before an
    /// edit lands rather than only at the construction time `try_new` already
    /// covers.
    ///
    /// `DeclareGroup`, `Add` and `Connect` are the three commands that can mint
    /// a subject this diagram did not already have — a group, a tile, a
    /// placement, or a link — and until now `check` validated none of them
    /// against the rest of the diagram's subjects. A district typed "Hall"
    /// next to a tile slugged `hall` was accepted and `write_turtle` emitted
    /// `d:hall` twice, once as `hive:Group` and once as `hive:Tile` — a file
    /// this crate's own reader refuses and SHACL rejects for closedness.
    SubjectCollision {
        local: String,
        first: String,
        second: String,
    },
}

/// What `check` approved, as a SHAPE rather than a list of moves.
///
/// WHY THIS IS NOT `Vec<(TileId, Cell)>`, which is what it used to be. The one
/// corruption `Diagram::relocate` cannot survive is a plan naming one cell
/// twice: it writes `placement` and `occupancy` in two passes, so the loser of
/// a duplicate keeps a `placement` entry that `occupancy` contradicts, and
/// `cells()` reads `occupancy` — the tile disappears from the render and from
/// the serialisation with nothing returning an error. A `Vec` can express that.
/// These three variants cannot: `Rigid` is one delta applied to a set, which is
/// injective because translation is; `Exchange` holds two tile ids and NO cell
/// at all, so the cells can only come from the diagram at the moment the plan is
/// read.
///
/// AND THERE IS NO LONGER A PRIVATE FIELD, which the previous wording claimed
/// there still was. An enum's variants ARE its public constructors: anyone can
/// write `Plan::Exchange { .. }`. The guarantee did not move, it changed shape —
/// [`Diagram::apply`] takes a `Command` and builds its own plan from
/// [`Diagram::check`], so it is not a function a plan can be handed to, and
/// `Diagram::relocate` is `pub(crate)`. A `Plan` built outside this crate is
/// therefore a value with nowhere to go.
///
/// The invariant that matters is the one the SHAPE carries rather than the one
/// visibility carries, and that one survives construction by anybody: a `Rigid`
/// is injective because translation is, and an `Exchange` holds no cell, so the
/// only way to get cells out of either is [`Plan::moves`] with a diagram in
/// hand. The one abuse still expressible is `Exchange { a: x, b: x }`, and
/// `check` refuses to produce it — see the `NoMove` guard in the `Swap` arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Plan {
    /// `Attach` and `Detach` move nothing.
    Nothing,
    /// A tile and, unless detached, its group, translated by one delta.
    Rigid {
        tiles: BTreeSet<TileId>,
        delta: Axial,
    },
    /// Two tiles trade cells. `a` is the one the user moved; `b` is the one
    /// that was already there and is about to be displaced.
    Exchange { a: TileId, b: TileId },
    /// Named tiles to named cells. Injective because it comes from a map keyed
    /// by cell — see `check`'s `Restore` arm, which builds it that way.
    Exact { cells: BTreeMap<TileId, Cell> },
}

impl Plan {
    /// THE ONLY PLACE A PLAN BECOMES CELLS, and it needs the diagram to do it —
    /// which is the whole safety property: an `Exchange` stores no cell, so it
    /// cannot carry a stale one, and the pair it produces is two distinct cells
    /// by construction whenever `a != b`.
    pub fn moves(&self, d: &Diagram) -> Vec<(TileId, Cell)> {
        match self {
            Plan::Nothing => Vec::new(),
            Plan::Rigid { tiles, delta } => tiles
                .iter()
                .filter_map(|id| {
                    let from = d.cell_of(id)?;
                    Some((id.clone(), Cell::from_axial(from.to_axial().plus(*delta))))
                })
                .collect(),
            Plan::Exchange { a, b } => match (d.cell_of(a), d.cell_of(b)) {
                (Some(ca), Some(cb)) => vec![(a.clone(), cb), (b.clone(), ca)],
                _ => Vec::new(),
            },
            Plan::Exact { cells } => cells.iter().map(|(id, c)| (id.clone(), *c)).collect(),
        }
    }

    /// The tile that will move WITHOUT having been grabbed, if any. A host paints
    /// it differently and a status line names it: a second tile moving is the
    /// single moment a user is most likely to think the editor malfunctioned.
    pub fn displaced(&self) -> Option<&TileId> {
        match self {
            Plan::Exchange { b, .. } => Some(b),
            _ => None,
        }
    }

    /// `Plan::Nothing` no longer means only "Attach or Detach": an Add and a
    /// Remove change the board and move nothing. The old name said `is_empty`,
    /// which now reads as "this command does nothing".
    pub fn moves_nothing(&self) -> bool {
        matches!(self, Plan::Nothing)
    }
}

impl Diagram {
    /// PURE. Never mutates.
    pub fn check(&self, cmd: &Command) -> Result<Plan, Rejection> {
        match cmd {
            Command::Translate {
                grabbed,
                delta,
                detach,
            } => {
                if self.cell_of(grabbed).is_none() {
                    return Err(Rejection::UnknownTile(grabbed.clone()));
                }
                if delta.q == 0 && delta.r == 0 {
                    return Err(Rejection::NoMove);
                }
                let moving = self.moving_set(grabbed, *detach);
                let mut blocked = Vec::new();
                for id in &moving {
                    let from = self
                        .cell_of(id)
                        .expect("a member of the moving set is placed, by construction");
                    let to = Cell::from_axial(from.to_axial().plus(*delta));
                    // RESTRICTED TO TILES NOT IN THE MOVING SET, and that
                    // restriction is the whole rule: a group translated by one
                    // cell always overlaps its OWN old footprint, so checking
                    // against unrestricted occupancy refuses every short group
                    // drag while passing every other test in the suite.
                    if let Some(occupant) = self.occupant(&to)
                        && !moving.contains(occupant)
                    {
                        blocked.push((to, occupant.clone()));
                    }
                }
                // TWO MEMBERS OF ONE GROUP TRADE PLACES INSTEAD OF REFUSING, and
                // every clause of this guard is load-bearing:
                //
                //   * exactly one blocker, so nothing here has to decide which
                //     of several tiles the user meant;
                //   * exactly one tile moving, so the gesture was a drag of a
                //     single hexagon and not of a cluster — a group sliding onto
                //     another group is a collision, not a rearrangement;
                //   * both tiles in the SAME group, which is what makes this
                //     "reorder what is already yours" rather than a way to
                //     quietly restructure somebody else's cluster. Two UNGROUPED
                //     tiles deliberately do not qualify: `None == None` is not
                //     "the same group", it is the absence of one, and treating
                //     it as a match would make swapping the default behaviour of
                //     a diagram with no groups at all.
                //
                // `moving.len() == 1` already implies `detach` for any grouped
                // tile — `moving_set` returns the whole group otherwise — so
                // there is no `*detach &&` term here. That is arithmetic, not an
                // oversight, and there is a test row that fails if it stops
                // being true.
                if let [(_, blocker)] = blocked.as_slice()
                    && moving.len() == 1
                    && let Some(ga) = self.group_of(grabbed)
                    && let Some(gb) = self.group_of(blocker)
                    && ga == gb
                {
                    return Ok(Plan::Exchange {
                        a: grabbed.clone(),
                        b: blocker.clone(),
                    });
                }
                if !blocked.is_empty() {
                    // In (row, col) order, so a host that outlines them walks
                    // the board the way a reader does rather than in whatever
                    // order the moving set happened to be in.
                    blocked.sort();
                    return Err(Rejection::Occupied { blocked });
                }
                Ok(Plan::Rigid {
                    tiles: moving,
                    delta: *delta,
                })
            }
            Command::Attach { tile, group } => {
                if self.cell_of(tile).is_none() {
                    return Err(Rejection::UnknownTile(tile.clone()));
                }
                if !self.has_group(group) {
                    return Err(Rejection::UnknownGroup(group.clone()));
                }
                Ok(Plan::Nothing)
            }
            Command::Detach { tile } => {
                if self.cell_of(tile).is_none() {
                    return Err(Rejection::UnknownTile(tile.clone()));
                }
                Ok(Plan::Nothing)
            }
            // NO GROUP CHECK HERE, deliberately. The same-group restriction is a
            // rule about the GESTURE and lives in the `Translate` arm above; a
            // host holding a `Swap` has already decided, the way a host holding
            // an `Attach` has. What this arm owes is that the command is
            // APPLICABLE — two distinct tiles that are both on the board — so
            // that an inverse recorded now still applies later.
            Command::Swap { a, b } => {
                if self.cell_of(a).is_none() {
                    return Err(Rejection::UnknownTile(a.clone()));
                }
                if self.cell_of(b).is_none() {
                    return Err(Rejection::UnknownTile(b.clone()));
                }
                if a == b {
                    return Err(Rejection::NoMove);
                }
                Ok(Plan::Exchange {
                    a: a.clone(),
                    b: b.clone(),
                })
            }
            // ORDERED SO THE ANSWER IS THE MOST USEFUL ONE. A blank-labelled
            // tile offered to the wrong mode should hear about the mode, not the
            // label — the label is fixable and the mode is a category error —
            // and a cell that is occupied matters only once everything about the
            // tile itself is in order.
            Command::Add { tile, at, what } => {
                if self.cell_of(tile).is_some() {
                    return Err(Rejection::AlreadyPlaced(tile.clone()));
                }
                if what.mode() != self.mode() {
                    return Err(Rejection::WrongMode {
                        diagram: self.mode(),
                        offered: what.mode(),
                    });
                }
                if what.label().is_some_and(|l| l.trim().is_empty()) {
                    return Err(Rejection::EmptyLabel(tile.clone()));
                }
                // THE VERBATIM CHECK `Attach` MAKES, and deliberately so: those
                // are the only two ways a tile comes to name a group, and a
                // diagram where one of them admits an undeclared group is a
                // diagram `hsh:GroupBelongsToItsDiagram` rejects.
                if let Some(g) = what.group()
                    && !self.has_group(g)
                {
                    return Err(Rejection::UnknownGroup(g.clone()));
                }
                // EVERY SUBJECT THIS ADD WOULD MINT, CHECKED AGAINST WHAT THE
                // DIAGRAM ALREADY MINTS. `Diagram::would_collide` is the same
                // check `try_new` runs over a whole spec at once, asked here
                // about one command before it lands — see the module header on
                // `model::Diagram` for the file this repairs. A STANDALONE
                // tile mints two subjects, its own and its placement's; a
                // PINNED tile mints only the placement, because `PinnedTile`
                // has nowhere to hold one of its own (the module header's
                // second bullet again, at the doorway instead of at rest).
                if self.mode() == Mode::Standalone
                    && let Some(first) = self.would_collide(tile.0.as_str())
                {
                    return Err(Rejection::SubjectCollision {
                        local: tile.0.as_str().to_string(),
                        first,
                        second: format!("tile {}", tile.0.as_str()),
                    });
                }
                let placement = format!("{}{}", crate::ttl::PLACEMENT_PREFIX, tile.0.as_str());
                if let Some(first) = self.would_collide(&placement) {
                    return Err(Rejection::SubjectCollision {
                        local: placement,
                        first,
                        second: format!("the placement of {}", tile.0.as_str()),
                    });
                }
                if let Some(occupant) = self.occupant(at) {
                    return Err(Rejection::Occupied {
                        blocked: vec![(*at, occupant.clone())],
                    });
                }
                Ok(Plan::Nothing)
            }
            Command::Restore { cells } => {
                if cells.is_empty() {
                    return Err(Rejection::NoMove);
                }
                let mut blocked = Vec::new();
                for (id, to) in cells {
                    if self.cell_of(id).is_none() {
                        return Err(Rejection::UnknownTile(id.clone()));
                    }
                    // RESTRICTED TO TILES NOT IN THE SET, the same restriction
                    // the Translate arm makes and for the same reason: a set
                    // going back to where it was always overlaps its own current
                    // footprint.
                    if let Some(occupant) = self.occupant(to)
                        && !cells.contains_key(occupant)
                    {
                        blocked.push((*to, occupant.clone()));
                    }
                }
                if !blocked.is_empty() {
                    blocked.sort();
                    return Err(Rejection::Occupied { blocked });
                }
                // Keyed by cell so two tiles cannot be sent to one: a duplicate
                // target is the one corruption `relocate` cannot survive, and
                // building the map this way makes it unrepresentable rather
                // than checked.
                let by_cell: BTreeMap<Cell, TileId> =
                    cells.iter().map(|(id, c)| (*c, id.clone())).collect();
                if by_cell.len() != cells.len() {
                    return Err(Rejection::Occupied {
                        blocked: Vec::new(),
                    });
                }
                Ok(Plan::Exact {
                    cells: cells.clone(),
                })
            }
            Command::Connect { id, link } => {
                if self.link(id).is_some() {
                    return Err(Rejection::AlreadyConnected(id.clone()));
                }
                // A link mints a subject with its own id, in both modes — see
                // `Command::Add`'s comment for why the check lives here at all
                // rather than only in `try_new`.
                if let Some(first) = self.would_collide(id.0.as_str()) {
                    return Err(Rejection::SubjectCollision {
                        local: id.0.as_str().to_string(),
                        first,
                        second: format!("link {}", id.0.as_str()),
                    });
                }
                if link.from == link.to {
                    return Err(Rejection::NotDrawable(id.clone()));
                }
                for end in [&link.from, &link.to] {
                    if self.cell_of(end).is_none() {
                        return Err(Rejection::UnknownTile(end.clone()));
                    }
                }
                Ok(Plan::Nothing)
            }
            Command::Disconnect { id } => {
                if self.link(id).is_none() {
                    return Err(Rejection::UnknownLink(id.clone()));
                }
                Ok(Plan::Nothing)
            }
            Command::DeclareGroup { id, .. } => {
                if self.has_group(id) {
                    return Err(Rejection::AlreadyDeclared(id.clone()));
                }
                // A GROUP MINTS A SUBJECT WITH ITS OWN SLUG, IN BOTH MODES — a
                // `Group`, unlike a `PinnedTile`, always has one of its own.
                // `try_new` already refuses two subjects sharing one IRI at
                // construction; this is the same check for the one command
                // that can add a group after construction, so a district typed
                // from a name that happens to match an existing tile,
                // placement or link no longer mints a file this crate's own
                // reader refuses. See the module header on `model::Diagram`.
                if let Some(first) = self.would_collide(id.0.as_str()) {
                    return Err(Rejection::SubjectCollision {
                        local: id.0.as_str().to_string(),
                        first,
                        second: format!("group {}", id.0.as_str()),
                    });
                }
                Ok(Plan::Nothing)
            }
            Command::RemoveGroup { id } => {
                if !self.has_group(id) {
                    return Err(Rejection::UnknownGroup(id.clone()));
                }
                let members = self.members(id);
                if !members.is_empty() {
                    return Err(Rejection::GroupInUse {
                        group: id.clone(),
                        members,
                    });
                }
                Ok(Plan::Nothing)
            }
            Command::EditGroup { id, .. } => {
                if !self.has_group(id) {
                    return Err(Rejection::UnknownGroup(id.clone()));
                }
                Ok(Plan::Nothing)
            }
            Command::Remove { tile } => {
                if self.cell_of(tile).is_none() {
                    return Err(Rejection::UnknownTile(tile.clone()));
                }
                if self.cells().count() <= 1 {
                    return Err(Rejection::LastPlacement);
                }
                // REFUSED RATHER THAN CASCADED, and the choice is about the
                // inverse. A removal that also swept up the links would have to
                // carry them all back, so `Add` would grow a field it needs in
                // one case out of many — and a user who did not notice the links
                // go would not notice them come back either. Named, so the host
                // can offer to remove them first.
                let links = self.links_at(tile);
                if !links.is_empty() {
                    return Err(Rejection::StillLinked {
                        tile: tile.clone(),
                        links,
                    });
                }
                Ok(Plan::Nothing)
            }
        }
    }

    /// ATOMIC, and returns THE INVERSE COMMAND. Calls `check` itself — it takes
    /// a `Command`, never a `Plan` — so a partially applied move is not
    /// expressible: the only plan it can act on is one it just validated.
    pub fn apply(&mut self, cmd: Command) -> Result<Command, Rejection> {
        let plan = self.check(&cmd)?;
        // Owned, so the immutable borrow of `self` ends before `relocate` takes
        // it mutably. Also the last moment the cells are read: everything below
        // works from this snapshot.
        let moves = plan.moves(self);
        match cmd {
            Command::Translate {
                grabbed,
                delta,
                detach,
            } => {
                // WHERE THEY WERE, CAPTURED BEFORE THE MOVE. This is the whole
                // difference between an inverse that is a value and one that is
                // a rule: read now, while the answer is still true.
                let was: BTreeMap<TileId, Cell> = moves
                    .iter()
                    .filter_map(|(id, _)| Some((id.clone(), self.cell_of(id)?)))
                    .collect();
                let _ = (&grabbed, &delta, &detach);
                self.relocate(&moves);
                // A DETACHED TRANSLATE NARROWS THE MOVING SET AND CHANGES NO
                // MEMBERSHIP, deliberately. The inverse of a Translate is a
                // Translate; if a detached drop also cleared the tile's group,
                // `apply(inverse)` would put the tile back and leave the
                // grouping gone — undo silently lossy in exactly the situation
                // the user reached for undo. Membership has its own two verbs,
                // and they have their own inverses.
                // THE INVERSE OF A TRANSLATE THAT SWAPPED IS A SWAP, not the
                // opposite translate. Both undo correctly today; only one of
                // them still undoes correctly after the pair stops sharing a
                // group, and a recorded inverse that can be refused later wedges
                // the undo stack rather than failing loudly. See
                // `Command::Swap`.
                match plan {
                    Plan::Exchange { a, b } => Ok(Command::Swap { a, b }),
                    _ => Ok(Command::Restore { cells: was }),
                }
            }
            Command::Attach { tile, group } => {
                let previous = self.group_of(&tile).cloned();
                self.set_group(&tile, Some(group));
                Ok(match previous {
                    Some(g) => Command::Attach { tile, group: g },
                    None => Command::Detach { tile },
                })
            }
            Command::Detach { tile } => {
                let previous = self.group_of(&tile).cloned();
                self.set_group(&tile, None);
                Ok(match previous {
                    Some(g) => Command::Attach { tile, group: g },
                    // Detaching a tile that had no group changes nothing, and
                    // its inverse must therefore also change nothing. Returning
                    // the same command is the only inverse that holds.
                    None => Command::Detach { tile },
                })
            }
            // SELF-INVERSE, EXACTLY AND UNCONDITIONALLY. It sends `a` to `b`'s
            // cell and `b` to `a`'s, so running it again on the result sends
            // them back — for any two distinct placed tiles, whatever has
            // happened to their membership in between. The only command in this
            // enum whose inverse needs no evidence from the diagram at all,
            // which is precisely why a swapped translate records one of these.
            Command::Swap { a, b } => {
                self.relocate(&moves);
                Ok(Command::Swap { a, b })
            }
            Command::Restore { cells } => {
                let was: BTreeMap<TileId, Cell> = cells
                    .keys()
                    .filter_map(|id| Some((id.clone(), self.cell_of(id)?)))
                    .collect();
                self.relocate(&moves);
                Ok(Command::Restore { cells: was })
            }
            Command::Connect { id, link } => {
                self.add_link(id.clone(), link);
                Ok(Command::Disconnect { id })
            }
            Command::Disconnect { id } => match self.take_link(&id) {
                Some(link) => Ok(Command::Connect { id, link }),
                None => Err(Rejection::UnknownLink(id)),
            },
            Command::DeclareGroup { id, group } => {
                self.declare_group(id.clone(), group);
                Ok(Command::RemoveGroup { id })
            }
            Command::RemoveGroup { id } => match self.undeclare_group(&id) {
                Some(group) => Ok(Command::DeclareGroup { id, group }),
                None => Err(Rejection::UnknownGroup(id)),
            },
            Command::EditGroup { id, group } => match self.swap_group(&id, group) {
                Some(was) => Ok(Command::EditGroup { id, group: was }),
                None => Err(Rejection::UnknownGroup(id)),
            },
            Command::Add { tile, at, what } => {
                self.insert(tile.clone(), at, what);
                Ok(Command::Remove { tile })
            }
            // SELF-CONTAINED BY CONSTRUCTION. `take` hands back the cell and the
            // content, so the `Add` recorded here carries everything needed to
            // undo — and, unlike an inverse re-derived at undo time, it cannot
            // be refused later for a reason that did not exist when it was
            // recorded. `check` has already established the tile is placed, so
            // the `else` is unreachable and says so rather than unwrapping.
            Command::Remove { tile } => match self.take(&tile) {
                Some((at, what)) => Ok(Command::Add { tile, at, what }),
                None => Err(Rejection::UnknownTile(tile)),
            },
        }
    }

    /// Every tile a drag on `grabbed` would carry, as a set the caller can test
    /// membership in. Exposed because a host paints the whole moving set while
    /// the pointer is down, not just the tile under it.
    pub fn moving_cells(&self, grabbed: &TileId, detach: bool) -> BTreeSet<Cell> {
        self.moving_set(grabbed, detach)
            .iter()
            .filter_map(|id| self.cell_of(id))
            .collect()
    }
}

/// How many commands the history remembers. A cap rather than unbounded because
/// the alternative is a tab that grows without limit over an afternoon of
/// dragging; 100 is far more than the distance anyone undoes by hand.
const CAP: usize = 100;

/// Undo lives in the model, not in the URL: `replaceState` creates no history
/// entry so Back would leave the page and lose the work, and `pushState` makes
/// Back-through-fifty-edits indistinguishable from Back-to-the-previous-page
/// against every user's muscle memory.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl History {
    /// Record the INVERSE that `apply` handed back, not the command that was
    /// applied. Storing the command and inverting it later would be a second
    /// implementation of the inverse, free to disagree with `apply`'s.
    pub fn record(&mut self, inverse: Command) {
        self.undo.push(inverse);
        if self.undo.len() > CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, d: &mut Diagram) -> Option<Command> {
        let cmd = self.undo.pop()?;
        match d.apply(cmd.clone()) {
            Ok(inverse) => {
                self.redo.push(inverse);
                Some(cmd)
            }
            // The stack is put back rather than dropped: a refused undo is a bug
            // somewhere else, and swallowing the entry would turn it into work
            // that cannot be recovered even once the bug is fixed.
            Err(_) => {
                self.undo.push(cmd);
                None
            }
        }
    }

    pub fn redo(&mut self, d: &mut Diagram) -> Option<Command> {
        let cmd = self.redo.pop()?;
        match d.apply(cmd.clone()) {
            Ok(inverse) => {
                self.undo.push(inverse);
                Some(cmd)
            }
            Err(_) => {
                self.redo.push(cmd);
                None
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
