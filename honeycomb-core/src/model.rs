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
//!
//! THAT SENTENCE WAS FALSE FOR A WHILE, and the gap is worth recording because
//! it is the kind this module claims not to have. The writer mints a placement's
//! subject as `at-{slug}` and nothing checked those locals against each other,
//! so a tile called `at-hall` beside a tile called `hall` was accepted here and
//! written as two different subjects with one IRI — refused by this crate's own
//! reader and by `hsh:PlacementShape`'s closedness. `ModelError::SubjectCollision`
//! is the repair.

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

/// Link identity, for the same reason.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(pub Slug);

/// An IRI this crate stores and never interprets: a `hive:represents` subject, a
/// `hive:pinnedTo` source, a `hive:styleKey`. Opaque on purpose — the moment
/// this crate knew what one meant it would be one host's diagram format.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Iri(pub String);

// -------------------------------------------------------------------- text

/// Human-readable text, and the language it is written in when the document
/// said one.
///
/// A PLAIN `String` WAS THE LAST SILENT LOSS ON IMPORT. `rdfs:label
/// "Mairie"@fr` loaded, the tag went nowhere, and the next save wrote
/// `rdfs:label "Mairie"` — a document the author did not write, with nothing
/// anywhere to say a language claim had been deleted from it. Worse, the loss
/// was ARBITRARY: the same `@fr` on a predicate this vocabulary does not
/// define came back untouched, because `ttl::read::extras` keeps the raw
/// statement, so whether an author's tag survived depended on which predicate
/// they had put it on.
///
/// WHY THIS IS NOT ON EVERY STRING IN THE MODEL. `hive:note`, `hive:slug`,
/// `hive:pinnedRevision`, `hive:generator`, `hive:formatVersion` and
/// `hive:generatedAt` each pin an explicit `sh:datatype` in `shapes.ttl` — and
/// a language-tagged literal is `rdf:langString`, which is neither
/// `xsd:string` nor `xsd:dateTime`. Carrying a tag on any of them would let a
/// `Diagram` exist that serialises to a document this crate's OWN shapes
/// reject, which is precisely the drift this module's header promises cannot
/// happen. `rdfs:label` and `rdfs:comment` are the two the shapes constrain
/// with no datatype at all, so they are exactly the two that carry one. A
/// tagged `hive:note` is a real wish and it is a VOCABULARY change — the range
/// in `ns.ttl` and the shape in `shapes.ttl` both say `xsd:string` — not a
/// change a reader may make on its own.
///
/// THE TAG IS VALIDATED AT CONSTRUCTION, for `Slug`'s reason one type up: the
/// tag is written back into the document verbatim, so an unparseable one
/// produces a file only this crate can read. The lexer in `ttl::read` scans a
/// tag as `[A-Za-z0-9-]+` and is documented as deliberately tolerant, which is
/// wider than Turtle's own `LANGTAG` — `"x"@fr-` lexes here and is refused by
/// a strict parser. This is where that gap is closed.
///
/// `Display` WRITES THE TEXT AND NOT THE TAG. Every host that draws a label
/// wants the words; the tag is metadata about them, and a heading reading
/// "Mairie@fr" would be a bug in every renderer at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Text {
    // Private for `Slug`'s reason: `lang` may only ever hold something the
    // writer can emit, and public fields would put that back in every caller's
    // hands.
    value: String,
    lang: Option<String>,
}

/// Why a string is not a language tag, carrying it so the message can quote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadLang {
    pub input: String,
    pub reason: &'static str,
}

impl Text {
    /// Text in no stated language — what every document this crate wrote before
    /// `Text` existed carries, and what a host building a diagram fresh means
    /// unless it says otherwise.
    pub fn plain(value: impl Into<String>) -> Text {
        Text {
            value: value.into(),
            lang: None,
        }
    }

    /// Text in a stated language. Refuses anything outside Turtle's `LANGTAG`,
    /// `[a-zA-Z]+ ('-' [a-zA-Z0-9]+)*` — see the type's own doc for why a
    /// tolerant lexer is not licence to hold a tag that cannot be written back.
    ///
    /// It checks the SHAPE and not the registry: whether `fr-Latn-CH` names a
    /// locale anybody has is a question this crate has no business answering,
    /// and answering it would mean shipping BCP 47's registry to say so. What
    /// it must catch is the tag that makes the next export unparseable.
    pub fn tagged(value: impl Into<String>, lang: &str) -> Result<Text, BadLang> {
        let bad = |reason| {
            Err(BadLang {
                input: lang.to_string(),
                reason,
            })
        };
        let mut parts = lang.split('-');
        let primary = parts.next().unwrap_or_default();
        if primary.is_empty() || !primary.chars().all(|c| c.is_ascii_alphabetic()) {
            return bad("a language tag opens with one or more ASCII letters, as in `fr` or `en`");
        }
        for sub in parts {
            if sub.is_empty() || !sub.chars().all(|c| c.is_ascii_alphanumeric()) {
                return bad(
                    "every `-` in a language tag introduces a further run of ASCII letters or \
                     digits, as in `fr-CH` or `de-DE-1901`; an empty or trailing one is not a tag",
                );
            }
        }
        Ok(Text {
            value: value.into(),
            lang: Some(lang.to_string()),
        })
    }

    /// The words. What a renderer draws, whatever language they are in.
    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn lang(&self) -> Option<&str> {
        self.lang.as_deref()
    }

    /// Nothing but whitespace, which is the test `try_new` and the writer both
    /// apply: a label of spaces draws an empty hexagon, and `rdfs:label "  "`
    /// claims a name that is not one.
    pub fn is_blank(&self) -> bool {
        self.value.trim().is_empty()
    }
}

/// Untagged, for the ordinary case. The conversion exists so that a host with
/// nothing to say about language says nothing, rather than reaching for a
/// constructor to express the absence of a claim.
impl From<&str> for Text {
    fn from(s: &str) -> Text {
        Text::plain(s)
    }
}

