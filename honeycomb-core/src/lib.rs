//! The honeycomb lattice, its model, and its RDF serialisation — with no UI.
//!
//! WHY THIS CRATE HAS NO YEW IN IT. Two consumers need this arithmetic and only
//! one of them is a browser. The editor drags tiles in WebAssembly; a static
//! site generator lays the same diagram out on the host and writes the same
//! Turtle, with no wasm anywhere. If the geometry lived with the component,
//! every generator that wanted a hexagon would have to link a UI framework to
//! get one — and the two would be free to disagree about which cell a point
//! falls in, which is exactly the bug that has no symptom until a drop lands
//! somewhere nobody expected.
//!
//! So: no yew, no web-sys, no wasm-bindgen. Ordinary `cargo test` on the host,
//! and `cargo test --target wasm32-unknown-unknown` is not needed to trust it.
//!
//! The vocabulary these terms serialise into is owned here too — see [`NS`].



// ---------------------------------------------------------------- geometry

/// Hex radius in board units, and the centre-to-centre step derived from it.
/// `GAP` matches the EONA-X portal's own `gap_factor` so a lattice drawn here
/// and one drawn there interlock rather than merely coexist.
pub const R: f64 = 52.0;
const GAP: f64 = 1.045;
/// `sqrt(3)` to full f64 precision, written out rather than computed, so this
/// is a `const` and so host and wasm32 cannot disagree about it.
const SQRT3: f64 = 1.732_050_807_568_877_2;
const STEP: f64 = SQRT3 * R * GAP;
const ROW: f64 = STEP * SQRT3 / 2.0;
const OX: f64 = 70.0;
const OY: f64 = 78.0;

/// Axial hex coordinates to a pixel centre. Multiplication and addition only.
pub fn axial(q: i32, r: i32) -> (f64, f64) {
    let (qf, rf) = (q as f64, r as f64);
    (OX + STEP * (qf + 0.5 * rf), OY + ROW * rf)
}

/// The inverse, plus cube rounding to the nearest cell. Used only while
/// dragging, i.e. only ever from a callback.
pub fn nearest_cell(x: f64, y: f64) -> (i32, i32) {
    let rf = (y - OY) / ROW;
    let qf = (x - OX) / STEP - 0.5 * rf;
    let sf = -qf - rf;
    let (mut q, mut r, s) = (qf.round(), rf.round(), sf.round());
    let (dq, dr, ds) = ((q - qf).abs(), (r - rf).abs(), (s - sf).abs());
    if dq > dr && dq > ds {
        q = -r - s;
    } else if dr > ds {
        r = -q - s;
    }
    (q as i32, r as i32)
}

pub fn hex_points(cx: f64, cy: f64, rad: f64) -> String {
    let dx = rad * SQRT3 / 2.0;
    let h = rad / 2.0;
    format!(
        "{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2} {:.2},{:.2}",
        cx, cy - rad, cx + dx, cy - h, cx + dx, cy + h,
        cx, cy + rad, cx - dx, cy + h, cx - dx, cy - h
    )
}

// ---------------------------------------------------------------- the model

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub q: i32,
    pub r: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    pub label: String,
    /// An ORDINAL, not a colour. The palette lives in CSS; a serialised
    /// `#2a78d6` would make the vocabulary carry presentation.
    pub slot: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub id: String,
    pub label: String,
    pub group: Option<String>,
    pub q: i32,
    pub r: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lattice {
    pub id: String,
    pub label: String,
    pub cells: Vec<Cell>,
    pub groups: Vec<Group>,
    pub tiles: Vec<Tile>,
}

