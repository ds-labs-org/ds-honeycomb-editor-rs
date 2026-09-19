//! The parser. ALWAYS COMPILED, and hand-written like the writer.
//!
//! It accepts the Turtle subset [`crate::write_turtle`] emits plus what a
//! hand-edit plausibly adds: `@prefix`/`@base` and their SPARQL spellings, IRI
//! refs, prefixed names, `a`, `;` `,` `.`, comments, quoted and long-quoted
//! literals with escapes, language tags, `^^` datatypes, integers, decimals,
//! doubles and booleans.
//!
//! It REFUSES blank nodes, `[ ]` property lists and `( )` collections BY NAME
//! rather than skipping them: a placement or group written as a blank node is
//! invisible to a consumer gate that selects subjects by IRI prefix, so
//! accepting one would produce the single unchecked subject in a checked file.
//!
//! WHAT IT WILL NOT DO IS GUESS. A mode that disagrees with the placements, a
//! slug that disagrees with its IRI, a convention it does not implement: each is
//! reported rather than reconciled, because every repair available here writes a
//! document the author did not mean — a corrected slug gains a duplicate subject
//! on the next export, and an approximated convention draws the same integers as
//! a different picture with nothing anywhere to say so.

use std::collections::{BTreeMap, BTreeSet};

use crate::lattice::Cell;
use crate::model::{
    Content, Diagram, DiagramSpec, Group, GroupId, Iri, LatticeConvention, Link, LinkId, Mode,
    ModelError, OwnTile, PinnedTile, Routing, Slug, Statement, Term, TileId, Timestamp,
};
use crate::ttl::{PLACEMENT_PREFIX, RDF_TYPE, has_scheme, last_segment};
use crate::{NS, terms};

const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_COMMENT: &str = "http://www.w3.org/2000/01/rdf-schema#comment";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadOpts {
    /// Fallback base for a document that declares none. `None` makes a relative
    /// IRI ref a named error rather than a subject silently resolved against
    /// nothing.
    pub base: Option<String>,
    /// Which diagram [`read_turtle`] should return. `None` is an error, not a
    /// guess, when the document holds more than one.
    pub slug: Option<Slug>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReadError {
    Syntax {
        line: u32,
        col: u32,
        found: String,
        expected: &'static str,
    },
    UnresolvedPrefix {
        prefix: String,
        line: u32,
    },
    RelativeIriWithoutBase {
        iri: String,
        line: u32,
    },
    /// A blank node where the contract requires an IRI (see the subset note).
    BlankNodeRefused {
        line: u32,
    },
    /// The vocabulary says a reader meeting an unimplemented convention or mode
    /// must REFUSE rather than approximate: the same integers under another
    /// convention are a different picture, and nothing in the file would say so.
    UnknownLatticeConvention(String),
    /// Same argument as the two above: a reader meeting a routing it does not
    /// implement must REFUSE rather than approximate, because there is no
    /// "roughly straight" and a line in the wrong place is a different diagram.
    UnknownRouting(String),
    /// A slug in the document that this crate will not accept as one.
    BadSlug(crate::model::BadSlug),
    UnknownMode(String),
    /// The file says one mode and its placements say the other. Reported rather
    /// than reconciled — guessing here is how half a diagram starts tracking its
    /// source while the other half is frozen.
    ModeDisagreesWithContent {
        declared: Mode,
        found: &'static str,
        subject: String,
    },
    MissingRequired {
        subject: String,
        predicate: &'static str,
    },
    /// A `hive:slug` that is not the last segment of its subject's IRI. REFUSED
    /// rather than repaired: reading it and writing it back under the corrected
    /// IRI is exactly how a document gains a duplicate subject.
    SlugDoesNotMatchIri {
        subject: String,
        slug: String,
    },
    /// No diagram, or several and [`ReadOpts::slug`] did not say which.
    NoDiagram,
    AmbiguousDocument {
        slugs: Vec<Slug>,
    },
    /// Everything [`Diagram::try_new`] refuses, surfaced unchanged so that a
    /// file and a programmatic construction fail with the same words.
    Model(ModelError),
    /// A document that PARSES and then breaks the contract in a way none of the
    /// variants above names. It exists because three real refusals had no home:
    /// a `hive:Placement` carrying a predicate `hsh:PlacementShape`'s
    /// `sh:closed` forbids; a property the shapes cap at one appearing twice
    /// (two `hive:mode` values are not a third mode, they are a file no reader
    /// can classify); and a lexically valid value the contract still rejects,
    /// such as a zoneless `xsd:dateTime`.
    ///
    /// It carries the offending predicate because "this file is wrong" is not
    /// something a contributor can act on and "delete this predicate from this
    /// subject" is.
    ContractViolated {
        subject: String,
        predicate: String,
        reason: &'static str,
    },
}

// ================================================================== lexing

#[derive(Debug, Clone, PartialEq)]
enum Tk {
    IriRef(String),
    PName(String, String),
    A,
    Str(String),
    Lang(String),
    Caret,
    Num(String, &'static str),
    Semi,
    Comma,
    Dot,
    AtPrefix,
    AtBase,
    KwPrefix,
    KwBase,
    /// `[`, `]`, `_:x`, `(` or `)` — all refused alike, and the string says
    /// which so the error can quote it.
    Refused(&'static str),
}

#[derive(Debug, Clone)]
struct Token {
    tk: Tk,
    line: u32,
    col: u32,
}

fn syntax(line: u32, col: u32, found: impl Into<String>, expected: &'static str) -> ReadError {
    ReadError::Syntax {
        line,
        col,
        found: found.into(),
        expected,
    }
}

/// True for a character that may sit inside a prefixed name. Deliberately
/// generous about non-ASCII (Turtle's PN_CHARS_BASE is most of Unicode) and
/// deliberately silent about PN_LOCAL escapes, which this crate never writes.
fn pn_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '%') || !c.is_ascii()
}

fn lex(src: &str) -> Result<Vec<Token>, ReadError> {
    let b: Vec<char> = src.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0usize;
    let (mut line, mut col) = (1u32, 1u32);
    // Whether anything separated the previous token from this one. A language
    // tag must touch its literal; `"x" @prefix` is a directive and `"x"@en` is
    // not, and the gap is the only thing that tells them apart.
    let mut gap = true;

    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
            gap = true;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            col += 1;
            gap = true;
            continue;
        }
        if c == '#' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            gap = true;
            continue;
        }

        let (tl, tc) = (line, col);
        let start = i;
        let mut produced: Vec<Tk> = Vec::new();

        match c {
            '<' => {
                let mut j = i + 1;
                let mut raw = String::new();
                loop {
                    if j >= b.len() || b[j] == '\n' {
                        return Err(syntax(tl, tc, "<", "a `>` closing this IRI ref"));
                    }
                    if b[j] == '>' {
                        break;
                    }
                    if b[j] == '\\' {
                        let (ch, next) = unescape_at(&b, j, tl, tc)?;
                        raw.push(ch);
                        j = next;
                        continue;
                    }
                    raw.push(b[j]);
                    j += 1;
                }
                produced.push(Tk::IriRef(raw));
                i = j + 1;
            }
            '"' | '\'' => {
                let (value, next) = scan_string(&b, i, tl, tc)?;
                produced.push(Tk::Str(value));
                i = next;
            }
            ';' => {
                produced.push(Tk::Semi);
                i += 1;
            }
            ',' => {
                produced.push(Tk::Comma);
                i += 1;
            }
            '[' => {
                produced.push(Tk::Refused("["));
                i += 1;
            }
            ']' => {
                produced.push(Tk::Refused("]"));
                i += 1;
            }
            '(' => {
                produced.push(Tk::Refused("("));
                i += 1;
            }
            ')' => {
                produced.push(Tk::Refused(")"));
                i += 1;
            }
            '^' => {
                if b.get(i + 1) != Some(&'^') {
                    return Err(syntax(tl, tc, "^", "`^^` introducing a datatype"));
                }
                produced.push(Tk::Caret);
                i += 2;
            }
            '@' => {
                let mut j = i + 1;
                while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == '-') {
                    j += 1;
                }
                let word: String = b[i + 1..j].iter().collect();
                let after_literal = !gap && matches!(out.last().map(|t| &t.tk), Some(Tk::Str(_)));
                if after_literal {
                    if word.is_empty() {
                        return Err(syntax(tl, tc, "@", "a language tag after the literal"));
                    }
                    produced.push(Tk::Lang(word));
                } else {
                    match word.to_ascii_lowercase().as_str() {
                        "prefix" => produced.push(Tk::AtPrefix),
                        "base" => produced.push(Tk::AtBase),
                        _ => return Err(syntax(tl, tc, format!("@{word}"), "@prefix or @base")),
                    }
                }
                i = j;
            }
            '.' if b.get(i + 1).is_some_and(|d| d.is_ascii_digit()) => {
                let (tk, next) = scan_number(&b, i);
                produced.push(tk);
                i = next;
            }
            '.' => {
                produced.push(Tk::Dot);
                i += 1;
            }
            '+' | '-' => {
                let (tk, next) = scan_number(&b, i);
                produced.push(tk);
                i = next;
            }
            '0'..='9' => {
                let (tk, next) = scan_number(&b, i);
                produced.push(tk);
                i = next;
            }
            '_' if b.get(i + 1) == Some(&':') => {
                let mut j = i + 2;
                while j < b.len() && pn_char(b[j]) {
                    j += 1;
                }
                produced.push(Tk::Refused("a _: blank node label"));
                i = j;
            }
            _ if pn_char(c) => {
                let mut j = i;
                while j < b.len() && pn_char(b[j]) {
                    j += 1;
                }
                let mut word: String = b[i..j].iter().collect();
                // A trailing `.` ends a statement: Turtle forbids one as the
                // last character of a prefixed name, so this cannot eat part of
                // a term.
                let mut dots = 0;
                while word.ends_with('.') {
                    word.pop();
                    dots += 1;
                }
                if word.is_empty() {
                    return Err(syntax(tl, tc, c.to_string(), "a term"));
                }
                match word.split_once(':') {
                    Some((p, local)) => produced.push(Tk::PName(p.to_string(), local.to_string())),
                    None => match word.as_str() {
                        "a" => produced.push(Tk::A),
                        "true" | "false" => {
                            produced.push(Tk::Num(word.clone(), "boolean"));
                        }
                        w if w.eq_ignore_ascii_case("prefix") => produced.push(Tk::KwPrefix),
                        w if w.eq_ignore_ascii_case("base") => produced.push(Tk::KwBase),
                        _ => {
                            return Err(syntax(
                                tl,
                                tc,
                                word,
                                "an IRI ref, a prefixed name, `a`, or a literal",
                            ));
                        }
                    },
                }
                for _ in 0..dots {
                    produced.push(Tk::Dot);
                }
                i = j;
            }
            _ => return Err(syntax(tl, tc, c.to_string(), "a term")),
        }

        for c in &b[start..i] {
            if *c == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        for tk in produced {
            out.push(Token {
                tk,
                line: tl,
                col: tc,
            });
        }
        gap = false;
    }
    Ok(out)
}