impl From<String> for Text {
    fn from(s: String) -> Text {
        Text::plain(s)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

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

/// A bare `MAJOR.MINOR.PATCH`, the shape `hive:formatVersion` carries and the
/// shape this crate's own `CARGO_PKG_VERSION` is. Not general SemVer — no
/// pre-release, no build metadata — because the only two things that will
/// ever be compared against each other with it are this crate's own releases,
/// which have never needed either.
///
/// `#[derive(PartialOrd, Ord)]` ON THE FIELDS IN THIS ORDER IS THE WHOLE
/// COMPARISON. `(0, 4, 0) > (0, 3, 9)` and `(1, 0, 0) > (0, 9, 9)` fall out of
/// lexicographic tuple ordering for free, which is exactly SemVer precedence
/// for a version with no pre-release component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadVersion {
    pub input: String,
    pub reason: &'static str,
}

impl Version {
    pub fn parse(s: &str) -> Result<Version, BadVersion> {
        let bad = |reason| {
            Err(BadVersion {
                input: s.to_string(),
                reason,
            })
        };
        let mut parts = s.split('.');
        let (Some(maj), Some(min), Some(pat), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return bad("not MAJOR.MINOR.PATCH: expected exactly two '.' separators");
        };
        let digits = |p: &str| p.parse::<u32>().is_ok() && !p.is_empty();
        if !(digits(maj) && digits(min) && digits(pat)) {
            return bad("each of MAJOR, MINOR and PATCH must be a plain non-negative integer");
        }
        Ok(Version {
            major: maj.parse().expect("checked by `digits` above"),
            minor: min.parse().expect("checked by `digits` above"),
            patch: pat.parse().expect("checked by `digits` above"),
        })
    }

    /// This crate's OWN version, parsed once from the same `CARGO_PKG_VERSION`
    /// `crate::TBOX_VERSION` is — the string `honeycomb-core/tests/vocabulary.rs`
    /// already pins equal to `ns.ttl`'s own `hive:tboxVersion`. The `expect` is
    /// safe rather than defensive: cargo itself refuses to build a package
    /// whose own version is not three dot-separated integers, so a failure here
    /// would mean this crate somehow built at all with a version cargo would
    /// have rejected.
    pub fn current() -> Version {
        Version::parse(crate::TBOX_VERSION)
            .expect("this crate's own CARGO_PKG_VERSION is not MAJOR.MINOR.PATCH")
    }

    /// The SAME major version as `current`, and strictly a LATER minor. A
    /// later MAJOR is refused before a `Diagram` exists at all — see
    /// `ReadError::UnsupportedFormatVersion` — so by the time anything calls
    /// this, a later major is not a possibility this needs to consider. A
    /// later PATCH with the same minor is deliberately NOT "newer" by this
    /// crate's own convention: a minor bump is what may add an optional term
    /// a reader does not implement yet, and a patch release changes no term
    /// at all.
    pub fn is_newer_minor_than(&self, current: Version) -> bool {
        self.major == current.major && self.minor > current.minor
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
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
    pub label: Text,
    pub comment: Option<Text>,
    pub style_key: Option<Iri>,
    /// Predicates this vocabulary does not define, preserved VERBATIM. This is
    /// what makes `hsh:TileShape`'s openness real rather than nominal: a
    /// component that drops predicates it does not understand has an open shape
    /// and a closed implementation, and a host loses its own content on a round
    /// trip without being told.
    pub extra: Vec<Statement>,
}

/// What [`Diagram::own_tile_mut`] hands out: every field of [`OwnTile`] a live
/// edit may touch, `group` EXCLUDED BY THE TYPE rather than by a rule a caller
/// has to already know.
///
/// `group` IS NOT HERE, AND THAT OMISSION IS THE WHOLE POINT OF THIS STRUCT.
/// `OwnTile.group` stays a `pub` field — a host builds one with a struct
/// literal, group included, every time it constructs a fresh tile for
/// `Command::Add` or a `DiagramSpec`, and `Diagram::try_new` is what validates
/// the result. But a tile already ON THE BOARD has its group governed by
/// `Command::Attach` and `Command::Detach`, which `Diagram::check` enforces
/// decision 4 through: the last member of a linked group may not leave it.
/// `own_tile_mut` used to return `&mut OwnTile` directly, and because the
/// field was `pub`, a caller holding that reference could write `group = None`
/// on a linked group's only member and empty it with no command, no check, no
/// rejection at all — a working exploit against exactly that fixture is
/// recorded, passing, in the commit that introduced this type, along with the
/// compiler error the same code produces now that it does not. A live edit
/// gets a view with nowhere to put that write instead;
/// `own_tile_mut_still_edits_content_on_the_last_member_of_a_linked_group` in
/// `honeycomb-core/tests/group_ends.rs` pins that the capability a legitimate
/// caller needs — editing content, even on the last member of a linked group
/// — survived the narrowing.
pub struct OwnTileEdit<'a> {
    pub label: &'a mut Text,
    pub comment: &'a mut Option<Text>,
    pub style_key: &'a mut Option<Iri>,
    pub extra: &'a mut Vec<Statement>,
}

/// A placement's content, on its way IN.
///
/// A SUM AND NOT TWO COMMANDS, because the thing that must never happen is a
/// pinned diagram acquiring a tile with a label. [`Content`] already makes that
/// unrepresentable at rest; this is the same shape at the doorway, so an Add can
/// be checked against `Content::mode()` once instead of the command set growing
/// a variant per mode and the check growing an arm per pair.
///
/// `Own` is boxed. An [`OwnTile`] carries a label, a comment, a style key and a
/// vector of preserved statements; a [`PinnedTile`] carries two fields. Without
/// the box every `Command` in the enum — including the `Translate` a drag
/// constructs on every pointer move — is as large as the biggest one, on a hot
/// path in a diagram that is pinned.
#[derive(Debug, Clone, PartialEq)]
pub enum NewTile {
    Pinned(PinnedTile),
    Own(Box<OwnTile>),
}

impl NewTile {
    pub fn mode(&self) -> Mode {
        match self {
            NewTile::Pinned(_) => Mode::Pinned,
            NewTile::Own(_) => Mode::Standalone,
        }
    }

    pub fn group(&self) -> Option<&GroupId> {
        match self {
            NewTile::Pinned(t) => t.group.as_ref(),
            NewTile::Own(t) => t.group.as_ref(),
        }
    }

