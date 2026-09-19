//! The serialiser. Deterministic, synchronous, and hand-rolled.

use crate::model::{Content, Diagram, Iri, Statement, Term, Timestamp};
use crate::ttl::{PLACEMENT_PREFIX, has_scheme};
use crate::{NS, terms};

const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// What this crate calls itself in `hive:generator`, from its own package
/// metadata rather than a string somebody has to remember to bump.
const GENERATOR: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// How subjects are named and what the preamble says. The three IRI parts are
/// private and validated: a `subject_ns` not ending in `/` or `#` concatenates
/// with a slug into an IRI whose last segment is not that slug, which nothing
/// here would notice and `hsh:SlugMatchesIri` would report one repository away.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteOpts {
    base: String,
    subject_prefix: String,
    subject_ns: String,
    prefixes: Vec<(String, String)>,
    generated_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BadWriteOpts {
    pub field: &'static str,
    pub reason: &'static str,
}

fn bad(field: &'static str, reason: &'static str) -> BadWriteOpts {
    BadWriteOpts { field, reason }
}

/// A prefix label this crate is willing to write, which is narrower than
/// Turtle's PN_PREFIX on purpose: an exotic label is legal and unreadable, and
/// the failure it causes lands in somebody else's parser.
fn ok_prefix(p: &str) -> bool {
    !p.is_empty()
        && p.starts_with(|c: char| c.is_ascii_alphabetic())
        && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn ok_namespace(ns: &str) -> bool {
    has_scheme(ns) && ns.ends_with(['/', '#'])
}

impl WriteOpts {
    pub fn new(
        base: &str,
        subject_prefix: &str,
        subject_ns: &str,
    ) -> Result<WriteOpts, BadWriteOpts> {
        if !has_scheme(base) {
            return Err(bad(
                "base",
                "the document's @base must be an absolute IRI; a relative one resolves against \
                 nothing and every subject in the file inherits that",
            ));
        }
        if !ok_prefix(subject_prefix) {
            return Err(bad(
                "subject_prefix",
                "a prefix label is an ASCII letter followed by letters, digits or hyphens",
            ));
        }
        if matches!(subject_prefix, "hive" | "rdfs" | "xsd") {
            return Err(bad(
                "subject_prefix",
                "that label is already bound to the vocabulary, rdfs or xsd; rebinding it would \
                 silently rename every term in the file",
            ));
        }
        if !ok_namespace(subject_ns) {
            return Err(bad(
                "subject_ns",
                "the subject namespace must be an absolute IRI ending in '/' or '#', or a slug \
                 concatenated onto it is not the last segment of the IRI it makes",
            ));
        }
        Ok(WriteOpts {
            base: base.to_string(),
            subject_prefix: subject_prefix.to_string(),
            subject_ns: subject_ns.to_string(),
            prefixes: Vec::new(),
            generated_at: None,
        })
    }

    /// Extra prefix bindings, for host predicates carried in `OwnTile::extra`.
    /// Unbound predicates are still written, as full IRI refs — a round trip
    /// must never lose a statement because nobody declared a prefix for it.
    pub fn with_prefix(mut self, prefix: &str, ns: &str) -> Result<WriteOpts, BadWriteOpts> {
        if !ok_prefix(prefix) {
            return Err(bad(
                "prefix",
                "a prefix label is an ASCII letter followed by letters, digits or hyphens",
            ));
        }
        if matches!(prefix, "hive" | "rdfs" | "xsd") || prefix == self.subject_prefix {
            return Err(bad(
                "prefix",
                "that label is already bound in this document, and a second binding would make \
                 which one wins a question about statement order",
            ));
        }
        if !ok_namespace(ns) {
            return Err(bad(
                "ns",
                "a namespace must be an absolute IRI ending in '/' or '#'",
            ));
        }
        self.prefixes.push((prefix.to_string(), ns.to_string()));
        Ok(self)
    }

    /// A PARAMETER, not a call to a clock. That is exactly what makes every
    /// golden-bytes test in this crate possible, and it is why this crate needs
    /// no time dependency.
    pub fn with_generated_at(mut self, t: Timestamp) -> WriteOpts {
        self.generated_at = Some(t);
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn subject_prefix(&self) -> &str {
        &self.subject_prefix
    }

    pub fn subject_ns(&self) -> &str {
        &self.subject_ns
    }

    pub fn prefixes(&self) -> &[(String, String)] {
        &self.prefixes
    }

    pub fn generated_at(&self) -> Option<&Timestamp> {
        self.generated_at.as_ref()
    }

    /// Every prefix bound in the document, longest namespace first, so that
    /// rendering picks the most specific binding rather than whichever happened
    /// to be declared first.
    fn bindings(&self) -> Vec<(&str, &str)> {
        let mut v: Vec<(&str, &str)> = vec![
            ("hive", NS),
            ("rdfs", RDFS),
            ("xsd", XSD),
            (&self.subject_prefix, &self.subject_ns),
        ];
        for (p, ns) in &self.prefixes {
            v.push((p.as_str(), ns.as_str()));
        }
        v.sort_by_key(|(_, ns)| core::cmp::Reverse(ns.len()));
        v
    }

    /// An IRI as a prefixed name where one is available and legal, otherwise as
    /// a full IRI ref. Never as a relative ref: a bare `<vault>` is readable
    /// only by something that resolved this document's `@base` the same way.
    ///
    /// The full-ref branch ESCAPES — see [`iri_ref`]. It did not, and `lit()`
    /// directly above it has escaped literals since the first commit, so the one
    /// thing in the document nobody escaped was the one thing that could end the
    /// token it was inside.
    fn iri(&self, iri: &str) -> String {
        for (p, ns) in self.bindings() {
            if let Some(local) = iri.strip_prefix(ns)
                && ok_local(local)
            {
                return format!("{p}:{local}");
            }
        }
        format!("<{}>", iri_ref(iri))
    }

    fn subject(&self, local: &str) -> String {
        format!("{}:{}", self.subject_prefix, local)
    }
}

/// A conservative PN_LOCAL: Turtle allows more, including escapes, and every
/// character beyond this set is one more way for a consumer's parser to
/// disagree with this one about where the name ends.
/// The characters an `IRIREF` may not contain, as `\uXXXX`.
///
/// AN UNESCAPED `>` ENDS THE TOKEN, and the result is not a document with an odd
/// IRI in it — it is a document that stops parsing several tokens later, at
/// something that looks unrelated. Nor is this only a host's problem: the lexer
/// UNESCAPES `\uXXXX` inside an IRI ref, so this crate's own reader hands back
/// `Iri` values containing `>` from a perfectly legal input, and writing one
/// back out produced a file it could no longer read.
///
/// Turtle forbids `<>"{}|^\` and everything at or below U+0020 inside an
/// `IRIREF`. A space is the common one: legal for this crate's tolerant reader,
/// rejected by a strict one, so it is escaped too.
fn iri_ref(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for c in iri.chars() {
        match c {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if (c as u32) <= 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn ok_local(s: &str) -> bool {
    !s.is_empty()
        && !s.ends_with('.')
        && s.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Escaped for the five characters that would otherwise end the literal or a
/// line. `\u` escapes are deliberately not produced: a non-ASCII character is
/// written as itself, because the file is UTF-8 and an escaped one is only
/// harder to read in a diff.
fn lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One subject's predicate-object lines, joined with ` ;` and closed with ` .`.
/// Collecting them first is what lets every block end correctly without each
/// writer knowing whether it is last.
fn block(subject: &str, kind: &str, lines: Vec<String>) -> String {
    let mut s = format!("{subject} a {kind}");
    for l in &lines {
        s.push_str(" ;\n    ");
        s.push_str(l);
    }
    s.push_str(" .\n\n");
    s
}

fn statement(o: &WriteOpts, st: &Statement) -> String {
    let obj = match &st.object {
        Term::Iri(Iri(i)) => o.iri(i),
        Term::Literal {
            value,
            datatype,
            lang,
        } => {
            let mut s = lit(value);
            if let Some(l) = lang {
                s.push('@');
                s.push_str(l);
            } else if let Some(Iri(d)) = datatype {
                s.push_str("^^");
                s.push_str(&o.iri(d));
            }
            s
        }
    };
    format!("{} {}", o.iri(&st.predicate.0), obj)
}

/// Turtle for the whole diagram. Hand-rolled, and SYNCHRONOUS — not an
/// optimisation: Safari consumes transient activation at the first `await`, so a
/// clipboard handler that awaited its own serialiser throws `NotAllowedError`
/// there and copy silently stops working on one browser.
///
/// DETERMINISTIC: exactly one `@base`; `hive:` bound to [`NS`]; every subject
/// written as a prefixed name, never a bare relative ref; placements in
/// (row, col) order and groups and tiles by slug. Without a total order every
/// export churns the whole file and a diagram's git history is worth nothing.
///
/// It emits `hive:generator` as this crate's own name and version and IGNORES
/// [`Diagram::generator`]: echoing back the value read would make the file name
/// a program that did not write it, which is worse than naming none.
///
/// ONE THING IT CANNOT CHECK, and the vocabulary rather than this crate is why.
/// `hsh:SlugMatchesIri` forces a diagram, a group and a tile each to live at
/// `{subject_ns}{slug}`, so three subjects sharing a slug share an IRI. Picking
/// slugs that do not collide is the host's job; there is no naming this writer
/// could choose that would both avoid it and keep the slug rule.
pub fn write_turtle(d: &Diagram, o: &WriteOpts) -> String {
    let mut out = String::new();

    // The @base is emitted even though nothing below is written relative to it,
    // so that a hand-edit which adds `<vault>` resolves inside this document's
    // own namespace rather than against whatever the reader happened to pass.
    out.push_str(&format!("@base <{}> .\n", o.base));
    out.push_str(&format!("@prefix hive: <{NS}> .\n"));
    out.push_str(&format!("@prefix rdfs: <{RDFS}> .\n"));
    out.push_str(&format!("@prefix xsd: <{XSD}> .\n"));
    out.push_str(&format!(
        "@prefix {}: <{}> .\n",
        o.subject_prefix, o.subject_ns
    ));
    for (p, ns) in &o.prefixes {
        out.push_str(&format!("@prefix {p}: <{ns}> .\n"));
    }
    out.push('\n');

    let hive = |t: &str| format!("hive:{t}");

    // ---- the diagram. FIRST in the file, because it is what a reader opens the
    // file to find out: what this is, which mode it is in, and what it lays out.
    let mut lines = vec![
        format!("{} {}", hive(terms::prop::SLUG), lit(d.slug().as_str())),
        format!("rdfs:label {}", lit(d.label())),
    ];
    if let Some(n) = d.note() {
        lines.push(format!("{} {}", hive(terms::prop::NOTE), lit(n)));
    }
    lines.push(format!(
        "{} hive:{}",
        hive(terms::prop::MODE),
        d.mode().term()
    ));
    lines.push(format!(
        "{} hive:{}",
        hive(terms::prop::LATTICE),
        d.convention().term()
    ));
    if let Some(Iri(src)) = d.source() {
        lines.push(format!("{} {}", hive(terms::prop::PINNED_TO), o.iri(src)));
    }
    if let Some(rev) = d.revision() {
        lines.push(format!(
            "{} {}",
            hive(terms::prop::PINNED_REVISION),
            lit(rev)
        ));
    }
    lines.push(format!(
        "{} {}",
        hive(terms::prop::GENERATOR),
        lit(GENERATOR)
    ));
    // The opts win over the document's own value so that a re-export stamps when
    // IT ran; falling back to the document's keeps a round trip that supplies no
    // timestamp from silently dropping the one the file already carried.
    if let Some(t) = o.generated_at().or_else(|| d.generated_at()) {
        lines.push(format!(
            "{} {}^^xsd:dateTime",
            hive(terms::prop::GENERATED_AT),
            lit(t.as_str())
        ));
    }
    if !d.groups().is_empty() {
        let gs: Vec<String> = d.groups().keys().map(|g| o.subject(g.0.as_str())).collect();
        lines.push(format!("{} {}", hive(terms::prop::GROUP), gs.join(" , ")));
    }
    let ps: Vec<String> = d
        .cells()
        .map(|(_, id)| o.subject(&format!("{PLACEMENT_PREFIX}{}", id.0.as_str())))
        .collect();
    lines.push(format!(
        "{} {}",
        hive(terms::prop::PLACEMENT),
        ps.join(" , ")
    ));
    // The host's own predicates, last, verbatim. `hsh:DiagramShape` is not
    // closed and says why: "a host hangs its own predicates on a diagram". They
    // were read and dropped for this crate's whole first life.
    for st in d.extra() {
        lines.push(statement(o, st));
    }
    out.push_str(&block(
        &o.subject(d.slug().as_str()),
        &hive(terms::class::DIAGRAM),
        lines,
    ));

    // ---- groups, by slug.
    for (id, g) in d.groups() {
        let mut lines = vec![
            format!("{} {}", hive(terms::prop::SLUG), lit(id.0.as_str())),
            format!("rdfs:label {}", lit(&g.label)),
        ];
        if let Some(Iri(k)) = &g.style_key {
            lines.push(format!("{} {}", hive(terms::prop::STYLE_KEY), o.iri(k)));
        }
        if let Some(n) = &g.note {
            lines.push(format!("{} {}", hive(terms::prop::NOTE), lit(n)));
        }
        for st in &g.extra {
            lines.push(statement(o, st));
        }
        out.push_str(&block(
            &o.subject(id.0.as_str()),
            &hive(terms::class::GROUP),
            lines,
        ));
    }

    // ---- tiles, by slug. Standalone only: a pinned diagram HAS no tiles, which
    // is the anti-drift property arriving in the bytes.
    if let Content::Standalone { tiles } = d.content() {
        for (id, t) in tiles {
            let mut lines = vec![
                format!("{} {}", hive(terms::prop::SLUG), lit(id.0.as_str())),
                format!("rdfs:label {}", lit(&t.label)),
            ];
            if let Some(c) = &t.comment {
                lines.push(format!("rdfs:comment {}", lit(c)));
            }
            if let Some(Iri(k)) = &t.style_key {
                lines.push(format!("{} {}", hive(terms::prop::STYLE_KEY), o.iri(k)));
            }
            for st in &t.extra {
                lines.push(statement(o, st));
            }
            out.push_str(&block(
                &o.subject(id.0.as_str()),
                &hive(terms::class::TILE),
                lines,
            ));
        }
    }

    // ---- placements, in (row, col) order: the order a reader's eye travels.
    for (cell, id) in d.cells() {
        let mut lines = vec![
            format!("{} {}", hive(terms::prop::COL), cell.col),
            format!("{} {}", hive(terms::prop::ROW), cell.row),
        ];
        if let Some(g) = d.group_of(id) {
            lines.push(format!(
                "{} {}",
                hive(terms::prop::IN_GROUP),
                o.subject(g.0.as_str())
            ));
        }
        match d.content() {
            Content::Pinned { tiles, .. } => {
                if let Some(t) = tiles.get(id) {
                    lines.push(format!(
                        "{} {}",
                        hive(terms::prop::REPRESENTS),
                        o.iri(&t.represents.0)
                    ));
                }
            }
            Content::Standalone { .. } => {
                lines.push(format!(
                    "{} {}",
                    hive(terms::prop::TILE),
                    o.subject(id.0.as_str())
                ));
            }
        }
        out.push_str(&block(
            &o.subject(&format!("{PLACEMENT_PREFIX}{}", id.0.as_str())),
            &hive(terms::class::PLACEMENT),
            lines,
        ));
    }

    out
}