fn scan_number(b: &[char], i: usize) -> (Tk, usize) {
    let mut j = i;
    if matches!(b.get(j), Some('+') | Some('-')) {
        j += 1;
    }
    let mut seen_dot = false;
    let mut seen_exp = false;
    while j < b.len() {
        match b[j] {
            '0'..='9' => j += 1,
            '.' if !seen_dot && !seen_exp && b.get(j + 1).is_some_and(|d| d.is_ascii_digit()) => {
                seen_dot = true;
                j += 1;
            }
            'e' | 'E' if !seen_exp => {
                let after = b.get(j + 1);
                let after2 = b.get(j + 2);
                let ok = after.is_some_and(|d| d.is_ascii_digit())
                    || (matches!(after, Some('+') | Some('-'))
                        && after2.is_some_and(|d| d.is_ascii_digit()));
                if !ok {
                    break;
                }
                seen_exp = true;
                j += 2;
            }
            _ => break,
        }
    }
    let text: String = b[i..j].iter().collect();
    let kind = if seen_exp {
        "double"
    } else if seen_dot {
        "decimal"
    } else {
        "integer"
    };
    (Tk::Num(text, kind), j)
}

/// Returns the UNESCAPED text and the index after the closing quote. Long
/// literals are what a hand-edit reaches for when a note runs to two lines, and
/// they contain `#` and `"` — getting this wrong does not produce a small error,
/// it produces a parser that reads prose as syntax.
fn scan_string(b: &[char], i: usize, line: u32, col: u32) -> Result<(String, usize), ReadError> {
    let q = b[i];
    let long = b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q);
    let mut j = i + if long { 3 } else { 1 };
    let mut s = String::new();
    loop {
        let Some(&c) = b.get(j) else {
            return Err(syntax(line, col, q.to_string(), "a closing quote"));
        };
        if c == '\\' {
            let (ch, next) = unescape_at(b, j, line, col)?;
            s.push(ch);
            j = next;
            continue;
        }
        if c == q {
            if !long {
                return Ok((s, j + 1));
            }
            if b.get(j + 1) == Some(&q) && b.get(j + 2) == Some(&q) {
                return Ok((s, j + 3));
            }
        }
        if c == '\n' && !long {
            return Err(syntax(
                line,
                col,
                "a newline",
                "a closing quote on the same line, or a \"\"\"long literal\"\"\"",
            ));
        }
        s.push(c);
        j += 1;
    }
}