    /// `Some` only in standalone: a [`PinnedTile`] has nowhere to put a label,
    /// which is the whole point of it.
    ///
    /// THE TEXT, NOT THE WHOLE [`Text`], for [`Diagram::label`]'s reason: the
    /// one caller is `rules.rs` asking whether an Add carries a name at all,
    /// and a language tag is not part of that question.
    pub fn label(&self) -> Option<&str> {
        match self {
            NewTile::Pinned(_) => None,
            NewTile::Own(t) => Some(t.label.as_str()),
        }
    }
}

/// One end of a link: a PLACEMENT, or a GROUP.
///
/// AN ENUM AND NOT A `TileId` WITH AN OPTIONAL GROUP BESIDE IT, and not a
/// single id with a kind flag. The two ends are alternatives, never a pair and
/// never one shading into the other, so the sum type is the shape that makes
/// "a tile end that also names a group" unrepresentable rather than checked —
/// the property this module's header claims for every other alternative in it.
/// A flag-plus-id would be the same information with one extra state nobody
/// wants: a group id flagged as a tile resolves to a tile the diagram does not
/// have, which is a `LinkToNowhere` invented by the representation.
///
/// IT IS NOT `Iri`, EITHER, and that is the other tempting shape: the document
/// writes an IRI at each end, so an endpoint could have been the IRI and the
/// reader could have stopped resolving. That would push the resolution out to
/// every consumer — each free to strip `at-` the way the reader used to, each
/// free to get it wrong differently — and it would lose the one thing the
/// reader is for, which is refusing an end that names nothing (see
/// `ReadError::UnresolvedLinkEnd`).
///
/// THE VARIANTS ARE THE CONSTRUCTORS. There is deliberately no `From<TileId>`
/// or `From<GroupId>`: an end's kind is the most consequential thing about it —
/// it decides whether a line meets one hexagon or a whole region, and it is
/// what the writer spells with or without `at-` — so a call site that builds
/// one should have to say which it means.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    Tile(TileId),
    Group(GroupId),
}

impl Endpoint {
    /// `Some` only for a placement end. A host that draws differently at the
    /// two kinds asks this rather than matching, so that adding a third kind —
    /// if a diagram ever gains something else a line can meet — is a compile
    /// error in exactly one place instead of a silent `_` arm in every host.
    pub fn tile(&self) -> Option<&TileId> {
        match self {
            Endpoint::Tile(t) => Some(t),
            Endpoint::Group(_) => None,
        }
    }

    pub fn group(&self) -> Option<&GroupId> {
        match self {
            Endpoint::Group(g) => Some(g),
            Endpoint::Tile(_) => None,
        }
    }

    /// The name, whichever kind this is. What a refusal quotes and what a
    /// status line reads out: both ids ARE their slugs — see [`TileId`] — so
    /// this loses nothing except the need for the caller to match twice to
    /// print one word.
    pub fn slug(&self) -> &Slug {
        match self {
            Endpoint::Tile(t) => &t.0,
            Endpoint::Group(g) => &g.0,
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug().as_str())
    }
}

/// A STATED CONNECTION BETWEEN TWO PLACEMENTS, and nothing about the layout.
///
/// THE README SAID THERE WOULD BE NONE OF THESE — "not a graph editor; there
/// are no edges, no ports and no routing" — and that is still true of the
/// LATTICE: a link moves nothing, reserves no cell, and a diagram with no links
/// is exactly the diagram it was. What it adds is the one thing a honeycomb
/// cannot say by arrangement, because adjacency on a packed grid is a
/// consequence of packing rather than of meaning.
///
/// DIRECTED, because undirected is a special case of directed and not the
/// reverse: a host that means "these are related" draws the same line without an
/// arrowhead, and one that means "this calls that" cannot recover the direction
/// from a document that never kept it.
///
/// EITHER END MAY NOW BE A WHOLE GROUP — see [`Endpoint`] — and the doc summary
/// above is left saying "two placements" only in the sense that a line still
/// ends at two CELLS: a group end resolves to whichever of its members faces
/// the other end, recomputed on every render rather than stored, so the region
/// is met at the edge pointing at what it connects to. [`Diagram::link_anchors`]
/// is that resolution and [`anchors`] is the same rule over cells a drag is
/// only proposing.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: Endpoint,
    pub to: Endpoint,
    /// Most lines say enough by existing.
    pub label: Option<Text>,
    pub routing: Routing,
    pub style_key: Option<Iri>,
    /// Same reason as [`OwnTile::extra`]: `hsh:LinkShape` is open.
    pub extra: Vec<Statement>,
}

/// How a link gets from one cell to the other.
///
/// GEOMETRY, NOT APPEARANCE, which is why it is a typed term when colour is an
/// opaque [`Iri`]: where a line goes is a question about the lattice, and two
/// hosts drawing one document must put it in the same place or they are drawing
/// different diagrams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Routing {
    /// Centre to centre, beneath the tiles. The default, so a document with no
    /// opinion does not have to say so.
    #[default]
    Straight,
    /// Along the comb's own edges, never crossing a cell.
    LatticePath,
    /// One curve, bowed clear of what lies between.
    Arc,
}

impl Routing {
    pub fn term(self) -> &'static str {
        match self {
            Routing::Straight => "straight",
            Routing::LatticePath => "latticePath",
            Routing::Arc => "arc",
        }
    }

    pub fn from_term(local: &str) -> Option<Routing> {
        match local {
            "straight" => Some(Routing::Straight),
            "latticePath" => Some(Routing::LatticePath),
            "arc" => Some(Routing::Arc),
            _ => None,
        }
    }
}

/// A group carries no members and no anchor. Membership lives on the tiles and
/// the region is re-derived from their cells on every render, which is the whole
/// meaning of "the group follows": an anchor stored here is a second source of
/// truth that disagrees the moment one member is dragged.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub label: Text,
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

