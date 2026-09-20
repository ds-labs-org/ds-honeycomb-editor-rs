//! Integer cells, axial arithmetic, and the one place a cell becomes a point.
//!
//! WHY THE ARITHMETIC IS HERE AND NOT IN THE COMPONENT. Two consumers need it
//! and only one of them is a browser: a host-side generator lays a diagram out
//! with no wasm anywhere, and the editor hit-tests a pointer against the same
//! lattice. If the two computed a centre from separately written expressions
//! they would be free to disagree in the last float bit, and a press on a
//! boundary would name one cell for the generator and another for the editor —
//! a bug with no symptom until a drop lands somewhere nobody expected.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// sqrt(3) as the nearest f64, written out rather than computed so that the
/// value is a `const` and so no target's libm can produce a second one.
/// `sqrt3_is_the_real_thing` asserts it is exactly `3.0_f64.sqrt()`.
pub(crate) const SQRT3: f64 = 1.732_050_807_568_877_2;

/// An offset cell: the pair `hive:col` / `hive:row` serialises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub col: i32,
    pub row: i32,
}

/// HAND-WRITTEN, not derived, and the difference is load-bearing. The derived
/// order would be `(col, row)` — the field order — while the serialiser writes
/// placements in `(row, col)`. A writer that iterated a `BTreeMap<Cell, _>` and
/// trusted the derived order would emit a different byte order than the golden
/// tests expect, and the whole export/commit/review loop depends on the bytes
/// being stable across an edit that changed nothing.
impl Ord for Cell {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.row, self.col).cmp(&(other.row, other.col))
    }
}

impl PartialOrd for Cell {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Axial coordinates. A TYPE and not a helper function, so that translating a
/// group in offset space — which breaks the shape for every delta with an odd
/// row component — is a compile error at every call site rather than a diagram
/// that silently loses its arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Axial {
    pub q: i32,
    pub r: i32,
}

impl Cell {
    /// `q = col - row.div_euclid(2)`. div_euclid, NEVER `/`: Rust integer
    /// division truncates toward zero, so `-1 / 2 == 0` while `floor(-0.5) ==
    /// -1`. Row -1 is ordinary — the origin is wherever the first tile landed,
    /// not a corner — and a `/` here passes every test whose rows happen to be
    /// non-negative and loses every cell above the first row.
    pub fn to_axial(self) -> Axial {
        Axial {
            q: self.col - self.row.div_euclid(2),
            r: self.row,
        }
    }

    /// `col = q + r.div_euclid(2)`, the exact inverse, for the same reason.
    pub fn from_axial(a: Axial) -> Cell {
        Cell {
            col: a.q + a.r.div_euclid(2),
            row: a.r,
        }
    }

    /// The six edge-sharers, with `s = row.rem_euclid(2)`. rem_euclid, never
    /// `%`: `%` keeps the sign of the dividend, so row -1 would shift left where
    /// row 1 shifts right, and contiguity would report a fracture that is not
    /// there.
    pub fn neighbours(self) -> [Cell; 6] {
        let s = self.row.rem_euclid(2);
        let (c, r) = (self.col, self.row);
        [
            Cell { col: c - 1, row: r },
            Cell { col: c + 1, row: r },
            Cell {
                col: c - 1 + s,
                row: r - 1,
            },
            Cell {
                col: c + s,
                row: r - 1,
            },
            Cell {
                col: c - 1 + s,
                row: r + 1,
            },
            Cell {
                col: c + s,
                row: r + 1,
            },
        ]
    }

    /// In a hex lattice two distinct cells share an edge or are disjoint; there
    /// is no corner-touching case and so no 4-vs-8 choice to make.
    pub fn is_neighbour(self, other: Cell) -> bool {
        self.neighbours().contains(&other)
    }
}

impl Axial {
    pub fn plus(self, d: Axial) -> Axial {
        Axial {
            q: self.q + d.q,
            r: self.r + d.r,
        }
    }

    pub fn minus(self, o: Axial) -> Axial {
        Axial {
            q: self.q - o.q,
            r: self.r - o.r,
        }
    }