fn unescape_at(b: &[char], j: usize, line: u32, col: u32) -> Result<(char, usize), ReadError> {
    let Some(&e) = b.get(j + 1) else {
        return Err(syntax(line, col, "\\", "an escape character"));
    };
    let simple = |c: char| Ok((c, j + 2));
    match e {
        't' => simple('\t'),
        'b' => simple('\u{8}'),
        'n' => simple('\n'),
        'r' => simple('\r'),
        'f' => simple('\u{c}'),
        '"' => simple('"'),
        '\'' => simple('\''),
        '\\' => simple('\\'),
        '>' => simple('>'),
        'u' | 'U' => {
            let n = if e == 'u' { 4 } else { 8 };
            if b.len() < j + 2 + n {
                return Err(syntax(
                    line,
                    col,
                    format!("\\{e}"),
                    "four or eight hex digits",
                ));
            }
            let hex: String = b[j + 2..j + 2 + n].iter().collect();
            let code = u32::from_str_radix(&hex, 16)
                .map_err(|_| syntax(line, col, hex.clone(), "hex digits"))?;
            let ch = char::from_u32(code)
                .ok_or_else(|| syntax(line, col, hex, "a Unicode scalar value"))?;
            Ok((ch, j + 2 + n))
        }
        _ => Err(syntax(line, col, format!("\\{e}"), "a Turtle escape")),
    }
}

// ================================================================= parsing

#[derive(Debug, Clone, PartialEq)]
enum Obj {
    Iri(String),
    Lit {
        value: String,
        datatype: Option<String>,
        lang: Option<String>,
    },
}

/// Document order is preserved for two different reasons and both matter:
/// `extra` statements must come back in the order they were written or a round
/// trip is not byte-identical, and a multi-diagram file must report its diagrams
/// in the order a reader would find them.
struct Doc {
    order: Vec<String>,
    by_subject: BTreeMap<String, Vec<(String, Obj, u32)>>,
}

struct Parser<'a> {
    t: &'a [Token],
    i: usize,
    base: Option<String>,
    prefixes: BTreeMap<String, String>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tk> {
        self.t.get(self.i).map(|t| &t.tk)
    }

    fn pos(&self) -> (u32, u32) {
        self.t
            .get(self.i)
            .or_else(|| self.t.last())
            .map(|t| (t.line, t.col))
            .unwrap_or((1, 1))
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let t = self.t.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    fn expect_dot(&mut self) -> Result<(), ReadError> {
        match self.peek() {
            Some(Tk::Dot) => {
                self.i += 1;
                Ok(())
            }
            other => {
                let (l, c) = self.pos();
                Err(syntax(l, c, describe(other), "`.` ending the statement"))
            }
        }
    }

    /// An IRI ref or prefixed name, resolved. Everything downstream compares
    /// full IRIs, so a prefix quietly rebound mid-document cannot make two
    /// different terms look like one.
    fn iri(&mut self) -> Result<String, ReadError> {
        let (l, c) = self.pos();
        match self.bump().map(|t| t.tk.clone()) {
            Some(Tk::IriRef(raw)) => self.resolve(&raw, l),
            Some(Tk::PName(p, local)) => match self.prefixes.get(&p) {
                Some(ns) => Ok(format!("{ns}{local}")),
                None => Err(ReadError::UnresolvedPrefix { prefix: p, line: l }),
            },
            Some(Tk::Refused(_)) => Err(ReadError::BlankNodeRefused { line: l }),
            other => Err(syntax(
                l,
                c,
                describe(other.as_ref()),
                "an IRI ref or a prefixed name",
            )),
        }
    }

    fn resolve(&self, raw: &str, line: u32) -> Result<String, ReadError> {
        if has_scheme(raw) {
            return Ok(raw.to_string());
        }
        let Some(base) = &self.base else {
            return Err(ReadError::RelativeIriWithoutBase {
                iri: raw.to_string(),
                line,
            });
        };
        Ok(resolve_against(base, raw))
    }

    fn object(&mut self) -> Result<Obj, ReadError> {
        let (l, c) = self.pos();
        match self.peek().cloned() {
            Some(Tk::IriRef(_)) | Some(Tk::PName(_, _)) => Ok(Obj::Iri(self.iri()?)),
            Some(Tk::Refused(_)) => Err(ReadError::BlankNodeRefused { line: l }),
            Some(Tk::Str(value)) => {
                self.i += 1;
                match self.peek().cloned() {
                    Some(Tk::Lang(tag)) => {
                        self.i += 1;
                        Ok(Obj::Lit {
                            value,
                            datatype: None,
                            lang: Some(tag),
                        })
                    }
                    Some(Tk::Caret) => {
                        self.i += 1;
                        let dt = self.iri()?;
                        Ok(Obj::Lit {
                            value,
                            datatype: Some(dt),
                            lang: None,
                        })
                    }
                    _ => Ok(Obj::Lit {
                        value,
                        datatype: None,
                        lang: None,
                    }),
                }
            }
            Some(Tk::Num(text, kind)) => {
                self.i += 1;
                Ok(Obj::Lit {
                    value: text,
                    datatype: Some(format!("{XSD}{kind}")),
                    lang: None,
                })
            }
            other => Err(syntax(l, c, describe(other.as_ref()), "an object")),
        }
    }
}

fn describe(t: Option<&Tk>) -> String {
    match t {
        None => "the end of the document".to_string(),
        Some(Tk::IriRef(s)) => format!("<{s}>"),
        Some(Tk::PName(p, l)) => format!("{p}:{l}"),
        Some(Tk::A) => "a".to_string(),
        Some(Tk::Str(s)) => format!("{:?}", s.chars().take(24).collect::<String>()),
        Some(Tk::Lang(l)) => format!("@{l}"),
        Some(Tk::Caret) => "^^".to_string(),
        Some(Tk::Num(n, _)) => n.clone(),
        Some(Tk::Semi) => ";".to_string(),
        Some(Tk::Comma) => ",".to_string(),
        Some(Tk::Dot) => ".".to_string(),
        Some(Tk::AtPrefix) => "@prefix".to_string(),
        Some(Tk::AtBase) => "@base".to_string(),
        Some(Tk::KwPrefix) => "PREFIX".to_string(),
        Some(Tk::KwBase) => "BASE".to_string(),
        Some(Tk::Refused(w)) => (*w).to_string(),
    }
}