/// Connected pieces of a set of cells, under the 6-neighbour relation.
///
/// A FREE FUNCTION BECAUSE THE VIEW NEEDS IT OVER CELLS THAT ARE NOT COMMITTED.
/// [`Diagram::group_components`] reads `self.cell_of`, so it can only ever
/// answer about the arrangement as stored — and while a pointer is down the
/// arrangement on screen is the one the drag is proposing. A component that
/// wants to say "this group is about to be in two pieces" BEFORE the drop has
/// to run the fill over the previewed cells, and the only alternative to
/// exposing this is a second flood fill in the view, free to disagree with the
/// one here.
pub fn components(mut left: BTreeSet<Cell>) -> Vec<BTreeSet<Cell>> {
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

/// Where a line between two sets of cells meets each of them: the CLOSEST PAIR,
/// or `None` when either side is empty.
///
/// THE ANCHOR RULE, WRITTEN ONCE. A link endpoint may name a whole group, and a
/// line has to meet that region somewhere; it meets it at whichever member cell
/// faces the other end, so it touches the edge pointing at what it connects to
/// rather than emerging from an arbitrary member.
///
/// WHY THE NEAREST CELL AND NOT THE CENTROID. All three routings keep working
/// with a real cell and only one of them keeps working with a point:
/// `Routing::LatticePath` walks the comb's own edges from cell to cell, and a
/// centroid is not a cell — it can land in a hole, on a member, or outside the
/// region entirely, and `route` has nothing to start from. A member cell is
/// always somewhere the lattice actually goes.
///
/// WHY A PAIR AND NOT TWO NEAREST-MEMBER QUESTIONS. With a group at both ends
/// each anchor is defined relative to the other, so asking them separately is
/// circular. Minimising over the pair is the only reading that terminates, and
/// it is also the only symmetric one: a line whose ends disagreed about which
/// was measured first would be drawn in two places depending on which end a
/// host started from, and two hosts drawing one document must not do that.
///
/// A FREE FUNCTION, for [`components`]' reason exactly. While a pointer is down
/// the arrangement on screen is the one the drag is PROPOSING, so a view that
/// wants to draw the line the drop would produce has to ask about cells no
/// `Diagram` holds. [`Diagram::link_anchors`] is this same function over what
/// is committed; the alternative to exposing it is a second nearest-member
/// search in the view, free to disagree with this one.
///
/// TIES ARE BROKEN BY `BTreeSet` ORDER, which is (row, col) — the order the
/// serialiser writes. Deliberately deterministic rather than deliberately
/// meaningful: when two members are exactly as close, any choice draws an
/// equally correct line, and the only unacceptable outcome is the line moving
/// between renders of an unchanged document.
pub fn anchors(from: &BTreeSet<Cell>, to: &BTreeSet<Cell>) -> Option<(Cell, Cell)> {
    let mut best: Option<(i32, Cell, Cell)> = None;
    for a in from {
        for b in to {
            let d = a.to_axial().distance(b.to_axial());
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, *a, *b));
            }
        }
    }
    best.map(|(_, a, b)| (a, b))
}

/// The empty cells a set of cells ENCLOSES — the holes in it.
///
/// A GROUP WITH A HOLE SHOULD READ AS ONE SHAPE, and without this it reads as a
/// ring: the region is the union of its members' grown hexagons, so a cell that
/// no member occupies is a gap in the ground however completely it is surrounded.
///
/// ENCLOSED, NOT MERELY EMPTY, and that distinction is the whole function. A
/// flood fill from OUTSIDE the set's bounding box reaches every empty cell that
/// has a way out; what it cannot reach is enclosed. So a donut is filled and a
/// group in two separate pieces is left visibly in two pieces — which is the
/// "fracture is reported, never hidden" rule arriving in the geometry, rather
/// than a distance threshold that would have to guess where a gap stops being a
/// hole.
///
/// The box is grown by one ring so the fill always has somewhere to start.
pub fn holes(cells: &BTreeSet<Cell>) -> BTreeSet<Cell> {
    let Some(first) = cells.iter().next() else {
        return BTreeSet::new();
    };
    let (mut min_col, mut max_col) = (first.col, first.col);
    let (mut min_row, mut max_row) = (first.row, first.row);
    for c in cells {
        min_col = min_col.min(c.col);
        max_col = max_col.max(c.col);
        min_row = min_row.min(c.row);
        max_row = max_row.max(c.row);
    }
    let (min_col, max_col) = (min_col - 1, max_col + 1);
    let (min_row, max_row) = (min_row - 1, max_row + 1);
    let inside =
        |c: &Cell| c.col >= min_col && c.col <= max_col && c.row >= min_row && c.row <= max_row;

    // Flood the empty space from the box's border inwards.
    let mut outside: BTreeSet<Cell> = BTreeSet::new();
    let mut stack: Vec<Cell> = Vec::new();
    for col in min_col..=max_col {
        for row in [min_row, max_row] {
            stack.push(Cell { col, row });
        }
    }
    for row in min_row..=max_row {
        for col in [min_col, max_col] {
            stack.push(Cell { col, row });
        }
    }
    while let Some(c) = stack.pop() {
        if cells.contains(&c) || !inside(&c) || !outside.insert(c) {
            continue;
        }
        stack.extend(c.neighbours());
    }

    let mut out = BTreeSet::new();
    for col in min_col..=max_col {
        for row in min_row..=max_row {
            let c = Cell { col, row };
            if !cells.contains(&c) && !outside.contains(&c) {
                out.insert(c);
            }
        }
    }
    out
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
    pub label: Text,
    pub note: Option<String>,
    pub convention: LatticeConvention,
    pub generator: Option<String>,
    pub generated_at: Option<Timestamp>,
    pub groups: BTreeMap<GroupId, Group>,
    pub content: Content,
    pub cells: BTreeMap<TileId, Cell>,
    /// The connections this diagram draws.
    pub links: BTreeMap<LinkId, Link>,
    /// Predicates this vocabulary does not define, on the DIAGRAM subject,
    /// preserved verbatim.
    ///
    /// `hsh:DiagramShape` is deliberately not closed — "a host hangs its own
    /// predicates on a diagram, and typo-catching on an open class is the host's
    /// business" — and for its whole first life this crate read them, dropped
    /// them, and wrote a document without them. Groups and tiles had `extra`
    /// from the start; the diagram did not, so a host that put a sheet number on
    /// it lost the sheet number the first time anybody dragged a hexagon, with
    /// no error anywhere. [`OwnTile::extra`]'s own doc names exactly that
    /// failure and this is the third place it had to be fixed.
    pub extra: Vec<Statement>,
}

