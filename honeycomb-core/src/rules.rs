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

/// The moves `check` approved. Its field is crate-private and it has no public
/// constructor, so `apply` cannot be handed a plan nothing validated.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub(crate) moves: Vec<(TileId, Cell)>,
}

impl Plan {
    pub fn moves(&self) -> &[(TileId, Cell)] {
        &self.moves
    }

    pub fn is_empty(&self) -> bool {
        self.moves.is_empty()
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
                let mut moves = Vec::with_capacity(moving.len());
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
                    moves.push((id.clone(), to));
                }
                if !blocked.is_empty() {
                    // In (row, col) order, so a host that outlines them walks
                    // the board the way a reader does rather than in whatever
                    // order the moving set happened to be in.
                    blocked.sort();
                    return Err(Rejection::Occupied { blocked });
                }
                Ok(Plan { moves })
            }
            Command::Attach { tile, group } => {
                if self.cell_of(tile).is_none() {
                    return Err(Rejection::UnknownTile(tile.clone()));
                }
                if !self.has_group(group) {
                    return Err(Rejection::UnknownGroup(group.clone()));
                }
                Ok(Plan { moves: Vec::new() })
            }
            Command::Detach { tile } => {
                if self.cell_of(tile).is_none() {
                    return Err(Rejection::UnknownTile(tile.clone()));
                }
                Ok(Plan { moves: Vec::new() })
            }
        }
    }

    /// ATOMIC, and returns THE INVERSE COMMAND. Calls `check` first; a partially
    /// applied move is not expressible because `Plan`'s field is private and can
    /// only come from `check`.
    pub fn apply(&mut self, cmd: Command) -> Result<Command, Rejection> {
        let plan = self.check(&cmd)?;
        match cmd {
            Command::Translate {
                grabbed,
                delta,
                detach,
            } => {
                self.relocate(&plan.moves);
                // A DETACHED TRANSLATE NARROWS THE MOVING SET AND CHANGES NO
                // MEMBERSHIP, deliberately. The inverse of a Translate is a
                // Translate; if a detached drop also cleared the tile's group,
                // `apply(inverse)` would put the tile back and leave the
                // grouping gone — undo silently lossy in exactly the situation
                // the user reached for undo. Membership has its own two verbs,
                // and they have their own inverses.
                Ok(Command::Translate {
                    grabbed,
                    delta: Axial {
                        q: -delta.q,
                        r: -delta.r,
                    },
                    detach,
                })
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