impl Lattice {
    pub fn tile(&self, id: &str) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.id == id)
    }

    pub fn tile_at(&self, q: i32, r: i32) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.q == q && t.r == r)
    }

    pub fn group(&self, id: &str) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    pub fn has_cell(&self, q: i32, r: i32) -> bool {
        self.cells.iter().any(|c| c.q == q && c.r == r)
    }

    /// Every tile that moves when `id` is dragged: the tile itself, plus the
    /// rest of its group if it has one. A group sticks and follows its members.
    pub fn moving_set(&self, id: &str) -> Vec<String> {
        match self.tile(id).and_then(|t| t.group.clone()) {
            Some(g) => self
                .tiles
                .iter()
                .filter(|t| t.group.as_deref() == Some(g.as_str()))
                .map(|t| t.id.clone())
                .collect(),
            None => vec![id.to_string()],
        }
    }

    /// Can the moving set shift by `(dq, dr)`? `Ok(())`, or the tile and cell
    /// that refused it. Overlap is forbidden and so is leaving the lattice.
    pub fn check_move(&self, moving: &[String], dq: i32, dr: i32) -> Result<(), Refusal> {
        for id in moving {
            let Some(t) = self.tile(id) else { continue };
            let (nq, nr) = (t.q + dq, t.r + dr);
            if !self.has_cell(nq, nr) {
                return Err(Refusal {
                    moved: t.label.clone(),
                    blocker: None,
                    q: nq,
                    r: nr,
                });
            }
            if let Some(other) = self.tile_at(nq, nr) {
                if !moving.contains(&other.id) {
                    return Err(Refusal {
                        moved: t.label.clone(),
                        blocker: Some(other.label.clone()),
                        q: nq,
                        r: nr,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn apply_move(&self, moving: &[String], dq: i32, dr: i32) -> Lattice {
        let mut next = self.clone();
        for t in next.tiles.iter_mut() {
            if moving.contains(&t.id) {
                t.q += dq;
                t.r += dr;
            }
        }
        next
    }
}

/// Reported to the host so the status line can name *which* tile blocked
/// *which* cell, rather than saying "no".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub moved: String,
    pub blocker: Option<String>,
    pub q: i32,
    pub r: i32,
}

impl Refusal {
    pub fn message(&self) -> String {
        match &self.blocker {
            Some(b) => format!(
                "Can't drop there — {b} is already on {}.",
                cell_ref(self.q, self.r)
            ),
            None => format!("Can't drop there — {} is off the lattice.", self.moved),
        }
    }
}

/// A human cell name: column letter, row number. Purely for display.
pub fn cell_ref(q: i32, r: i32) -> String {
    let col = (b'A' + (q.clamp(0, 25)) as u8) as char;
    format!("{col}{}", r + 1)
}

// ---------------------------------------------------------------- serialiser

/// The vocabulary this component defines.
///
/// TWO NAMESPACES, NOT ONE, and the split is the point of the `{type}` segment:
/// the terms a diagram is written in live under `vocab/`, and the SHACL that
/// constrains them lives under `shapes/`. That mirrors how both consuming
/// repositories already separate `vocab/` from `shapes/` on disk, so a reader
/// who knows one knows the other.
///
/// THE IRI DOES NOT DEREFERENCE YET, and that is a deliberate, recorded state
/// rather than an oversight: semantic.ds-labs.org is not serving at the time of
/// writing. An IRI is an identifier first; `ns.ttl` in this repository is the
/// authoritative bytes, and standing the host up is a separate job that changes
/// nothing here when it happens. The alternative — minting the namespace from
/// whatever host happened to be convenient — is how a vocabulary ends up named
/// after a hosting decision it later regrets.
pub const NS: &str = "https://semantic.ds-labs.org/vocab/honeycomb#";

/// The namespace the component's SHACL shapes are written in.
pub const SHAPES_NS: &str = "https://semantic.ds-labs.org/shapes/honeycomb#";

impl Lattice {
    /// RDF/Turtle for the whole diagram. Pure string work with no web_sys and
    /// no clock, so the SSG can call it on the host and write the same bytes
    /// the browser would produce.
    pub fn to_turtle(&self) -> String {
        let mut s = String::new();
        s.push_str("@prefix hive: <");
        s.push_str(NS);
        s.push_str("> .\n");
        s.push_str("@prefix ex:   <https://example.org/honeycomb/demo#> .\n");
        s.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n");
        s.push_str(&format!(
            "ex:{} a hive:Lattice ;\n    rdfs:label \"{}\" ;\n    hive:cellCount {} .\n\n",
            self.id,
            self.label,
            self.cells.len()
        ));
        for g in &self.groups {
            s.push_str(&format!(
                "ex:{} a hive:Group ;\n    rdfs:label \"{}\" ;\n    hive:slot {} .\n\n",
                g.id, g.label, g.slot
            ));
        }
        for t in &self.tiles {
            s.push_str(&format!(
                "ex:{} a hive:Tile ;\n    rdfs:label \"{}\" ;\n    hive:inLattice ex:{} ;\n",
                t.id, t.label, self.id
            ));
            if let Some(g) = &t.group {
                s.push_str(&format!("    hive:inGroup ex:{g} ;\n"));
            }
            s.push_str(&format!("    hive:q {} ;\n    hive:r {} .\n\n", t.q, t.r));
        }
        s
    }
}