/// One drawing. Every field is private: `occupancy` and `placement` are two
/// views of one fact and only this module may write either, so "two tiles in one
/// cell" is unrepresentable at rest rather than checked after the fact.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    slug: Slug,
    label: Text,
    note: Option<String>,
    convention: LatticeConvention,
    generator: Option<String>,
    generated_at: Option<Timestamp>,
    groups: BTreeMap<GroupId, Group>,
    content: Content,
    links: BTreeMap<LinkId, Link>,
    extra: Vec<Statement>,
    /// Whole subjects this document contains that no walk from THIS diagram
    /// reaches — an unplaced tile, a host's own subject nothing here points
    /// at, an ontology header. Distinct from `extra`, which is predicates on
    /// THIS diagram's own subject: these are OTHER subjects entirely, each
    /// with its own IRI and its own predicate list, that `hive:placement`,
    /// `hive:group`, `hive:tile` and `hive:link` never reach from here.
    ///
    /// ALWAYS EMPTY ON A `Diagram` BUILT BY HAND — every fixture in this
    /// workspace among them — because there is no unreached subject to have
    /// until one has been read out of a file that already contained one.
    /// `ttl::read::read_turtle_all` is the only thing that ever populates it,
    /// through `set_unreached`, and only when the document it parsed held
    /// exactly one diagram; see that function's own doc for why two or more
    /// still loses them.
    unreached: Vec<(Iri, Vec<Statement>)>,
    /// What `hive:formatVersion` this document DECLARED, if it declared one at
    /// all — never what this crate is about to write. Set the same way
    /// `unreached` is: `None` from `try_new`, always, because a `Diagram` a
    /// host is building fresh has no declared version to have yet; only
    /// `ttl::read::build` ever calls `set_format_version`, once, right after a
    /// file's own claim has already been checked against `Version::current()`
    /// and found acceptable (a document naming a newer MAJOR never reaches
    /// this field at all — see `ReadError::UnsupportedFormatVersion`).
    ///
    /// THE WRITER NEVER READS THIS FIELD. `write_turtle` always states
    /// `Version::current()`, for the identical reason `hive:generator` always
    /// states this crate's own name and version rather than echoing back what
    /// `generator()` returns: a file this crate is about to re-save is a file
    /// THIS build is now answering for, whatever an earlier build claimed.
    format_version: Option<Version>,
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
    /// contents rather than an empty canvas.
    ///
    /// IT USED TO SAY "the command set has no delete, so a diagram cannot
    /// legally become empty later". The command set has one now, and
    /// `Rejection::LastPlacement` is what re-establishes the sentence: a Remove
    /// that would empty the board is refused, so this stays a construction-time
    /// error and the property it protects survives.
    NoPlacements,
    /// Two of this diagram's subjects would be written with ONE IRI.
    ///
    /// The writer mints a placement's subject as `at-{slug}`, so a tile called
    /// `at-hall` collides with the placement of a tile called `hall`; and in
    /// standalone mode a tile and a group sharing a slug collide outright. Both
    /// produce a document that this crate's OWN READER refuses and that
    /// `hsh:PlacementShape`'s closedness rejects — so a `Diagram` that exists
    /// would not have serialised to a document that validates, which is the one
    /// promise this module's header makes.
    SubjectCollision {
        local: String,
        first: String,
        second: String,
    },
    /// A link naming a tile this diagram does not place, or a group it does not
    /// declare. One variant for both because it is one mistake — an end that
    /// points at something this document does not contain — and the
    /// [`Endpoint`] says which kind was meant without a second variant to say
    /// it again.
    LinkToNowhere {
        link: LinkId,
        end: Endpoint,
    },
    /// A link to a group with NO MEMBERS.
    ///
    /// The group exists and the link still cannot be drawn: the region is
    /// derived from its members' cells, so a group with none has no cell for a
    /// line to meet. `hsh:LinkShape` already writes this argument down about a
    /// non-placement end — "a line the renderer has no cell to draw from draws
    /// nothing, which looks exactly like a link that was never there" — and
    /// that argument is the reason the widening admits a POPULATED group and
    /// refuses an empty one rather than admitting groups outright.
    ///
    /// LIVE, NOT HYPOTHETICAL: production declares fourteen groups and draws
    /// eight of them, so six empty groups are sitting in a real document right
    /// now waiting to be picked as an endpoint.
    LinkToEmptyGroup {
        link: LinkId,
        group: GroupId,
    },
    /// A link between a group and one of its own members.
    ///
    /// Containment already states the relationship, and a line from a whole to
    /// its own part adds nothing a reader can use. It is also the likeliest
    /// mis-drag the new gesture has: start the line on a ground and release it
    /// on one of that ground's own hexagons. Named in both directions by one
    /// variant, because "these two are the same thing" has no direction.
    LinkToOwnMember {
        link: LinkId,
        group: GroupId,
        tile: TileId,
    },
    /// A link from an end to itself: no direction, no length, nothing to draw.
    LinkToItself(LinkId),
    EmptyLabel {
        subject: String,
    },
}

/// Every subject local name a diagram built from these ingredients will mint
/// when it is written, paired with a short description of what it belongs to.
///
/// THE SINGLE DEFINITION OF WHAT THIS CRATE MINTS AS A SUBJECT. `try_new`
/// calls this with a `DiagramSpec`'s own pieces, before a `Diagram` exists, to
/// validate a whole document at once. [`Diagram::would_collide`] calls it
/// again with `self`'s fields, on a diagram that already exists — and is
/// already known to be collision-free, because nothing reaches a live
/// `Diagram` except through this same check — to answer the identical
/// question about ONE prospective new local name before a command that would
/// mint it is applied. Two call sites and one list, so the list cannot drift
/// out of step with itself the way the writer once drifted from this check
/// entirely (see the module header).
///
/// The minted set is the diagram's own slug, every group's, every placement's
/// `at-{slug}` — and, in standalone mode only, every tile's, because a pinned
/// placement names no tile subject at all.
fn minted_subjects(
    slug: &Slug,
    groups: &BTreeMap<GroupId, Group>,
    content: &Content,
    links: &BTreeMap<LinkId, Link>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    out.push((slug.as_str().to_string(), "the diagram".to_string()));
    for id in groups.keys() {
        out.push((
            id.0.as_str().to_string(),
            format!("group {}", id.0.as_str()),
        ));
    }
    if let Content::Standalone { tiles } = content {
        for id in tiles.keys() {
            out.push((id.0.as_str().to_string(), format!("tile {}", id.0.as_str())));
        }
    }
    for id in content.ids() {
        out.push((
            format!("{}{}", crate::ttl::PLACEMENT_PREFIX, id.0.as_str()),
            format!("the placement of {}", id.0.as_str()),
        ));
    }
    for id in links.keys() {
        out.push((id.0.as_str().to_string(), format!("link {}", id.0.as_str())));
    }
    out
}

