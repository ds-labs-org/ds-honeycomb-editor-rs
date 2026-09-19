//! The document: what a diagram is, and everything it refuses to be.
//!
//! THE SHAPE OF THIS MODULE IS THE CONTRACT. Three properties the SHACL shapes
//! state for a file are made UNREPRESENTABLE here rather than checked:
//!
//!   * A pinned placement cannot carry content, because [`PinnedTile`] has no
//!     field to put it in. No line of code in this crate could cache a label
//!     into a layout.
//!   * Two tiles cannot share a cell at rest, because the cell is the key of the
//!     map [`Diagram`] stores placements in, and only this module may write it.
//!   * A standalone diagram cannot name an external source, and a pinned one
//!     cannot fail to, because the source lives inside the `Pinned` variant of
//!     [`Content`] and the mode IS that variant's discriminant.
//!
//! What remains checkable is checked once, in [`Diagram::try_new`], which is the
//! only constructor. A `Diagram` that exists serialises to a document that
//! validates, so the writer and the shapes cannot drift apart.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::lattice::Cell;
use crate::terms;

// ------------------------------------------------------------------- names

/// A validated kebab-case identifier. Parsing rather than a bare `String`
/// because every slug is also the last segment of its subject's IRI: an
/// unvalidated one produces a file the shapes reject, discovered a repository
/// away rather than at the call that built it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

/// Why a string is not a slug, carrying the string so the message can quote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadSlug {
    pub input: String,
    pub reason: &'static str,
}