    /// How many steps from one cell to the other over the 6-neighbour relation:
    /// `(|dq| + |dr| + |dq + dr|) / 2`.
    ///
    /// THE THIRD TERM IS THE WHOLE FUNCTION. Axial coordinates are cube
    /// coordinates with the third axis left implicit as `s = -q - r`, and hex
    /// distance is the largest of the three absolute differences; `|dq + dr|`
    /// is `|ds|`, and the half-sum of all three equals that maximum because the
    /// three always sum to zero. Dropping it gives Manhattan distance on a
    /// square grid, which is right along a row and wrong on every diagonal —
    /// the kind of error that puts a line on the correct cell in every test
    /// whose tiles happen to be in one row.
    ///
    /// IT LIVES HERE AND NOT IN `model`, next to its one caller, for the reason
    /// this module exists at all: two consumers do this arithmetic and only one
    /// of them is a browser, so a second copy of it anywhere is a second copy
    /// free to disagree about which hexagon a line meets a region at.
    ///
    /// EXACT AND INTEGER, never a float: the anchor is chosen by comparing two
    /// of these, and a tie broken by floating-point noise would move a line
    /// between two members of a group for no reason a reader could see.
    pub fn distance(self, other: Axial) -> i32 {
        let d = self.minus(other);
        (d.q.abs() + d.r.abs() + (d.q + d.r).abs()) / 2
    }
}

/// Hex radius and gap in board units. PRESENTATION, never serialised: two hosts
/// may draw the same document at different sizes and it is the same diagram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lattice {
    pub r: f64,
    pub gap: f64,
}

impl Lattice {
    pub const fn new(r: f64, gap: f64) -> Self {
        Lattice { r, gap }
    }

    /// Flat width of a pointy-top hexagon: vertical left and right edges.
    pub fn w(&self) -> f64 {
        SQRT3 * self.r
    }

    /// Point-to-point height.
    pub fn h(&self) -> f64 {
        2.0 * self.r
    }

    /// Centre-to-centre distance of two edge-sharing neighbours in one row.
    pub fn step(&self) -> f64 {
        self.w() * self.gap
    }

    /// THE ONE DEFINITION, written `self.step() * SQRT3 / 2.0` VERBATIM and
    /// never as `1.5 * self.r * self.gap`. The two are equal in the reals and
    /// differ in the last bit in f64 — at r = 46, gap = 1 they are
    /// 68.99999999999998579 and 69.0 exactly. If a host recomputes the
    /// expression inline and this method disagrees in the last bit, a
    /// generator-placed cell and a pointer hit-test can name different cells for
    /// a point on a boundary, and nothing on screen says why the drop went
    /// elsewhere.
    pub fn pitch(&self) -> f64 {
        self.step() * SQRT3 / 2.0
    }

    /// `x = step*(col + 0.5*(row.rem_euclid(2)))`, `y = pitch*row`.
    /// rem_euclid for the reason `neighbours` gives: a negative row must keep
    /// its parity or it is drawn shifted the wrong way.
    pub fn centre(&self, c: Cell) -> (f64, f64) {
        let s = c.row.rem_euclid(2) as f64;
        (
            self.step() * (c.col as f64 + 0.5 * s),
            self.pitch() * c.row as f64,
        )
    }

    /// TOTAL: every finite point maps to exactly one cell, by cube rounding,
    /// which is provably nearest-centre because regular hexagons ARE the Voronoi
    /// cells of their centres. Totality is what lets the host hit-test by
    /// arithmetic instead of by DOM — a press on a label, on a `fill="none"`
    /// frame path, or between two cells' ink all hit the wrong element.
    ///
    /// Ties at edge midpoints and vertices resolve deterministically to one of
    /// the genuinely equidistant cells; WHICH one is unspecified, and a caller
    /// that asserts it is asserting a float artefact.
    pub fn cell_at(&self, x: f64, y: f64) -> Cell {
        let rf = y / self.pitch();
        let qf = x / self.step() - rf / 2.0;
        let sf = -qf - rf;
        let (mut q, mut r, s) = (qf.round(), rf.round(), sf.round());
        let (dq, dr, ds) = ((q - qf).abs(), (r - rf).abs(), (s - sf).abs());
        if dq > dr && dq > ds {
            q = -r - s;
        } else if dr > ds {
            r = -q - s;
        }
        Cell::from_axial(Axial {
            q: q as i32,
            r: r as i32,
        })
    }