/// Enough of RFC 3986 for a document that declares its own base: this crate
/// writes no relative refs at all, so every path through here is a hand-edit.
fn resolve_against(base: &str, raw: &str) -> String {
    if raw.is_empty() {
        return base.to_string();
    }
    let stem = base.split(['#', '?']).next().unwrap_or(base);
    if let Some(rest) = raw.strip_prefix('#') {
        return format!("{stem}#{rest}");
    }
    if raw.starts_with("//") {
        let scheme = base.split_once(':').map(|(s, _)| s).unwrap_or("https");
        return format!("{scheme}:{raw}");
    }
    if raw.starts_with('/') {
        let after_scheme = stem.find("://").map(|i| i + 3).unwrap_or(0);
        let authority_end = stem[after_scheme..]
            .find('/')
            .map(|i| after_scheme + i)
            .unwrap_or(stem.len());
        return format!("{}{raw}", &stem[..authority_end]);
    }
    match stem.rfind('/') {
        Some(i) => format!("{}{raw}", &stem[..=i]),
        None => format!("{stem}{raw}"),
    }
}

fn parse(src: &str, opts: &ReadOpts) -> Result<Doc, ReadError> {
    let tokens = lex(src)?;
    let mut p = Parser {
        t: &tokens,
        i: 0,
        base: opts.base.clone(),
        prefixes: BTreeMap::new(),
    };
    let mut doc = Doc {
        order: Vec::new(),
        by_subject: BTreeMap::new(),
    };

    while p.i < p.t.len() {
        match p.peek().cloned() {
            Some(Tk::AtPrefix) | Some(Tk::KwPrefix) => {
                let sparql = matches!(p.peek(), Some(Tk::KwPrefix));
                p.i += 1;
                let (l, c) = p.pos();
                let label = match p.bump().map(|t| t.tk.clone()) {
                    Some(Tk::PName(label, local)) if local.is_empty() => label,
                    other => {
                        return Err(syntax(
                            l,
                            c,
                            describe(other.as_ref()),
                            "a prefix label ending in `:`",
                        ));
                    }
                };
                let (l, c) = p.pos();
                let ns = match p.bump().map(|t| t.tk.clone()) {
                    Some(Tk::IriRef(raw)) => p.resolve(&raw, l)?,
                    other => {
                        return Err(syntax(l, c, describe(other.as_ref()), "a <namespace IRI>"));
                    }
                };
                p.prefixes.insert(label, ns);
                if !sparql {
                    p.expect_dot()?;
                }
            }
            Some(Tk::AtBase) | Some(Tk::KwBase) => {
                let sparql = matches!(p.peek(), Some(Tk::KwBase));
                p.i += 1;
                let (l, c) = p.pos();
                let raw = match p.bump().map(|t| t.tk.clone()) {
                    Some(Tk::IriRef(raw)) => raw,
                    other => return Err(syntax(l, c, describe(other.as_ref()), "a <base IRI>")),
                };
                p.base = Some(p.resolve(&raw, l)?);
                if !sparql {
                    p.expect_dot()?;
                }
            }
            Some(Tk::Refused(_)) => {
                let (l, _) = p.pos();
                return Err(ReadError::BlankNodeRefused { line: l });
            }
            _ => {
                let subject = p.iri()?;
                if !doc.by_subject.contains_key(&subject) {
                    doc.order.push(subject.clone());
                    doc.by_subject.insert(subject.clone(), Vec::new());
                }
                loop {
                    while matches!(p.peek(), Some(Tk::Semi)) {
                        p.i += 1;
                    }
                    if matches!(p.peek(), Some(Tk::Dot)) {
                        p.i += 1;
                        break;
                    }
                    let (l, c) = p.pos();
                    let predicate = match p.peek().cloned() {
                        Some(Tk::A) => {
                            p.i += 1;
                            RDF_TYPE.to_string()
                        }
                        Some(Tk::IriRef(_)) | Some(Tk::PName(_, _)) => p.iri()?,
                        other => {
                            return Err(syntax(l, c, describe(other.as_ref()), "a predicate"));
                        }
                    };
                    loop {
                        let line = p.pos().0;
                        let object = p.object()?;
                        doc.by_subject
                            .get_mut(&subject)
                            .expect("the subject was inserted above")
                            .push((predicate.clone(), object, line));
                        if matches!(p.peek(), Some(Tk::Comma)) {
                            p.i += 1;
                            continue;
                        }
                        break;
                    }
                    match p.peek() {
                        Some(Tk::Semi) => continue,
                        Some(Tk::Dot) => {
                            p.i += 1;
                            break;
                        }
                        other => {
                            let (l, c) = p.pos();
                            return Err(syntax(
                                l,
                                c,
                                describe(other),
                                "`;` for another predicate or `.` to end the statement",
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(doc)
}

// ================================================================ building

type Preds = [(String, Obj, u32)];

fn term(local: &str) -> String {
    format!("{NS}{local}")
}

fn violated(subject: &str, predicate: &str, reason: &'static str) -> ReadError {
    ReadError::ContractViolated {
        subject: subject.to_string(),
        predicate: predicate.to_string(),
        reason,
    }
}

/// At most one value, or the contract violation that two of them are. The
/// shapes cap almost every property at one; where they do, a second value is
/// not extra information, it is a file whose meaning depends on statement order.
fn at_most_one<'a>(
    preds: &'a Preds,
    subject: &str,
    p: &str,
    label: &str,
) -> Result<Option<&'a Obj>, ReadError> {
    let mut found = None;
    for (pred, obj, _) in preds {
        if pred == p {
            if found.is_some() {
                return Err(violated(
                    subject,
                    label,
                    "the shapes allow this property at most once, and two values make which one \
                     wins a question about statement order",
                ));
            }
            found = Some(obj);
        }
    }
    Ok(found)
}

fn as_iri(o: &Obj, subject: &str, label: &str) -> Result<String, ReadError> {
    match o {
        Obj::Iri(i) => Ok(i.clone()),
        Obj::Lit { .. } => Err(violated(
            subject,
            label,
            "this property's value is an IRI; a literal here names something nothing can resolve",
        )),
    }
}

fn as_string(o: &Obj, subject: &str, label: &str) -> Result<String, ReadError> {
    match o {
        Obj::Lit { value, .. } => Ok(value.clone()),
        Obj::Iri(_) => Err(violated(
            subject,
            label,
            "this property's value is a literal, and an IRI here is a name where text was meant",
        )),
    }
}

fn all_iris(preds: &Preds, p: &str) -> Vec<String> {
    preds
        .iter()
        .filter(|(pred, _, _)| pred == p)
        .filter_map(|(_, o, _)| match o {
            Obj::Iri(i) => Some(i.clone()),
            Obj::Lit { .. } => None,
        })
        .collect()
}

fn slug_of(subject: &str, preds: &Preds) -> Result<Slug, ReadError> {
    let value =
        at_most_one(preds, subject, &term(terms::prop::SLUG), "hive:slug")?.ok_or_else(|| {
            ReadError::MissingRequired {
                subject: subject.to_string(),
                predicate: "hive:slug",
            }
        })?;
    let text = as_string(value, subject, "hive:slug")?;
    if last_segment(subject) != text {
        return Err(ReadError::SlugDoesNotMatchIri {
            subject: subject.to_string(),
            slug: text,
        });
    }
    Slug::parse(&text).map_err(|_| {
        violated(
            subject,
            "hive:slug",
            "a slug is kebab-case: lower-case ASCII letters, digits and single hyphens",
        )
    })
}

/// KNOWN LOSSINESS, RECORDED RATHER THAN HIDDEN. A language-tagged
/// `rdfs:label "Town hall"@en` loads, and the tag does not survive the way back
/// out: the model stores a label as a plain `String` and has nowhere to keep
/// one. The shapes permit the tag, so refusing the document here would make
/// this reader stricter than the contract it ships beside — which is its own
/// bug, and the worse of the two. Nothing this crate writes is ever tagged, so
/// the loss can only reach a document somebody hand-wrote in more than one
/// language; the fix when that matters is a label type that carries a tag, and
/// it is a change to the model rather than to this function.
fn label_of(subject: &str, preds: &Preds) -> Result<String, ReadError> {
    let value = at_most_one(preds, subject, RDFS_LABEL, "rdfs:label")?.ok_or_else(|| {
        ReadError::MissingRequired {
            subject: subject.to_string(),
            predicate: "rdfs:label",
        }
    })?;
    as_string(value, subject, "rdfs:label")
}

fn preds_of<'a>(
    doc: &'a Doc,
    subject: &str,
    expected: &'static str,
) -> Result<&'a Preds, ReadError> {
    doc.by_subject
        .get(subject)
        .map(|v| v.as_slice())
        .ok_or_else(|| ReadError::MissingRequired {
            subject: subject.to_string(),
            predicate: expected,
        })
}

/// The five classes this crate's OWN writer already types a subject with, via
/// `block()`'s `a {kind}` head. An `rdf:type` naming one of these is not extra
/// information — it is the type this crate is going to write back anyway — so
/// it is the one kind of `a` triple `extras()` may still drop. Anything else
/// (`ex:Building`, the `owl:NamedIndividual` every Protégé export adds) is a
/// host's own type assertion and `extras()` used to drop that indiscriminately
/// too: `extras()` filtered `p != RDF_TYPE` UNCONDITIONALLY, so a subject typed
/// `d:hall a hive:Tile , ex:Building` came back with `ex:Building` gone on the
/// very first save, silently, on every file a Protégé/OWL-API tool had ever
/// touched.
fn is_modeled_class(iri: &str) -> bool {
    [
        terms::class::DIAGRAM,
        terms::class::GROUP,
        terms::class::TILE,
        terms::class::LINK,
        terms::class::PLACEMENT,
    ]
    .iter()
    .any(|local| iri == term(local))
}

fn statement_of(p: &str, o: &Obj) -> Statement {
    Statement {
        predicate: Iri(p.to_string()),
        object: match o {
            Obj::Iri(i) => Term::Iri(Iri(i.clone())),
            Obj::Lit {
                value,
                datatype,
                lang,
            } => Term::Literal {
                value: value.clone(),
                datatype: datatype.clone().map(Iri),
                lang: lang.clone(),
            },
        },
    }
}

/// Everything the vocabulary does not define, kept VERBATIM and in order. A
/// component that silently dropped these would have an open shape and a closed
/// implementation, and a host would lose its own content one save at a time.
///
/// AN `rdf:type` IS NOW KEPT UNLESS ITS OBJECT IS ONE OF THE FIVE CLASSES THIS
/// CRATE MODELS — see `is_modeled_class`'s own doc. `write_turtle`'s `block()`
/// still writes exactly one `a {kind}` from the model rather than from this
/// list, so a class this crate itself assigns is never duplicated; what
/// changed is that a class it does NOT assign is no longer silently equated
/// with one that is.
fn extras(preds: &Preds, consumed: &[&str]) -> Vec<Statement> {
    preds
        .iter()
        .filter(|(p, o, _)| {
            if p == RDF_TYPE {
                !matches!(o, Obj::Iri(i) if is_modeled_class(i))
            } else {
                !consumed.contains(&p.as_str())
            }
        })
        .map(|(p, o, _)| statement_of(p, o))
        .collect()
}

/// Every predicate-object pair a subject carries, kept VERBATIM and in order —
/// including `rdf:type`, which `extras()` above is selective about and this is
/// not. Used only for a subject `read_turtle_all` never reaches through the
/// diagram's own walk: this crate has no model for it at all, so there is no
/// "consumed" list to filter against and nothing to be selective about.
fn raw_statements(preds: &Preds) -> Vec<Statement> {
    preds.iter().map(|(p, o, _)| statement_of(p, o)).collect()
}

struct ReadPlacement {
    id: TileId,
    cell: Cell,
    group: Option<GroupId>,
    represents: Option<Iri>,
    tile_subject: Option<String>,
}

fn read_placement(doc: &Doc, subject: &str, declared: Mode) -> Result<ReadPlacement, ReadError> {
    let preds = preds_of(doc, subject, "hive:col")?;

    // THE CLOSED SHAPE, IN RUST. Checked before anything else, because the
    // predicate a contributor has to delete is the one thing worth telling them
    // and every other error here would name something further down the file.
    const ALLOWED: [&str; 5] = [
        terms::prop::COL,
        terms::prop::ROW,
        terms::prop::IN_GROUP,
        terms::prop::REPRESENTS,
        terms::prop::TILE,
    ];
    for (p, _, _) in preds {
        if p == RDF_TYPE {
            continue;
        }
        let ok = p
            .strip_prefix(NS)
            .is_some_and(|local| ALLOWED.contains(&local));
        if !ok {
            return Err(violated(
                subject,
                p,
                "hive:Placement is the one closed shape in the contract: content copied onto a \
                 placement is cached beside a layout and nothing anywhere invalidates it",
            ));
        }
    }

    let coord = |p: &'static str, label: &'static str| -> Result<i32, ReadError> {
        let value = at_most_one(preds, subject, &term(p), label)?.ok_or_else(|| {
            ReadError::MissingRequired {
                subject: subject.to_string(),
                predicate: label,
            }
        })?;
        let text = as_string(value, subject, label)?;
        text.parse::<i32>().map_err(|_| {
            violated(
                subject,
                label,
                "a coordinate is a plain integer, which may be negative: the origin is wherever \
                 the first tile landed rather than a corner",
            )
        })
    };
    let col = coord(terms::prop::COL, "hive:col")?;
    let row = coord(terms::prop::ROW, "hive:row")?;

    let group = at_most_one(preds, subject, &term(terms::prop::IN_GROUP), "hive:inGroup")?
        .map(|o| as_iri(o, subject, "hive:inGroup"))
        .transpose()?
        .map(|iri| slug_from_iri(&iri, subject, "hive:inGroup").map(GroupId))
        .transpose()?;

    let represents = at_most_one(
        preds,
        subject,
        &term(terms::prop::REPRESENTS),
        "hive:represents",
    )?
    .map(|o| as_iri(o, subject, "hive:represents"))
    .transpose()?;
    let tile_subject = at_most_one(preds, subject, &term(terms::prop::TILE), "hive:tile")?
        .map(|o| as_iri(o, subject, "hive:tile"))
        .transpose()?;

    // Mode disagreement is reported BEFORE the exactly-one rule, because a
    // placement that points the wrong way is a mode problem and being told it
    // "draws nothing" would send the reader to fix the wrong thing.
    match (declared, &represents, &tile_subject) {
        (Mode::Pinned, _, Some(_)) => {
            return Err(ReadError::ModeDisagreesWithContent {
                declared,
                found: "hive:tile",
                subject: subject.to_string(),
            });
        }
        (Mode::Standalone, Some(_), _) => {
            return Err(ReadError::ModeDisagreesWithContent {
                declared,
                found: "hive:represents",
                subject: subject.to_string(),
            });
        }
        _ => {}
    }
    if represents.is_none() && tile_subject.is_none() {
        return Err(violated(
            subject,
            "hive:represents",
            "a placement draws external content or its own, never neither: an empty hexagon still \
             reserves its cell, so the next drop onto it is refused for a reason nothing on screen \
             explains",
        ));
    }

    // WHERE THE TileId COMES FROM. In standalone mode it is the tile's own slug,
    // which `hsh:SlugMatchesIri` already ties to the tile's IRI. In pinned mode
    // there is no tile subject and the placement's own IRI is the only identity
    // the file carries for that cell, so the writer's `at-` convention is read
    // back here — see `ttl::PLACEMENT_PREFIX`.
    let id = match &tile_subject {
        Some(t) => TileId(slug_from_iri(t, subject, "hive:tile")?),
        None => {
            let last = last_segment(subject);
            let bare = last.strip_prefix(PLACEMENT_PREFIX).unwrap_or(last);
            TileId(Slug::parse(bare).map_err(|_| {
                violated(
                    subject,
                    "hive:placement",
                    "a placement's own IRI is the only name a pinned file has for its tile, so its \
                     last segment must be a kebab-case slug",
                )
            })?)
        }
    };

    Ok(ReadPlacement {
        id,
        cell: Cell { col, row },
        group,
        represents: represents.map(Iri),
        tile_subject,
    })
}

fn slug_from_iri(iri: &str, subject: &str, label: &'static str) -> Result<Slug, ReadError> {
    Slug::parse(last_segment(iri)).map_err(|_| {
        violated(
            subject,
            label,
            "the last segment of this IRI is the subject's slug, and a slug is kebab-case",
        )
    })
}

/// Builds one diagram AND returns every subject its own walk reached — the
/// diagram itself, every group, every placement, every standalone tile, every
/// link. `read_turtle_all` needs this list for exactly one job: subtracting it
/// from the whole document to find what nothing here reaches at all, so THAT
/// can be preserved instead of vanishing on the next save. See
/// `Diagram::unreached`'s own doc.
fn build(doc: &Doc, subject: &str) -> Result<(Diagram, BTreeSet<String>), ReadError> {
    let mut consumed: BTreeSet<String> = BTreeSet::new();
    consumed.insert(subject.to_string());

    let preds = preds_of(doc, subject, "hive:slug")?;
    let slug = slug_of(subject, preds)?;
    let label = label_of(subject, preds)?;

    let mode_iri =
        at_most_one(preds, subject, &term(terms::prop::MODE), "hive:mode")?.ok_or_else(|| {
            ReadError::MissingRequired {
                subject: subject.to_string(),
                predicate: "hive:mode",
            }
        })?;
    let mode_iri = as_iri(mode_iri, subject, "hive:mode")?;
    let mode = mode_iri
        .strip_prefix(NS)
        .and_then(Mode::from_term)
        .ok_or_else(|| ReadError::UnknownMode(mode_iri.clone()))?;

    let lattice_iri = at_most_one(preds, subject, &term(terms::prop::LATTICE), "hive:lattice")?
        .ok_or_else(|| ReadError::MissingRequired {
            subject: subject.to_string(),
            predicate: "hive:lattice",
        })?;
    let lattice_iri = as_iri(lattice_iri, subject, "hive:lattice")?;
    let convention = lattice_iri
        .strip_prefix(NS)
        .and_then(LatticeConvention::from_term)
        .ok_or_else(|| ReadError::UnknownLatticeConvention(lattice_iri.clone()))?;

    let note = at_most_one(preds, subject, &term(terms::prop::NOTE), "hive:note")?
        .map(|o| as_string(o, subject, "hive:note"))
        .transpose()?;
    let generator = at_most_one(
        preds,
        subject,
        &term(terms::prop::GENERATOR),
        "hive:generator",
    )?
    .map(|o| as_string(o, subject, "hive:generator"))
    .transpose()?;
    let generated_at = at_most_one(
        preds,
        subject,
        &term(terms::prop::GENERATED_AT),
        "hive:generatedAt",
    )?
    .map(|o| as_string(o, subject, "hive:generatedAt"))
    .transpose()?
    .map(|s| {
        Timestamp::parse(&s).map_err(|_| {
            violated(
                subject,
                "hive:generatedAt",
                "an xsd:dateTime with an explicit time zone; a zoneless one compares wrongly the \
                 moment a reader sits in another zone",
            )
        })
    })
    .transpose()?;

    let pinned_to = at_most_one(
        preds,
        subject,
        &term(terms::prop::PINNED_TO),
        "hive:pinnedTo",
    )?
    .map(|o| as_iri(o, subject, "hive:pinnedTo"))
    .transpose()?;
    let revision = at_most_one(
        preds,
        subject,
        &term(terms::prop::PINNED_REVISION),
        "hive:pinnedRevision",
    )?
    .map(|o| as_string(o, subject, "hive:pinnedRevision"))
    .transpose()?;

    // hsh:ModeIsHonest, at the diagram level. Both halves are refusals rather
    // than repairs: dropping the value would leave the document looking
    // regenerable to the next reader while nothing will ever regenerate it.
    if mode == Mode::Standalone {
        if pinned_to.is_some() {
            return Err(ReadError::ModeDisagreesWithContent {
                declared: mode,
                found: "hive:pinnedTo",
                subject: subject.to_string(),
            });
        }
        if revision.is_some() {
            return Err(ReadError::ModeDisagreesWithContent {
                declared: mode,
                found: "hive:pinnedRevision",
                subject: subject.to_string(),
            });
        }
    }

    let mut groups: BTreeMap<GroupId, Group> = BTreeMap::new();
    for g_iri in all_iris(preds, &term(terms::prop::GROUP)) {
        consumed.insert(g_iri.clone());
        let g_preds = preds_of(doc, &g_iri, "hive:slug")?;
        let g_slug = slug_of(&g_iri, g_preds)?;
        let style_key = at_most_one(
            g_preds,
            &g_iri,
            &term(terms::prop::STYLE_KEY),
            "hive:styleKey",
        )?
        .map(|o| as_iri(o, &g_iri, "hive:styleKey"))
        .transpose()?
        .map(Iri);
        let g_note = at_most_one(g_preds, &g_iri, &term(terms::prop::NOTE), "hive:note")?
            .map(|o| as_string(o, &g_iri, "hive:note"))
            .transpose()?;
        groups.insert(
            GroupId(g_slug),
            Group {
                // A group's label is OPTIONAL — see the writer. An absent one
                // reads as the empty string, which is what `Group.label` holds
                // for "no name" and what the writer omits again on the way out.
                label: at_most_one(g_preds, &g_iri, RDFS_LABEL, "rdfs:label")?
                    .map(|v| as_string(v, &g_iri, "rdfs:label"))
                    .transpose()?
                    .unwrap_or_default(),
                style_key,
                note: g_note,
                extra: extras(
                    g_preds,
                    &[
                        &term(terms::prop::SLUG),
                        RDFS_LABEL,
                        &term(terms::prop::STYLE_KEY),
                        &term(terms::prop::NOTE),
                    ],
                ),
            },
        );
    }

    let placement_iris = all_iris(preds, &term(terms::prop::PLACEMENT));
    if placement_iris.is_empty() {
        return Err(ReadError::MissingRequired {
            subject: subject.to_string(),
            predicate: "hive:placement",
        });
    }

    let mut cells: BTreeMap<TileId, Cell> = BTreeMap::new();
    let mut pinned: BTreeMap<TileId, PinnedTile> = BTreeMap::new();
    let mut own: BTreeMap<TileId, OwnTile> = BTreeMap::new();

    for p_iri in &placement_iris {
        consumed.insert(p_iri.clone());
        let p = read_placement(doc, p_iri, mode)?;
        if cells.contains_key(&p.id) {
            return Err(violated(
                p_iri,
                "hive:placement",
                "two placements name one tile, which would be one tile in two cells at once",
            ));
        }
        cells.insert(p.id.clone(), p.cell);
        match (&p.represents, &p.tile_subject) {
            (Some(r), _) => {
                pinned.insert(
                    p.id.clone(),
                    PinnedTile {
                        group: p.group.clone(),
                        represents: r.clone(),
                    },
                );
            }
            (None, Some(t_iri)) => {
                consumed.insert(t_iri.clone());
                let t_preds = preds_of(doc, t_iri, "rdfs:label")?;
                // Read for its side effect: a tile whose slug disagrees with its
                // own IRI is refused here rather than silently re-homed, which is
                // how a document gains a duplicate subject on the next export.
                let _ = slug_of(t_iri, t_preds)?;
                let style_key = at_most_one(
                    t_preds,
                    t_iri,
                    &term(terms::prop::STYLE_KEY),
                    "hive:styleKey",
                )?
                .map(|o| as_iri(o, t_iri, "hive:styleKey"))
                .transpose()?
                .map(Iri);
                let comment = at_most_one(t_preds, t_iri, RDFS_COMMENT, "rdfs:comment")?
                    .map(|o| as_string(o, t_iri, "rdfs:comment"))
                    .transpose()?;
                own.insert(
                    p.id.clone(),
                    OwnTile {
                        group: p.group.clone(),
                        label: label_of(t_iri, t_preds)?,
                        comment,
                        style_key,
                        extra: extras(
                            t_preds,
                            &[
                                &term(terms::prop::SLUG),
                                RDFS_LABEL,
                                RDFS_COMMENT,
                                &term(terms::prop::STYLE_KEY),
                            ],
                        ),
                    },
                );
            }
            (None, None) => unreachable!("read_placement refuses a placement that draws nothing"),
        }
    }

    let content = match mode {
        Mode::Pinned => Content::Pinned {
            source: Iri(pinned_to.ok_or_else(|| ReadError::MissingRequired {
                subject: subject.to_string(),
                predicate: "hive:pinnedTo",
            })?),
            revision,
            tiles: pinned,
        },
        Mode::Standalone => Content::Standalone { tiles: own },
    };

    // ---- links. Ends are PLACEMENT subjects in the document and TILE ids in
    // the model, so each is resolved back through the placement it names — the
    // same indirection the writer performs outward.
    let mut links: BTreeMap<LinkId, Link> = BTreeMap::new();
    for l_iri in all_iris(preds, &term(terms::prop::LINK)) {
        consumed.insert(l_iri.clone());
        let l_preds = preds_of(doc, &l_iri, "hive:from")?;
        let l_slug = last_segment(&l_iri).to_string();
        let end = |p: &str, name: &'static str| -> Result<TileId, ReadError> {
            let v = at_most_one(l_preds, &l_iri, &term(p), name)?.ok_or_else(|| {
                ReadError::MissingRequired {
                    subject: l_iri.clone(),
                    predicate: name,
                }
            })?;
            let iri = as_iri(v, &l_iri, name)?;
            let local = last_segment(&iri).to_string();
            let tile = local.strip_prefix(PLACEMENT_PREFIX).unwrap_or(&local);
            Ok(TileId(Slug::parse(tile).map_err(ReadError::BadSlug)?))
        };
        let routing = at_most_one(l_preds, &l_iri, &term(terms::prop::ROUTING), "hive:routing")?
            .map(|v| as_iri(v, &l_iri, "hive:routing"))
            .transpose()?
            .map(|iri| {
                let local = last_segment(&iri).to_string();
                Routing::from_term(&local).ok_or(ReadError::UnknownRouting(local))
            })
            .transpose()?
            .unwrap_or_default();
        links.insert(
            LinkId(Slug::parse(&l_slug).map_err(ReadError::BadSlug)?),
            Link {
                from: end(terms::prop::FROM, "hive:from")?,
                to: end(terms::prop::TO, "hive:to")?,
                label: at_most_one(l_preds, &l_iri, RDFS_LABEL, "rdfs:label")?
                    .map(|v| as_string(v, &l_iri, "rdfs:label"))
                    .transpose()?,
                routing,
                style_key: at_most_one(
                    l_preds,
                    &l_iri,
                    &term(terms::prop::STYLE_KEY),
                    "hive:styleKey",
                )?
                .map(|v| as_iri(v, &l_iri, "hive:styleKey"))
                .transpose()?
                .map(Iri),
                extra: extras(
                    l_preds,
                    &[
                        // `hive:slug` MUST be in this list. It is the link's own
                        // identity, not a host predicate — left out, it is
                        // preserved as an extra and written a second time on the
                        // next export, so the document grows a duplicate slug
                        // every round trip.
                        &term(terms::prop::SLUG),
                        &term(terms::prop::FROM),
                        &term(terms::prop::TO),
                        &term(terms::prop::ROUTING),
                        &term(terms::prop::STYLE_KEY),
                        RDFS_LABEL,
                        RDF_TYPE,
                    ],
                ),
            },
        );
    }

    let d = Diagram::try_new(DiagramSpec {
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
        // `hsh:DiagramShape` is not closed, and this crate read a host's own
        // predicates off the diagram subject and threw them away for its whole
        // first life — an open shape with a closed implementation, which is
        // exactly what `OwnTile::extra` exists to prevent one level down.
        extra: extras(
            preds,
            &[
                &term(terms::prop::SLUG),
                RDFS_LABEL,
                &term(terms::prop::NOTE),
                &term(terms::prop::MODE),
                &term(terms::prop::LATTICE),
                &term(terms::prop::PINNED_TO),
                &term(terms::prop::PINNED_REVISION),
                &term(terms::prop::GENERATOR),
                &term(terms::prop::GENERATED_AT),
                &term(terms::prop::GROUP),
                &term(terms::prop::PLACEMENT),
                // AND hive:link, for the reason the link's own slug is in its
                // list: a term this crate models is not a host predicate, and
                // one left out of the consumed set is read as an extra and
                // written a SECOND time on the next export. The fixed-point
                // assertion in `modes.rs` is what catches it; nothing else would.
                &term(terms::prop::LINK),
                RDF_TYPE,
            ],
        ),
    })
    .map_err(ReadError::Model)?;
    Ok((d, consumed))
}

/// Every diagram in the document. Needed because a corpus ships a pinned and a
/// standalone diagram in one file — a shape targeting only one of the two modes
/// selects no focus node otherwise, and a shape that checks nothing is a rule
/// nobody is enforcing.
///
/// A SUBJECT NOTHING HERE REACHES USED TO SIMPLY VANISH ON THE NEXT SAVE. A
/// conformant `d:annex a hive:Tile ; hive:slug "annex" ; rdfs:label "…" .` that
/// is not placed anywhere, a host's own `ex:style/civic a ex:Style` subject, an
/// ontology header — none of these are reached by walking `hive:placement`,
/// `hive:group`, `hive:tile` or `hive:link` from the diagram, so nothing about
/// them was ever read, and a re-export silently dropped them.
///
/// FIXED ONLY WHEN THE DOCUMENT HOLDS EXACTLY ONE DIAGRAM. With two or more
/// there is no principled diagram to attribute a stray subject to — attaching
/// it to "the first one" would work until the file is re-exported one diagram
/// at a time (as `one_document_may_hold_one_diagram_of_each_mode` in
/// `modes.rs` does), at which point the stray subject either duplicates across
/// both exports or vanishes from whichever export it was not attached to. That
/// is the exact "read as an extra, written twice" trap this file's own header
/// warns about, reproduced one level up; refusing to guess is safer than
/// guessing wrong. So a multi-diagram document still loses subjects nothing
/// reaches — a known, deliberate limitation, not a claim this closes it.
pub fn read_turtle_all(src: &str, o: &ReadOpts) -> Result<Vec<Diagram>, ReadError> {
    let doc = parse(src, o)?;
    let diagram_class = format!("{NS}{}", terms::class::DIAGRAM);
    let mut built: Vec<(Diagram, BTreeSet<String>)> = Vec::new();
    for subject in &doc.order {
        let preds = &doc.by_subject[subject];
        let is_diagram = preds
            .iter()
            .any(|(p, obj, _)| p == RDF_TYPE && matches!(obj, Obj::Iri(i) if *i == diagram_class));
        if is_diagram {
            built.push(build(&doc, subject)?);
        }
    }
    if let [(d, consumed)] = built.as_mut_slice() {
        let unreached: Vec<(Iri, Vec<Statement>)> = doc
            .order
            .iter()
            .filter(|s| !consumed.contains(s.as_str()))
            .map(|s| (Iri(s.clone()), raw_statements(&doc.by_subject[s])))
            .collect();
        d.set_unreached(unreached);
    }
    Ok(built.into_iter().map(|(d, _)| d).collect())
}

/// One diagram. A document holding several is an error naming all of them
/// rather than a guess: whichever one was picked, the others would be invisible
/// to a caller who never learns they exist.
pub fn read_turtle(src: &str, o: &ReadOpts) -> Result<Diagram, ReadError> {
    let mut all = read_turtle_all(src, o)?;
    if let Some(want) = &o.slug {
        return match all.iter().position(|d| d.slug() == want) {
            Some(i) => Ok(all.remove(i)),
            None => Err(ReadError::NoDiagram),
        };
    }
    match all.len() {
        0 => Err(ReadError::NoDiagram),
        1 => Ok(all.remove(0)),
        _ => Err(ReadError::AmbiguousDocument {
            slugs: all.iter().map(|d| d.slug().clone()).collect(),
        }),
    }
}