impl Diagram {
    /// The only constructor. Everything the shapes can reject about structure is
    /// rejected here, so a `Diagram` that exists serialises to a document that
    /// validates — the writer and the shapes cannot drift.
    pub fn try_new(spec: DiagramSpec) -> Result<Self, ModelError> {
        let DiagramSpec {
            extra,
            links,
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

        if label.is_blank() {
            return Err(ModelError::EmptyLabel {
                subject: slug.as_str().to_string(),
            });
        }
        // A GROUP MAY HAVE NO LABEL, and a tile may not. The ground IS the
        // signal for a group — a coloured region with a heading is sometimes
        // saying the same thing twice, and a diagram whose clusters are obvious
        // should be allowed to stay quiet. A TILE with no label draws as an
        // empty hexagon, which is nothing at all, so that check stays below.
        let _ = &groups;
        if let Content::Standalone { tiles } = &content {
            for (id, t) in tiles {
                if t.label.is_blank() {
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

        // A LINK REACHING A TILE THIS DIAGRAM DOES NOT PLACE IS A LINE TO
        // NOWHERE: the renderer has no cell to draw from, so it draws nothing,
        // which is indistinguishable from a link that was never there. The file
        // asserts a connection and the picture silently omits it.
        //
        // AN EMPTY GROUP IS THE SAME SENTENCE ABOUT A DIFFERENT SUBJECT, which
        // is why the widening to group endpoints refuses one rather than
        // admitting groups outright: a group's region is derived from its
        // members' cells and is nothing at all when it has none.
        //
        // Membership is asked of `content` directly because no `Diagram` exists
        // yet to ask `members` — same answer, one construction earlier.
        let members_of = |g: &GroupId| -> bool {
            content
                .ids()
                .any(|id| content.group_of(id) == Some(g) && cells.contains_key(id))
        };
        for (id, l) in &links {
            for end in [&l.from, &l.to] {
                let present = match end {
                    Endpoint::Tile(t) => cells.contains_key(t),
                    Endpoint::Group(g) => groups.contains_key(g),
                };
                if !present {
                    return Err(ModelError::LinkToNowhere {
                        link: id.clone(),
                        end: end.clone(),
                    });
                }
                if let Endpoint::Group(g) = end
                    && !members_of(g)
                {
                    return Err(ModelError::LinkToEmptyGroup {
                        link: id.clone(),
                        group: g.clone(),
                    });
                }
            }
            if l.from == l.to {
                return Err(ModelError::LinkToItself(id.clone()));
            }
            // A GROUP AND ITS OWN MEMBER, EITHER WAY ROUND. Containment says it
            // already; the line adds nothing and is the likeliest mis-drag —
            // pressing on a ground and releasing on one of its own hexagons.
            // Checked in both directions from one loop rather than written
            // twice, because "these are the same thing" has no direction and a
            // rule enforced one way round is a rule half the mistakes miss.
            for (a, b) in [(&l.from, &l.to), (&l.to, &l.from)] {
                if let (Some(g), Some(t)) = (a.group(), b.tile())
                    && content.group_of(t) == Some(g)
                {
                    return Err(ModelError::LinkToOwnMember {
                        link: id.clone(),
                        group: g.clone(),
                        tile: t.clone(),
                    });
                }
            }
        }

        // EVERY SUBJECT THIS DIAGRAM WILL BE WRITTEN AS, CHECKED FOR COLLISIONS.
        // The header above promises that a `Diagram` which exists serialises to
        // a document that validates. It did not: nothing checked the local names
        // the writer mints against each other, and two subjects with one IRI is
        // a file the reader refuses and SHACL rejects for closedness.
        //
        // `minted_subjects` is the ONE enumeration of what gets minted — see its
        // own doc for why `rules.rs` calls it too, on a diagram that already
        // exists, to ask the same question about a command that has not landed
        // yet. A second copy of this list here would be the exact bug this
        // block repairs, recreated one scope down.
        {
            let mut seen: BTreeMap<String, String> = BTreeMap::new();
            for (local, what) in minted_subjects(&slug, &groups, &content, &links) {
                if let Some(first) = seen.insert(local.clone(), what.clone()) {
                    return Err(ModelError::SubjectCollision {
                        local,
                        first,
                        second: what,
                    });
                }
            }
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
            links,
            extra,
            // See the field's own doc: nothing but a read of an existing file
            // ever has one of these to carry, and `try_new` is the only
            // constructor, so every hand-built `Diagram` starts with none.
            unreached: Vec::new(),
            // Same reasoning, same constructor, same reason it starts `None`.
            format_version: None,
            occupancy,
            placement: cells,
        })
    }

    pub fn slug(&self) -> &Slug {
        &self.slug
    }

    /// THE TEXT, AND NOT THE TAG. Every host that draws a heading wants the
    /// words — see [`Text`]'s own doc — so the common call stays the short one
    /// and keeps returning what it always returned. A caller that needs to know
    /// which language the words are in asks [`Diagram::label_text`].
    pub fn label(&self) -> &str {
        self.label.as_str()
    }

    /// The label WITH its language, for the writer and for a host that wants to
    /// mark up the language it is rendering.
    pub fn label_text(&self) -> &Text {
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

    pub fn links(&self) -> &BTreeMap<LinkId, Link> {
        &self.links
    }

    pub fn link(&self, id: &LinkId) -> Option<&Link> {
        self.links.get(id)
    }

    /// Every link with an end AT THIS TILE ITSELF. What a removal has to refuse
    /// over.
    ///
    /// DELIBERATELY NOT "every link that touches this tile". A link to the
    /// tile's GROUP also draws to this hexagon whenever this is the member
    /// facing the other end — but removing the tile only breaks that link if it
    /// was the group's LAST member, and the two refusals are different
    /// sentences with different fixes: "remove the line" versus "put something
    /// else in the group first". [`Diagram::links_at_group`] answers the other
    /// half and `rules.rs` asks both.
    pub fn links_at(&self, id: &TileId) -> Vec<LinkId> {
        self.links
            .iter()
            .filter(|(_, l)| l.from.tile() == Some(id) || l.to.tile() == Some(id))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Every link with an end at this GROUP. What emptying it has to refuse
    /// over — see [`Diagram::links_at`] for why the two are separate lists.
    pub fn links_at_group(&self, g: &GroupId) -> Vec<LinkId> {
        self.links
            .iter()
            .filter(|(_, l)| l.from.group() == Some(g) || l.to.group() == Some(g))
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Every cell an endpoint could be met at: one for a placement, the whole
    /// membership for a group.
    ///
    /// EMPTY IS A REAL ANSWER and not an error, because this is also what a
    /// host calls to find out whether an end is drawable at all before offering
    /// to connect it — `Command::Connect` refuses an empty group, and a host
    /// that could only learn that by being refused is a host whose palette
    /// offers lines that cannot be drawn.
    pub fn endpoint_cells(&self, e: &Endpoint) -> BTreeSet<Cell> {
        match e {
            Endpoint::Tile(t) => self.cell_of(t).into_iter().collect(),
            Endpoint::Group(g) => self
                .members(g)
                .iter()
                .filter_map(|id| self.cell_of(id))
                .collect(),
        }
    }

    /// The two cells this link is drawn between, right now.
    ///
    /// RECOMPUTED ON EVERY CALL AND STORED NOWHERE, for the reason a group's
    /// region is: an anchor written into the document is a second source of
    /// truth that disagrees with the members the moment one of them is dragged.
    /// See [`anchors`] for the rule and for the version a drag preview needs.
    ///
    /// `None` only when an end has no cell at all, which a `Diagram` that
    /// exists cannot have — `try_new` refuses both `LinkToNowhere` and
    /// `LinkToEmptyGroup` — so a caller meeting one has been handed a link that
    /// is not this diagram's.
    pub fn link_anchors(&self, l: &Link) -> Option<(Cell, Cell)> {
        anchors(&self.endpoint_cells(&l.from), &self.endpoint_cells(&l.to))
    }

    pub(crate) fn add_link(&mut self, id: LinkId, link: Link) {
        self.links.insert(id, link);
    }

    pub(crate) fn take_link(&mut self, id: &LinkId) -> Option<Link> {
        self.links.remove(id)
    }

    /// Predicates this vocabulary does not define, on the diagram subject.
    pub fn extra(&self) -> &[Statement] {
        &self.extra
    }

    /// Whole subjects this document carries that no walk from this diagram
    /// reaches. See the field's own doc for what these are and why they used
    /// to vanish on the next save.
    pub fn unreached(&self) -> &[(Iri, Vec<Statement>)] {
        &self.unreached
    }

    /// `pub(crate)`: only `ttl::read::read_turtle_all` may call this, and only
    /// once, immediately after `try_new` returns — see `unreached`'s own doc
    /// for why a hand-built `Diagram` never needs to.
    pub(crate) fn set_unreached(&mut self, unreached: Vec<(Iri, Vec<Statement>)>) {
        self.unreached = unreached;
    }

    /// What `hive:formatVersion` this document declared, or `None` for a
    /// document — every file this crate wrote before this existed, among
    /// others — that declared none at all. See the field's own doc for why
    /// this is never what the NEXT save will state.
    pub fn format_version(&self) -> Option<Version> {
        self.format_version
    }

    /// `pub(crate)` for `unreached`'s own reason: only `ttl::read::build` ever
    /// has one of these to record.
    pub(crate) fn set_format_version(&mut self, v: Option<Version>) {
        self.format_version = v;
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
    /// no content to hand out.
    ///
    /// RETURNS [`OwnTileEdit`], NOT `&mut OwnTile`. An `OwnTile` carries no
    /// cell, so nothing reachable through a `&mut OwnTile` could desynchronise
    /// the occupancy index — THAT reasoning is still true, and it is exactly
    /// as far as it ever went: it says nothing about `group`, because when it
    /// was written a tile's group was not yet something a `Diagram` had to
    /// keep synchronised with anything either. It became load-bearing the
    /// moment a link could name a group, and a `&mut OwnTile`'s `group` field
    /// being `pub` turned this accessor into a second, unchecked door onto the
    /// same membership `Command::Attach` and `Command::Detach` guard through
    /// `Diagram::check` — see [`OwnTileEdit`]'s own doc for the exploit that
    /// was possible here and the test that proved it. Widening what an
    /// existing accessor can reach is exactly the kind of change this comment
    /// exists to flag for the next one: an accessor's safety argument is
    /// scoped to the invariants that existed when it was written, not to
    /// whatever gets built on top of the type it returns later.
    pub fn own_tile_mut(&mut self, id: &TileId) -> Option<OwnTileEdit<'_>> {
        match &mut self.content {
            Content::Standalone { tiles } => tiles.get_mut(id).map(|t| OwnTileEdit {
                label: &mut t.label,
                comment: &mut t.comment,
                style_key: &mut t.style_key,
                extra: &mut t.extra,
            }),
            Content::Pinned { .. } => None,
        }
    }

    pub fn group_mut(&mut self, g: &GroupId) -> Option<&mut Group> {
        self.groups.get_mut(g)
    }

    /// Flood fill over the 6-neighbour relation, over what is COMMITTED. See
    /// the free [`components`] for the same fill over cells a caller already
    /// holds — which is what a view needs, because during a drag the arrangement
    /// on screen is not the one in this struct.
    ///
    /// REPORTED, never enforced: only a detach can fracture a group (a rigid
    /// translation is an isometry), and an editor that permits regrouping must
    /// permit the transiently split state between pulling a member out and
    /// putting it back.
    pub fn group_components(&self, g: &GroupId) -> Vec<BTreeSet<Cell>> {
        components(
            self.members(g)
                .iter()
                .filter_map(|id| self.cell_of(id))
                .collect(),
        )
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
        // THE TWO WAYS A SET OF MOVES DESTROYS A DIAGRAM, ASSERTED HERE BECAUSE
        // THIS IS THE ONLY DOOR THEY CAN COME THROUGH.
        //
        // (a) Two moves naming one cell. Phase two below would write
        //     `placement[A] = c; occupancy[c] = A` and then the same for B:
        //     `placement` would claim both tiles sit at `c` while `occupancy`
        //     names only B — and `cells()` iterates `occupancy`, so tile A
        //     disappears from the render AND from the Turtle the page writes,
        //     with nothing anywhere returning an error. `Diagram::try_new` is
        //     the only other place a duplicate cell is ever caught, and it never
        //     runs again after construction.
        //
        // (b) A move landing on a tile that is not itself moving. That is an
        //     overwrite: the occupant is evicted from `occupancy` while
        //     `placement` still claims the cell for it.
        //
        // `Plan`'s shape makes both true by construction for the two rules that
        // exist today — which is the point of it being an enum — so these never
        // fire. They are here for the THIRD rule, whoever writes it. Note
        // honestly that `debug_assert` is compiled out of the release wasm this
        // ships in: these protect the test suite, and the `Plan` type protects
        // the user.
        debug_assert!(
            moves.iter().map(|(_, c)| *c).collect::<BTreeSet<_>>().len() == moves.len(),
            "two moves target one cell, so a tile is about to vanish: {moves:?}"
        );
        debug_assert!(
            moves.iter().all(|(_, to)| match self.occupancy.get(to) {
                None => true,
                Some(occ) => moves.iter().any(|(id, _)| id == occ),
            }),
            "a move lands on a tile that is not itself moving: {moves:?}"
        );
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

    /// THE THREE MAPS ARE ONE FACT AND THIS WRITES ALL THREE. `content`,
    /// `placement` and `occupancy` are three views of "this tile is here"; a
    /// mutator that wrote two of them would leave a tile `members` can find and
    /// `cells` cannot, and `members` is what a group drag iterates — so the
    /// editor would translate a tile with no cell and panic on the `expect` in
    /// `check`.
    ///
    /// `pub(crate)` for `relocate`'s own reason: only `rules.rs` may call it,
    /// and only through `apply`, which has already run `check`.
    pub(crate) fn insert(&mut self, id: TileId, at: Cell, what: NewTile) {
        debug_assert!(
            !self.occupancy.contains_key(&at),
            "an add lands on an occupied cell: {id:?} -> {at:?}"
        );
        debug_assert!(
            !self.placement.contains_key(&id),
            "an add overwrites a tile already on the board: {id:?}"
        );
        match (&mut self.content, what) {
            (Content::Pinned { tiles, .. }, NewTile::Pinned(t)) => {
                tiles.insert(id.clone(), t);
            }
            (Content::Standalone { tiles }, NewTile::Own(t)) => {
                tiles.insert(id.clone(), *t);
            }
            // RETURNS BEFORE TOUCHING EITHER INDEX. `check` refuses a mismatch
            // with `Rejection::WrongMode` and `apply` never gets here — but if
            // it ever did, writing the cell and not the content is the orphan
            // this function exists to make impossible.
            _ => return,
        }
        self.placement.insert(id.clone(), at);
        self.occupancy.insert(at, id);
    }

    /// The inverse, and it HANDS BACK WHAT IT REMOVED so the caller can record a
    /// self-contained inverse. A `Remove` whose undo had to re-derive the
    /// content from the diagram would be re-deriving it from a diagram that no
    /// longer has it.
    ///
    /// Leaves `groups` alone, deliberately: see `Command::Remove`.
    pub(crate) fn take(&mut self, id: &TileId) -> Option<(Cell, NewTile)> {
        let at = self.placement.remove(id)?;
        self.occupancy.remove(&at);
        let what = match &mut self.content {
            Content::Pinned { tiles, .. } => tiles.remove(id).map(NewTile::Pinned),
            Content::Standalone { tiles } => tiles.remove(id).map(|t| NewTile::Own(Box::new(t))),
        };
        // Unreachable while the three maps agree, which `insert` and `try_new`
        // are what guarantee — and putting the cell back is the only honest
        // thing to do if they ever do not.
        let Some(what) = what else {
            self.placement.insert(id.clone(), at);
            self.occupancy.insert(at, id.clone());
            return None;
        };
        Some((at, what))
    }

    /// The three group verbs' mutators. `pub(crate)` for `relocate`'s reason:
    /// only `rules.rs` may call them, and only through `apply`, which has
    /// already run `check`.
    pub(crate) fn declare_group(&mut self, id: GroupId, group: Group) {
        self.groups.insert(id, group);
    }

    pub(crate) fn undeclare_group(&mut self, id: &GroupId) -> Option<Group> {
        debug_assert!(
            self.members(id).is_empty(),
            "a group is being undeclared with tiles still in it: {id:?}"
        );
        self.groups.remove(id)
    }

    pub(crate) fn swap_group(&mut self, id: &GroupId, group: Group) -> Option<Group> {
        self.groups.get_mut(id).map(|g| std::mem::replace(g, group))
    }

    pub(crate) fn occupant(&self, cell: &Cell) -> Option<&TileId> {
        self.occupancy.get(cell)
    }

    /// PUBLIC BECAUSE A HOST HAS TO ASK BEFORE IT OFFERS. `Command::Add` and
    /// `Command::Attach` both refuse a group this diagram has not declared, and a
    /// host that can only learn that by being refused is a host whose palette
    /// offers things that cannot be dropped.
    pub fn has_group(&self, g: &GroupId) -> bool {
        self.groups.contains_key(g)
    }

    pub(crate) fn set_group(&mut self, id: &TileId, g: Option<GroupId>) {
        self.content.set_group(id, g);
    }

    /// Would minting `local` collide with a subject this diagram already
    /// mints — and if so, what does it already belong to?
    ///
    /// `pub(crate)` because `rules.rs` is the only caller. `try_new` checks a
    /// whole `DiagramSpec` against itself, once, before a `Diagram` exists;
    /// `check` has no such moment for `DeclareGroup`, `Add` or `Connect` — each
    /// mints a subject into a diagram that is already live — so it needs the
    /// same answer about a SINGLE prospective local name, on demand. Both call
    /// [`minted_subjects`], which is the one place that list is written down.
    pub(crate) fn would_collide(&self, local: &str) -> Option<String> {
        minted_subjects(&self.slug, &self.groups, &self.content, &self.links)
            .into_iter()
            .find(|(l, _)| l == local)
            .map(|(_, what)| what)
    }
}