    /// Hysteresis for a live drag. Cube rounding resolves an exactly-equidistant
    /// point deterministically but arbitrarily, so a pointer resting on an edge
    /// flickers between two cells as the last float bit moves. Switches only
    /// when the new centre is nearer by more than `0.08 * step` — 16% of a
    /// half-width, enough to kill the flicker and small enough to feel
    /// immediate.
    ///
    /// Lives HERE, not in the view: it is pure lattice arithmetic, and a pure
    /// function in a browser-only crate can only be tested in a browser.
    pub fn next_candidate(&self, current: Cell, x: f64, y: f64) -> Cell {
        let c = self.cell_at(x, y);
        if c == current {
            return current;
        }
        let d = |k: Cell| {
            let (cx, cy) = self.centre(k);
            ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt()
        };
        if d(current) - d(c) > 0.08 * self.step() {
            c
        } else {
            current
        }
    }

    /// Six vertices as an SVG path `d`, at an arbitrary radius so a host can
    /// draw the grown hexagons a group region is unioned from.
    pub fn hex_path(&self, cx: f64, cy: f64, r: f64) -> String {
        let dx = r * SQRT3 / 2.0;
        let half = r / 2.0;
        let p = |x: f64, y: f64| format!("{x:.3},{y:.3}");
        format!(
            "M{} L{} L{} L{} L{} L{} Z",
            p(cx, cy - r),
            p(cx + dx, cy - half),
            p(cx + dx, cy + half),
            p(cx, cy + r),
            p(cx - dx, cy + half),
            p(cx - dx, cy - half),
        )
    }
}

/// The lattice-space to user-unit mapping, EXPORTED rather than left as four
/// locals inside a render function, so that the generator and the editor cannot
/// place the same diagram at two different origins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub min: Cell,
    pub max: Cell,
    pub origin_x: f64,
    pub origin_y: f64,
    pub pad: f64,
}

impl Frame {
    /// None only for an empty iterator. A `Diagram` always has at least one
    /// placement, so a frame built from one is always Some.
    pub fn around(cells: impl Iterator<Item = Cell>, l: Lattice, pad: f64) -> Option<Frame> {
        let mut it = cells;
        let first = it.next()?;
        let (mut min_col, mut max_col) = (first.col, first.col);
        let (mut min_row, mut max_row) = (first.row, first.row);
        for c in it {
            min_col = min_col.min(c.col);
            max_col = max_col.max(c.col);
            min_row = min_row.min(c.row);
            max_row = max_row.max(c.row);
        }
        // The leftmost ink is at an EVEN row of min_col; an odd row is half a
        // step further right. Taking the even case is what stops a diagram whose
        // topmost row happens to be odd from being cropped on the left.
        let left = l.step() * min_col as f64 - l.w() / 2.0;
        let top = l.pitch() * min_row as f64 - l.r;
        Some(Frame {
            min: Cell {
                col: min_col,
                row: min_row,
            },
            max: Cell {
                col: max_col,
                row: max_row,
            },
            origin_x: pad - left,
            origin_y: pad - top,
            pad,
        })
    }

    pub fn at(&self, c: Cell, l: Lattice) -> (f64, f64) {
        let (x, y) = l.centre(c);
        (self.origin_x + x, self.origin_y + y)
    }

    pub fn cell_at(&self, x: f64, y: f64, l: Lattice) -> Cell {
        l.cell_at(x - self.origin_x, y - self.origin_y)
    }

    /// `(min_x, min_y, width, height)` for an `<svg viewBox>`. The origin is
    /// always (0,0): a viewBox with a non-zero origin and a translated content
    /// are two ways of saying one thing, and hosts that mix them draw the pad on
    /// one side only.
    pub fn viewbox(&self, l: Lattice) -> (f64, f64, f64, f64) {
        // The rightmost ink is at an ODD row of max_col, half a step further
        // right than an even one — the mirror of the `left` case above.
        let right = l.step() * (self.max.col as f64 + 0.5) + l.w() / 2.0;
        let bottom = l.pitch() * self.max.row as f64 + l.r;
        (
            0.0,
            0.0,
            self.origin_x + right + self.pad,
            self.origin_y + bottom + self.pad,
        )
    }
}