impl Slug {
    /// Refuses anything outside `^[a-z0-9]+(-[a-z0-9]+)*$` — the same pattern
    /// `hsh:DiagramShape`, `hsh:TileShape` and `hsh:GroupShape` apply, written
    /// once here so a subject this crate builds can never fail it.
    pub fn parse(s: &str) -> Result<Slug, BadSlug> {
        let bad = |reason| {
            Err(BadSlug {
                input: s.to_string(),
                reason,
            })
        };
        if s.is_empty() {
            return bad("a slug is the last segment of an IRI and cannot be empty");
        }
        if s.starts_with('-') || s.ends_with('-') {
            return bad("a slug may not begin or end with a hyphen");
        }
        if s.contains("--") {
            return bad("a slug may not contain two hyphens in a row");
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return bad("a slug is lower-case ASCII letters, digits and single hyphens only");
        }
        Ok(Slug(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Tile identity. This IS the slug: a `TileId` is the map key, the last segment
/// of the tile's IRI and the value of its `hive:slug`, all at once, so the three
/// cannot disagree and `hsh:SlugMatchesIri` cannot fire on anything this crate
/// writes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileId(pub Slug);

/// Group identity, for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub Slug);

/// An IRI this crate stores and never interprets: a `hive:represents` subject, a
/// `hive:pinnedTo` source, a `hive:styleKey`. Opaque on purpose — the moment
/// this crate knew what one meant it would be one host's diagram format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Iri(pub String);

/// An `xsd:dateTime` WITH an explicit time zone, validated on the way in. A
/// zoneless timestamp parses as `xsd:dateTime` and still compares wrongly
/// against a source's last change once CI runs in another zone; the shapes
/// reject it with a pattern, and this type is what stops the writer producing
/// one at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timestamp(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadTimestamp {
    pub input: String,
    pub reason: &'static str,
}

impl Timestamp {
    pub fn parse(s: &str) -> Result<Timestamp, BadTimestamp> {
        let bad = |reason| {
            Err(BadTimestamp {
                input: s.to_string(),
                reason,
            })
        };
        // Deliberately a SHAPE check and not a calendar: whether 2026-02-30
        // exists is a question this crate has no business answering, and
        // answering it would mean shipping a date library to say so. What it
        // must catch is the missing zone, because that is the one defect that
        // silently compares wrongly instead of failing.
        let b = s.as_bytes();
        let digits = |from: usize, n: usize| {
            b.len() >= from + n && b[from..from + n].iter().all(|c| c.is_ascii_digit())
        };
        if !(digits(0, 4)
            && b.get(4) == Some(&b'-')
            && digits(5, 2)
            && b.get(7) == Some(&b'-')
            && digits(8, 2)
            && b.get(10) == Some(&b'T')
            && digits(11, 2)
            && b.get(13) == Some(&b':')
            && digits(14, 2)
            && b.get(16) == Some(&b':')
            && digits(17, 2))
        {
            return bad("not an xsd:dateTime of the form YYYY-MM-DDThh:mm:ss");
        }
        let mut i = 19;
        if b.get(i) == Some(&b'.') {
            i += 1;
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            if i == start {
                return bad("a fractional second was opened with '.' and no digits followed");
            }
        }
        match b.get(i) {
            Some(b'Z') if i + 1 == b.len() => Ok(Timestamp(s.to_string())),
            Some(b'+') | Some(b'-')
                if b.len() == i + 6
                    && digits(i + 1, 2)
                    && b.get(i + 3) == Some(&b':')
                    && digits(i + 4, 2) =>
            {
                Ok(Timestamp(s.to_string()))
            }
            _ => bad(
                "no explicit time zone: write Z or +hh:mm, because a zoneless timestamp compares \
                 wrongly the moment a reader sits in another zone",
            ),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ------------------------------------------------------------------- modes

/// Which of the two modes a diagram is. Stated, never inferred: a diagram is
/// still pinned when it has lost its last placement, and the editor must know
/// that in order to refuse to let a user type a label into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Pinned,
    Standalone,
}

impl Mode {
    pub const fn term(self) -> &'static str {
        match self {
            Mode::Pinned => terms::individual::PINNED,
            Mode::Standalone => terms::individual::STANDALONE,
        }
    }

    /// None for anything else. A third mode is a third renderer, so a reader
    /// that meets one must refuse rather than pick the nearest.
    pub fn from_term(local: &str) -> Option<Mode> {
        match local {
            terms::individual::PINNED => Some(Mode::Pinned),
            terms::individual::STANDALONE => Some(Mode::Standalone),
            _ => None,
        }
    }
}

/// How col and row map to the plane. One variant today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LatticeConvention {
    OddRPointyTop,
}

impl LatticeConvention {
    pub const fn term(self) -> &'static str {
        match self {
            LatticeConvention::OddRPointyTop => terms::individual::ODD_R_POINTY_TOP,
        }
    }

    /// None for an unimplemented convention. Approximating here draws the same
    /// integers as a different picture, with nothing anywhere to say so.
    pub fn from_term(local: &str) -> Option<LatticeConvention> {
        match local {
            terms::individual::ODD_R_POINTY_TOP => Some(LatticeConvention::OddRPointyTop),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- contents

/// A pinned placement's content. THERE IS NO CONTENT FIELD, and that is the
/// anti-drift property as a TYPE rather than a check: no line of code in this
/// crate could write a label into a pinned document, because there is nowhere to
/// put one. `sh:closed` on `hive:Placement` says the same about a hand-edit;
/// this says it before the file is ever written.
#[derive(Debug, Clone, PartialEq)]
pub struct PinnedTile {
    pub group: Option<GroupId>,
    pub represents: Iri,
}

/// A standalone tile's content. It carries no `cell` and no `slug`: the cell
/// lives in the diagram's occupancy index and the slug IS the `TileId` key, so
/// neither can drift out of step with a second copy of itself.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnTile {
    pub group: Option<GroupId>,
    pub label: String,
    pub comment: Option<String>,
    pub style_key: Option<Iri>,
    /// Predicates this vocabulary does not define, preserved VERBATIM. This is
    /// what makes `hsh:TileShape`'s openness real rather than nominal: a
    /// component that drops predicates it does not understand has an open shape
    /// and a closed implementation, and a host loses its own content on a round
    /// trip without being told.
    pub extra: Vec<Statement>,
}

/// A group carries no members and no anchor. Membership lives on the tiles and
/// the region is re-derived from their cells on every render, which is the whole
/// meaning of "the group follows": an anchor stored here is a second source of
/// truth that disagrees the moment one member is dragged.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub label: String,
    pub style_key: Option<Iri>,
    pub note: Option<String>,
    /// Same reason as [`OwnTile::extra`]: `hsh:GroupShape` is open.
    pub extra: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub predicate: Iri,
    pub object: Term,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Term {
    Iri(Iri),
    Literal {
        value: String,
        datatype: Option<Iri>,
        lang: Option<String>,
    },
}

/// Where the content comes from. The mode is this enum's discriminant, so the
/// three `hsh:ModeIsHonest` branches are unrepresentable rather than checked:
/// `Pinned` cannot exist without a `source`, a `PinnedTile` has nowhere to hold
/// a tile reference, and an `OwnTile` has nowhere to hold a `represents`.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Pinned {
        source: Iri,
        revision: Option<String>,
        tiles: BTreeMap<TileId, PinnedTile>,
    },
    Standalone {
        tiles: BTreeMap<TileId, OwnTile>,
    },
}

impl Content {
    pub fn mode(&self) -> Mode {
        match self {
            Content::Pinned { .. } => Mode::Pinned,
            Content::Standalone { .. } => Mode::Standalone,
        }
    }

    pub fn ids(&self) -> impl Iterator<Item = &TileId> {
        let it: Box<dyn Iterator<Item = &TileId> + '_> = match self {
            Content::Pinned { tiles, .. } => Box::new(tiles.keys()),
            Content::Standalone { tiles } => Box::new(tiles.keys()),
        };
        it
    }

    fn group_of(&self, id: &TileId) -> Option<&GroupId> {
        match self {
            Content::Pinned { tiles, .. } => tiles.get(id).and_then(|t| t.group.as_ref()),
            Content::Standalone { tiles } => tiles.get(id).and_then(|t| t.group.as_ref()),
        }
    }

    fn set_group(&mut self, id: &TileId, g: Option<GroupId>) {
        match self {
            Content::Pinned { tiles, .. } => {
                if let Some(t) = tiles.get_mut(id) {
                    t.group = g;
                }
            }
            Content::Standalone { tiles } => {
                if let Some(t) = tiles.get_mut(id) {
                    t.group = g;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- diagram

/// Everything needed to build a [`Diagram`], as a struct rather than a
/// nine-argument call. `cells` is separate from `content` because a cell is
/// stored EXACTLY ONCE in the finished `Diagram`; this is the one boundary where
/// the two must be matched up, and [`Diagram::try_new`] refuses any mismatch
/// instead of inventing a default.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagramSpec {
    pub slug: Slug,
    pub label: String,
    pub note: Option<String>,
    pub convention: LatticeConvention,
    pub generator: Option<String>,
    pub generated_at: Option<Timestamp>,
    pub groups: BTreeMap<GroupId, Group>,
    pub content: Content,
    pub cells: BTreeMap<TileId, Cell>,
}

/// One drawing. Every field is private: `occupancy` and `placement` are two
/// views of one fact and only this module may write either, so "two tiles in one
/// cell" is unrepresentable at rest rather than checked after the fact.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    slug: Slug,
    label: String,
    note: Option<String>,
    convention: LatticeConvention,
    generator: Option<String>,
    generated_at: Option<Timestamp>,
    groups: BTreeMap<GroupId, Group>,
    content: Content,
    occupancy: BTreeMap<Cell, TileId>,
    pub(crate) placement: BTreeMap<TileId, Cell>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    /// Names BOTH ids: "occupied" with one name is not actionable, because it is
    /// the pair that is the problem and the caller would have to go and find the
    /// other half itself.
    DuplicateCell {
        cell: Cell,
        first: TileId,
        second: TileId,
    },
    /// A tile naming a group the diagram does not declare — the same failure
    /// `hsh:GroupBelongsToItsDiagram` catches in a file, caught here first.
    UnknownGroup {
        tile: TileId,
        group: GroupId,
    },
    TileWithoutCell(TileId),
    CellWithoutTile(TileId),
    /// A diagram with no placements draws nothing, which is a file that lost its
    /// contents rather than an empty canvas. Refused at construction because the
    /// command set has no delete, so a diagram cannot legally become empty later.
    NoPlacements,
    EmptyLabel {
        subject: String,
    },
}

impl Diagram {
    /// The only constructor. Everything the shapes can reject about structure is
    /// rejected here, so a `Diagram` that exists serialises to a document that
    /// validates — the writer and the shapes cannot drift.
    pub fn try_new(spec: DiagramSpec) -> Result<Self, ModelError> {
        let DiagramSpec {
            slug,
            label,
            note,
            convention,
            generator,
            generated_at,
            groups,
            content,
            cells,
        } = spec;

        if label.trim().is_empty() {
            return Err(ModelError::EmptyLabel {
                subject: slug.as_str().to_string(),
            });
        }
        for (id, g) in &groups {
            if g.label.trim().is_empty() {
                return Err(ModelError::EmptyLabel {
                    subject: id.0.as_str().to_string(),
                });
            }
        }
        if let Content::Standalone { tiles } = &content {
            for (id, t) in tiles {
                if t.label.trim().is_empty() {
                    return Err(ModelError::EmptyLabel {
                        subject: id.0.as_str().to_string(),
                    });
                }
            }
        }

        // The two directions are checked separately because they are different
        // bugs: a tile with no cell is a tile the renderer can only pile at the
        // origin, and a cell with no tile is a reserved position nothing will
        // ever draw — and the next drop onto it would be refused for a reason
        // nothing on screen explains.
        for id in content.ids() {
            if !cells.contains_key(id) {
                return Err(ModelError::TileWithoutCell(id.clone()));
            }
        }
        for id in cells.keys() {
            if !content.ids().any(|c| c == id) {
                return Err(ModelError::CellWithoutTile(id.clone()));
            }
        }
        if cells.is_empty() {
            return Err(ModelError::NoPlacements);
        }

        for id in content.ids() {
            if let Some(g) = content.group_of(id)
                && !groups.contains_key(g)
            {
                return Err(ModelError::UnknownGroup {
                    tile: id.clone(),
                    group: g.clone(),
                });
            }
        }

        let mut occupancy: BTreeMap<Cell, TileId> = BTreeMap::new();
        for (id, cell) in &cells {
            if let Some(first) = occupancy.get(cell) {
                return Err(ModelError::DuplicateCell {
                    cell: *cell,
                    first: first.clone(),
                    second: id.clone(),
                });
            }
            occupancy.insert(*cell, id.clone());
        }

        Ok(Diagram {
            slug,
            label,
            note,
            convention,
            generator,
            generated_at,
            groups,
            content,
            occupancy,
            placement: cells,
        })
    }

    pub fn slug(&self) -> &Slug {
        &self.slug
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn convention(&self) -> LatticeConvention {
        self.convention
    }

    pub fn mode(&self) -> Mode {
        self.content.mode()
    }

    /// Some only in `Pinned`; the type, not a runtime check, is what guarantees
    /// a standalone diagram cannot name an external source.
    pub fn source(&self) -> Option<&Iri> {
        match &self.content {
            Content::Pinned { source, .. } => Some(source),
            Content::Standalone { .. } => None,
        }
    }

    pub fn revision(&self) -> Option<&str> {
        match &self.content {
            Content::Pinned { revision, .. } => revision.as_deref(),
            Content::Standalone { .. } => None,
        }
    }

    /// What the FILE said wrote it. The writer never echoes this back; see
    /// `write_turtle`.
    pub fn generator(&self) -> Option<&str> {
        self.generator.as_deref()
    }

    pub fn generated_at(&self) -> Option<&Timestamp> {
        self.generated_at.as_ref()
    }

    pub fn content(&self) -> &Content {
        &self.content
    }

    pub fn groups(&self) -> &BTreeMap<GroupId, Group> {
        &self.groups
    }

    pub fn group(&self, g: &GroupId) -> Option<&Group> {
        self.groups.get(g)
    }

    pub fn cell_of(&self, id: &TileId) -> Option<Cell> {
        self.placement.get(id).copied()
    }

    pub fn at(&self, cell: Cell) -> Option<&TileId> {
        self.occupancy.get(&cell)
    }

    /// In (row, col) order: the order the serialiser writes.
    pub fn cells(&self) -> impl Iterator<Item = (Cell, &TileId)> {
        self.occupancy.iter().map(|(c, id)| (*c, id))
    }

    pub fn group_of(&self, id: &TileId) -> Option<&GroupId> {
        self.content.group_of(id)
    }

    pub fn members(&self, g: &GroupId) -> Vec<TileId> {
        self.content
            .ids()
            .filter(|id| self.content.group_of(id) == Some(g))
            .cloned()
            .collect()
    }

    pub fn moving_set(&self, grabbed: &TileId, detach: bool) -> BTreeSet<TileId> {
        if !self.placement.contains_key(grabbed) {
            return BTreeSet::new();
        }
        match (detach, self.content.group_of(grabbed)) {
            (false, Some(g)) => self.members(&g.clone()).into_iter().collect(),
            _ => [grabbed.clone()].into_iter().collect(),
        }
    }

    /// Editing content is possible ONLY in standalone mode, and that is the
    /// return type rather than a rule: `Pinned` holds `PinnedTile`s, which have
    /// no content to hand out. Safe to expose as `&mut` because an `OwnTile`
    /// carries no cell, so nothing reachable through it can desynchronise the
    /// occupancy index.
    pub fn own_tile_mut(&mut self, id: &TileId) -> Option<&mut OwnTile> {
        match &mut self.content {
            Content::Standalone { tiles } => tiles.get_mut(id),
            Content::Pinned { .. } => None,
        }
    }

    pub fn group_mut(&mut self, g: &GroupId) -> Option<&mut Group> {
        self.groups.get_mut(g)
    }

    /// Flood fill over the 6-neighbour relation. REPORTED, never enforced: only
    /// a detach can fracture a group (a rigid translation is an isometry), and
    /// an editor that permits regrouping must permit the transiently split state
    /// between pulling a member out and putting it back.
    pub fn group_components(&self, g: &GroupId) -> Vec<BTreeSet<Cell>> {
        let mut left: BTreeSet<Cell> = self
            .members(g)
            .iter()
            .filter_map(|id| self.cell_of(id))
            .collect();
        let mut out = Vec::new();
        while let Some(&seed) = left.iter().next() {
            left.remove(&seed);
            let mut piece = BTreeSet::new();
            let mut stack = vec![seed];
            while let Some(c) = stack.pop() {
                piece.insert(c);
                for n in c.neighbours() {
                    if left.remove(&n) {
                        stack.push(n);
                    }
                }
            }
            out.push(piece);
        }
        out
    }

    /// Pairs of groups with edge-adjacent cells, so a host whose regions bleed
    /// past their cells can warn. Never a refusal: whether a merged region is
    /// wrong is a question about painting, and real diagrams already contain
    /// one.
    pub fn touching_groups(&self) -> Vec<(GroupId, GroupId)> {
        let mut out: BTreeSet<(GroupId, GroupId)> = BTreeSet::new();
        for (cell, id) in self.cells() {
            let Some(a) = self.content.group_of(id) else {
                continue;
            };
            for n in cell.neighbours() {
                let Some(other) = self.occupancy.get(&n) else {
                    continue;
                };
                let Some(b) = self.content.group_of(other) else {
                    continue;
                };
                if a < b {
                    out.insert((a.clone(), b.clone()));
                } else if b < a {
                    out.insert((b.clone(), a.clone()));
                }
            }
        }
        out.into_iter().collect()
    }

    /// In `Pinned` mode, the tiles whose `represents` is absent from `known`.
    /// REPORT these; never draw them as empty hexagons — an empty hexagon still
    /// reserves its cell, so the next drop onto it is refused for a reason
    /// nothing on screen explains.
    pub fn unresolved(&self, known: &BTreeSet<Iri>) -> Vec<TileId> {
        match &self.content {
            Content::Pinned { tiles, .. } => tiles
                .iter()
                .filter(|(_, t)| !known.contains(&t.represents))
                .map(|(id, _)| id.clone())
                .collect(),
            Content::Standalone { .. } => Vec::new(),
        }
    }

    // Only `rules.rs` may call these, and only through `apply`, which has
    // already run `check`. Moving a tile is two writes to two maps that are two
    // views of one fact; anything that could do one without the other would put
    // the diagram in a state no reader could draw.
    pub(crate) fn relocate(&mut self, moves: &[(TileId, Cell)]) {
        for (id, _) in moves {
            if let Some(old) = self.placement.get(id).copied() {
                self.occupancy.remove(&old);
            }
        }
        for (id, to) in moves {
            self.placement.insert(id.clone(), *to);
            self.occupancy.insert(*to, id.clone());
        }
    }

    pub(crate) fn occupant(&self, cell: &Cell) -> Option<&TileId> {
        self.occupancy.get(cell)
    }

    pub(crate) fn has_group(&self, g: &GroupId) -> bool {
        self.groups.contains_key(g)
    }

    pub(crate) fn set_group(&mut self, id: &TileId, g: Option<GroupId>) {
        self.content.set_group(id, g);
    }
}
