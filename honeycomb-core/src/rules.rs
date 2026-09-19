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

use std::collections::BTreeSet;

use crate::lattice::{Axial, Cell};
use crate::model::{Diagram, GroupId, TileId};

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
/// The field stayed private and the constructor stayed absent for the same
/// reason as before: only [`Diagram::check`] may build one, so [`Diagram::apply`]
/// cannot be handed a plan that nothing validated.
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

    pub fn is_empty(&self) -> bool {
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
        }
    }

    /// ATOMIC, and returns THE INVERSE COMMAND. Calls `check` first; a partially
    /// applied move is not expressible because `Plan`'s field is private and can
    /// only come from `check`.
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
                    _ => Ok(Command::Translate {
                        grabbed,
                        delta: Axial {
                            q: -delta.q,
                            r: -delta.r,
                        },
                        detach,
                    }),
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