/// A route from one cell to another that never enters a blocked one.
///
/// BREADTH-FIRST, NOT A*, and the difference does not matter here: a board is a
/// few hundred cells and every step costs the same, so BFS finds a shortest path
/// and a heuristic would only save time nobody is waiting for.
///
/// THE ENDS ARE NOT BLOCKED BY THEMSELVES. A link runs between two OCCUPIED
/// cells by definition, so `from` and `to` are exempt from `blocked` — without
/// that the search starts inside a wall and returns nothing, every time.
///
/// Returns the whole route including both ends, or `None` when the blocked cells
/// separate them. A host that gets `None` has to fall back to a straight line:
/// a link the user drew and the board declines to draw is worse than one drawn
/// across something.
pub fn route(from: Cell, to: Cell, blocked: &BTreeSet<Cell>, bound: i32) -> Option<Vec<Cell>> {
    if from == to {
        return Some(vec![from]);
    }
    // A box around both ends, grown, so the search cannot wander off across an
    // unbounded lattice looking for a way round something that has no way round.
    let (lo_col, hi_col) = (from.col.min(to.col) - bound, from.col.max(to.col) + bound);
    let (lo_row, hi_row) = (from.row.min(to.row) - bound, from.row.max(to.row) + bound);

    let mut came: BTreeMap<Cell, Cell> = BTreeMap::new();
    let mut seen: BTreeSet<Cell> = [from].into_iter().collect();
    let mut queue: VecDeque<Cell> = [from].into_iter().collect();
    while let Some(c) = queue.pop_front() {
        if c == to {
            let mut out = vec![to];
            let mut at = to;
            while let Some(prev) = came.get(&at) {
                out.push(*prev);
                at = *prev;
            }
            out.reverse();
            return Some(out);
        }
        for n in c.neighbours() {
            if n.col < lo_col || n.col > hi_col || n.row < lo_row || n.row > hi_row {
                continue;
            }
            if n != to && blocked.contains(&n) {
                continue;
            }
            if seen.insert(n) {
                came.insert(n, c);
                queue.push_back(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The constant exists so host and wasm cannot disagree; that is only worth
    /// anything if it is the same number the libm would have produced.
    #[test]
    fn sqrt3_is_the_real_thing() {
        assert_eq!(
            SQRT3,
            3.0_f64.sqrt(),
            "the written-out sqrt(3) is not the f64 the platform computes, so a host that wrote \
             `3.0_f64.sqrt()` inline and this crate would place the same cell at two different \
             points"
        );
    }

    /// Cube rounding is claimed to be nearest-centre. Over a block that includes
    /// negative rows and columns, every centre must round back to its own cell —
    /// the cheapest possible refutation of a `/`-instead-of-`div_euclid` bug.
    #[test]
    fn every_centre_rounds_back_to_its_own_cell() {
        let l = Lattice::new(46.0, 1.0);
        for row in -20..=20 {
            for col in -20..=20 {
                let c = Cell { col, row };
                let (x, y) = l.centre(c);
                assert_eq!(
                    l.cell_at(x, y),
                    c,
                    "the centre of {c:?} hit-tests as a different cell, so a press in the middle of \
                     a hexagon picks up its neighbour"
                );
            }
        }
    }

    /// Ordering is what the serialiser iterates. If it ever became the derived
    /// `(col, row)` the exported bytes would reorder without a single placement
    /// having moved.
    #[test]
    fn cells_order_by_row_then_column() {
        let mut v = [
            Cell { col: 0, row: 1 },
            Cell { col: 5, row: -1 },
            Cell { col: -3, row: 0 },
            Cell { col: -9, row: 1 },
        ];
        v.sort();
        assert_eq!(
            v,
            [
                Cell { col: 5, row: -1 },
                Cell { col: -3, row: 0 },
                Cell { col: -9, row: 1 },
                Cell { col: 0, row: 1 },
            ],
            "cells no longer sort by (row, col); the writer emits placements in this order, so the \
             next export of an unchanged diagram would rewrite every line of the file"
        );
    }

    /// The six edge-sharers are exactly the cells at one step; the second ring
    /// is at 3r and cannot be mistaken for one.
    #[test]
    fn neighbours_are_the_six_cells_one_step_away_including_above_row_zero() {
        let l = Lattice::new(46.0, 1.0);
        for row in -3..=3 {
            for col in -3..=3 {
                let c = Cell { col, row };
                let (cx, cy) = l.centre(c);
                for n in c.neighbours() {
                    let (nx, ny) = l.centre(n);
                    let d = ((nx - cx).powi(2) + (ny - cy).powi(2)).sqrt();
                    assert!(
                        (d - l.step()).abs() < 1e-9,
                        "{n:?} is listed as a neighbour of {c:?} but sits {d} away rather than one \
                         step: contiguity would then report pieces the drawing does not have"
                    );
                }
            }
        }
    }
}
