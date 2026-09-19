//! The vocabulary this crate owns, checked where it is owned.
//!
//! WHAT BREAKS WITHOUT THIS FILE. `ns.ttl` and `shapes.ttl` are data, not code.
//! Nothing in this workspace compiles them, links them or reads them at run
//! time, so nothing here notices when an edit to them is wrong — and they are
//! the one artefact of this repository that leaves it VERBATIM. A consumer
//! vendors both files into its own tree and runs them, unmodified, over a real
//! corpus in its own validation gate. Without this file the first reader of a
//! bad edit is that gate: one repository away, inside bytes that repository
//! copied and is forbidden to fix locally, on a merge request that has nothing
//! to do with the vocabulary. The person who has to act is not the person who
//! typed it, and the fix has to travel back through a release.
//!
//! Every assertion below is therefore a failure that has a known address in
//! somebody else's pipeline:
//!
//!   1. The files scan, and every prefixed name has a declared prefix. A
//!      vocabulary that does not parse does not fail on its own line — it takes
//!      the consumer's entire gate down, and the consumer's whole corpus goes
//!      unchecked until somebody notices the run was short.
//!   2. No property the shapes attach to more than one class declares an
//!      `rdfs:domain`. At least one real consumer reads `rdfs:domain`
//!      entailment-free as a constraint on instance data; a domain on
//!      `hive:slug` — carried by Diagram, Group AND Tile — is then a hard
//!      validation error on every CORRECT document, with nothing wrong locally.
//!   3. Zero relative IRI refs and no `@base` in either file. A consumer that
//!      vendors these bytes prepends its own `@base` to satisfy its own rules.
//!      That prepend stays a prepend — a pure, reviewable, mechanical transform
//!      — only while there is nothing here for it to silently rewrite.
//!   4. Every node that states a constraint carries an `sh:message`, including
//!      the four nested property shapes inside the `sh:xone` that can never be
//!      shown. Without them a contributor gets "Constraint Violation in
//!      MinCountConstraintComponent" and an IRI; and a consumer gate that
//!      requires a message on every constraint-stating node reports four
//!      violations in the vendored copy, which is the one place it cannot fix.
//!   5. Every prefix the SPARQL constraints use is declared in `sh:declare`. An
//!      undeclared one does not fail loudly: it fails inside the validator at
//!      query-parse time, in a message that names no shape.
//!   6. The version the TBox states is the crate version. "A term cannot change
//!      without a version bump" is only true if something fails when it does not
//!      — this is that something.
//!   7. Both files are written in the namespaces this crate exports, and neither
//!      mentions the host the namespace used to be minted from. A term whose IRI
//!      moved is a new term; every document already written keeps naming the old
//!      one, and nothing anywhere says the two were meant to be the same.
//!
//! WHY THIS TEST DOES NOT USE AN RDF CRATE. `honeycomb-core` has zero
//! dependencies and keeps zero — that is the property which lets a host-side
//! generator lay a diagram out without linking a UI framework, and a
//! dev-dependency would spend it for a test. So the scanning below is
//! hand-written, like the serialiser, and it is deliberately narrow. What it
//! does and does not establish is written out above `scan`, so that a pass here
//! is never read as "these files are valid SHACL".
//!
//! WHAT THIS FILE NEEDS THAT DOES NOT EXIST YET: `honeycomb_core::TBOX_VERSION`.
//! It is named rather than worked around — re-reading `CARGO_PKG_VERSION` here
//! would compare the file against a value the crate does not export, and pass
//! while the constant the writer uses said something else. Until it exists this
//! file does not compile and the error names it; once it does, all seven tests
//! fail saying `ns.ttl` and `shapes.ttl` are not in the repository, which is the
//! second thing to add and the entire reason the first one is worth having.

use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------- the files
//
// At the REPOSITORY ROOT, not inside this crate, because that is where a
// consumer fetches them from: the vendoring script reads
// raw.githubusercontent.com/<repo>/<tag>/ns.ttl. This crate owns the bytes and
// is where they are checked; the root is where they are published from.

const NS_FILE: &str = "ns.ttl";
const SHAPES_FILE: &str = "shapes.ttl";

// Namespaces used by name below. Written out rather than reconstructed from a
// prefix in the file under test: a test that resolved `sh:` through the very
// prefix map it is checking would agree with any rebinding of it.
const SH: &str = "http://www.w3.org/ns/shacl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const OWL: &str = "http://www.w3.org/2002/07/owl#";
const VANN: &str = "http://purl.org/vocab/vann/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The SHACL core constraint parameters. A node carrying one of these STATES a
/// constraint, and a constraint that fires with no `sh:message` prints the name
/// of a constraint component and an IRI — which is not something a contributor
/// who has never read SHACL can act on.
///
/// `sh:sparql` is deliberately ABSENT: the message belongs on the
/// `sh:SPARQLConstraint` node itself, so that a shape carrying three of them
/// (`hsh:ModeIsHonest`) reports three different failures with three different
/// messages rather than one message copied over all of them. That case is
/// checked separately, by type.
const CONSTRAINT_PARAMS: &[&str] = &[
    "class", "datatype", "nodeKind", "minCount", "maxCount", "minExclusive",
    "minInclusive", "maxExclusive", "maxInclusive", "minLength", "maxLength",
    "pattern", "languageIn", "uniqueLang", "equals", "disjoint", "lessThan",
    "lessThanOrEquals", "not", "and", "or", "xone", "node", "qualifiedValueShape",
    "qualifiedMinCount", "qualifiedMaxCount", "closed", "hasValue", "in",
];

fn path_to(name: &str) -> String {
    format!("{}/../{}", env!("CARGO_MANIFEST_DIR"), name)
}

fn load(name: &str) -> Doc {
    let path = path_to(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{name} could not be read at {path}: {e}\n\
             This crate OWNS the honeycomb vocabulary: these bytes are the \
             authoritative copy, they are what a consumer vendors verbatim, and \
             this file is the only place their consumer-safety is enforced \
             before they leave the repository."
        )
    });
    match parse(&text) {
        Ok(doc) => doc,
        Err(e) => panic!(
            "{name} does not scan: {e}\n\
             A vocabulary that does not parse does not fail on its own line in a \
             consumer's gate — it takes the whole validation run down, and every \
             other file that run was supposed to check goes unchecked with it."
        ),
    }
}

// ---------------------------------------------------------------- the scanner
//
// WHAT IT ESTABLISHES. That the file is a sequence of `.`-terminated statements
// whose IRI refs, literals (short, long, escaped, language-tagged, typed),
// comments, blank-node property lists and collections are all closed; that
// every prefixed name resolves against a prefix the file declares; and — by
// walking predicate/object pairs one nesting level at a time — which predicates
// each node carries AT ITS OWN LEVEL.
//
// WHAT IT DOES NOT ESTABLISH, stated so that a pass here is never mistaken for
// validity:
//
//   * It builds NO GRAPH. Nothing is deduplicated, nothing is merged across
//     subjects, and no `rdf:List` is interpreted beyond finding the members
//     written inside its parentheses.
//   * It does not check Turtle's grammar for names, numbers or IRIs — a
//     malformed `hive:1bad` or `<not an iri>` scans as a token here.
//   * It does not unescape literals (`\n`, `\uXXXX`) or verify that a lexical
//     form matches its datatype.
//   * It does not read the SPARQL-style dot-less `PREFIX` / `BASE` forms. A
//     test asserts they are absent rather than pretending to understand them.
//   * It knows NO SHACL SEMANTICS. A shape can scan perfectly here and
//     constrain nothing, target nothing, or contradict another shape.
//   * It does not run SHACL at all. pyshacl over committed fixtures does that
//     in CI; a consumer's gate does it over a real corpus. This file is the
//     cheap always-on half, and its job is to stop bytes leaving the repository
//     in a state that breaks either of those.
//
// ONE DELIBERATE ASYMMETRY. The consumer's gate exempts the direct members of
// an `sh:or` / `sh:and` / `sh:xone` list from the sh:message requirement. This
// scanner has no such exemption — it asks only whether a node carries a
// constraint parameter at its own level, which the members do not (they carry
// `sh:property`). If a member ever does carry one, this test fails where the
// gate would not. That is the safe direction: it can cost a message nobody
// reads, never hide one somebody needed.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    /// The text between `<` and `>`.
    Iri(String),
    /// The text between the quotes, escapes left as written.
    Literal(String),
    /// A prefixed name, `a`, a number, a boolean, a `@lang` tag, a `@prefix`
    /// directive keyword, or a `^^`-prefixed datatype.
    Word(String),
    Open,
    Close,
    ListOpen,
    ListClose,
    Semi,
    Comma,
    Dot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    tok: Tok,
    line: u32,
}

fn scan(src: &str) -> Result<Vec<Token>, String> {
    let b: Vec<char> = src.chars().collect();
    let mut out: Vec<Token> = Vec::new();
    let mut i = 0usize;
    let mut line = 1u32;

    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // A `#` outside an IRI ref and outside a literal starts a comment. Both
        // of those are consumed whole below, which is why `[/#]` inside a SPARQL
        // string and the `#` ending a namespace IRI do not truncate this file.
        if c == '#' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '<' {
            let opened = line;
            let mut j = i + 1;
            let mut s = String::new();
            loop {
                if j >= b.len() || b[j] == '\n' {
                    return Err(format!("line {opened}: an IRI ref opened with `<` and was never closed"));
                }
                if b[j] == '>' {
                    break;
                }
                s.push(b[j]);
                j += 1;
            }
            out.push(Token { tok: Tok::Iri(s), line });
            i = j + 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let (lit, next, next_line) = scan_literal(&b, i, line)?;
            out.push(Token { tok: Tok::Literal(lit), line });
            i = next;
            line = next_line;
            continue;
        }
        let single = match c {
            '[' => Some(Tok::Open),
            ']' => Some(Tok::Close),
            '(' => Some(Tok::ListOpen),
            ')' => Some(Tok::ListClose),
            ';' => Some(Tok::Semi),
            ',' => Some(Tok::Comma),
            _ => None,
        };
        if let Some(t) = single {
            out.push(Token { tok: t, line });
            i += 1;
            continue;
        }

        let start = i;
        while i < b.len()
            && !b[i].is_whitespace()
            && !matches!(b[i], '#' | '<' | '>' | '"' | '\'' | '[' | ']' | '(' | ')' | ';' | ',')
        {
            i += 1;
        }
        if start == i {
            return Err(format!("line {line}: nothing can begin with {c:?}, and the scanner cannot get past it"));
        }
        let mut w: String = b[start..i].iter().collect();
        // A trailing `.` ends a statement: Turtle forbids one as the last
        // character of a prefixed name, so this cannot eat part of a term.
        let mut dots = 0;
        while w.ends_with('.') {
            w.pop();
            dots += 1;
        }
        if !w.is_empty() {
            out.push(Token { tok: Tok::Word(w), line });
        }
        for _ in 0..dots {
            out.push(Token { tok: Tok::Dot, line });
        }
    }
    Ok(out)
}

/// Returns the literal's inner text, the index after it, and the line it ended
/// on. Long literals are what carry the `sh:message` and `sh:select` bodies, and
/// those contain `#`, `"` and `[` — so getting this wrong does not produce a
/// small error, it produces a scanner that reads prose as syntax.
fn scan_literal(b: &[char], i: usize, line: u32) -> Result<(String, usize, u32), String> {
    let q = b[i];
    let long = i + 2 < b.len() && b[i + 1] == q && b[i + 2] == q;
    let mut j = i + if long { 3 } else { 1 };
    let mut s = String::new();
    let mut l = line;
    loop {
        if j >= b.len() {
            return Err(format!("line {line}: a literal opened with {q:?} and was never closed"));
        }
        if b[j] == '\\' {
            if j + 1 >= b.len() {
                return Err(format!("line {line}: a literal ends in a lone backslash"));
            }
            s.push(b[j]);
            s.push(b[j + 1]);
            if b[j + 1] == '\n' {
                l += 1;
            }
            j += 2;
            continue;
        }
        if b[j] == q {
            if !long {
                return Ok((s, j + 1, l));
            }
            if j + 2 < b.len() && b[j + 1] == q && b[j + 2] == q {
                return Ok((s, j + 3, l));
            }
        }
        if b[j] == '\n' {
            if !long {
                return Err(format!("line {line}: a single-quoted literal ran past the end of its line"));
            }
            l += 1;
        }
        s.push(b[j]);
        j += 1;
    }
}

#[derive(Debug)]
struct Doc {
    text: String,
    prefixes: BTreeMap<String, String>,
    /// Lines carrying an `@base`. A vendoring consumer prepends its own; a
    /// second one here would make that prepend a rewrite.
    bases: Vec<u32>,
    /// One entry per `.`-terminated statement: the subject token followed by
    /// its predicate/object tokens. Directives are not included.
    statements: Vec<Vec<Token>>,
    /// Every `<...>` in the file, so relative refs can be found wherever they
    /// hide — including inside a blank node or a collection.
    iri_refs: Vec<Token>,
}

fn parse(src: &str) -> Result<Doc, String> {
    let toks = scan(src)?;
    let mut stack: Vec<(Tok, u32)> = Vec::new();
    let mut cur: Vec<Token> = Vec::new();
    let mut raw: Vec<Vec<Token>> = Vec::new();
    let mut iri_refs = Vec::new();

    for t in &toks {
        if let Tok::Iri(_) = t.tok {
            iri_refs.push(t.clone());
        }
        match t.tok {
            Tok::Open | Tok::ListOpen => {
                stack.push((t.tok.clone(), t.line));
                cur.push(t.clone());
            }
            Tok::Close | Tok::ListClose => {
                let want = if t.tok == Tok::Close { Tok::Open } else { Tok::ListOpen };
                match stack.pop() {
                    Some((open, _)) if open == want => {}
                    Some((_, opened)) => {
                        return Err(format!(
                            "line {}: this closes a group that was opened on line {opened} with the other bracket",
                            t.line
                        ))
                    }
                    None => return Err(format!("line {}: a group is closed that was never opened", t.line)),
                }
                cur.push(t.clone());
            }
            Tok::Dot if stack.is_empty() => {
                if !cur.is_empty() {
                    raw.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(t.clone()),
        }
    }
    if let Some((_, opened)) = stack.last() {
        return Err(format!("a group opened on line {opened} is never closed"));
    }
    if let Some(t) = cur.first() {
        return Err(format!(
            "the statement beginning on line {} is never terminated with `.`",
            t.line
        ));
    }

    let mut prefixes = BTreeMap::new();
    let mut bases = Vec::new();
    let mut statements = Vec::new();
    for st in raw {
        let head = match &st[0].tok {
            Tok::Word(w) => w.clone(),
            _ => String::new(),
        };
        if head.eq_ignore_ascii_case("@prefix") {
            match (st.get(1).map(|t| &t.tok), st.get(2).map(|t| &t.tok)) {
                (Some(Tok::Word(name)), Some(Tok::Iri(ns))) if name.ends_with(':') => {
                    prefixes.insert(name.trim_end_matches(':').to_string(), ns.clone());
                }
                _ => {
                    return Err(format!(
                        "line {}: an @prefix directive is not `@prefix name: <iri> .`",
                        st[0].line
                    ))
                }
            }
            continue;
        }
        if head.eq_ignore_ascii_case("@base") {
            bases.push(st[0].line);
            continue;
        }
        statements.push(st);
    }

    Ok(Doc { text: src.to_string(), prefixes, bases, statements, iri_refs })
}

impl Doc {
    /// The full IRI a token names, or `None` for a literal, a number, or a
    /// keyword. Comparisons are made on full IRIs throughout, so that a test
    /// cannot be fooled by a prefix quietly rebound to another namespace.
    fn expand(&self, t: &Token) -> Option<String> {
        match &t.tok {
            Tok::Iri(s) => Some(s.clone()),
            Tok::Word(w) => {
                if w == "a" {
                    return Some(RDF_TYPE.to_string());
                }
                let w = w.strip_prefix("^^").unwrap_or(w);
                if w.starts_with('@') {
                    return None;
                }
                let (p, local) = w.split_once(':')?;
                let ns = self.prefixes.get(p)?;
                Some(format!("{ns}{local}"))
            }
            _ => None,
        }
    }

    /// `hive:col` rather than the full IRI, for a message a reader can find in
    /// the file with one search.
    fn short(&self, iri: &str) -> String {
        let mut best: Option<(&String, &String)> = None;
        for (p, ns) in &self.prefixes {
            if iri.starts_with(ns.as_str())
                && best.map(|(_, b)| ns.len() > b.len()).unwrap_or(true)
            {
                best = Some((p, ns));
            }
        }
        match best {
            Some((p, ns)) => format!("{p}:{}", &iri[ns.len()..]),
            None => format!("<{iri}>"),
        }
    }

    fn name_of(&self, t: &Token) -> String {
        match self.expand(t) {
            Some(iri) => self.short(&iri),
            None => match &t.tok {
                Tok::Word(w) => w.clone(),
                Tok::Literal(l) => format!("{:?}", l.chars().take(40).collect::<String>()),
                Tok::Iri(s) => format!("<{s}>"),
                _ => "?".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone)]
enum Obj {
    Single(Token),
    /// The tokens between `[` and `]`.
    Block(Vec<Token>),
    /// The tokens between `(` and `)`.
    Collection(Vec<Token>),
}

fn read_object(toks: &[Token], i: usize) -> Option<(Obj, usize)> {
    match toks.get(i)?.tok {
        Tok::Open => {
            let (inner, next) = group(toks, i, Tok::Open, Tok::Close)?;
            Some((Obj::Block(inner), next))
        }
        Tok::ListOpen => {
            let (inner, next) = group(toks, i, Tok::ListOpen, Tok::ListClose)?;
            Some((Obj::Collection(inner), next))
        }
        _ => Some((Obj::Single(toks[i].clone()), i + 1)),
    }
}

fn group(toks: &[Token], i: usize, open: Tok, close: Tok) -> Option<(Vec<Token>, usize)> {
    let mut depth = 0usize;
    for (j, t) in toks.iter().enumerate().skip(i) {
        if t.tok == open {
            depth += 1;
        } else if t.tok == close {
            depth -= 1;
            if depth == 0 {
                return Some((toks[i + 1..j].to_vec(), j + 1));
            }
        }
    }
    None
}

/// The predicate/object pairs written at ONE level: the objects of a nested
/// `[ ... ]` come back as a slice rather than being flattened into the parent.
/// That distinction is the whole point — "does this node state a constraint" is
/// a question about the node's own level, and flattening would make every node
/// shape appear to state every constraint its property shapes do.
fn pairs(toks: &[Token]) -> Vec<(Token, Obj)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        while i < toks.len() && matches!(toks[i].tok, Tok::Semi | Tok::Comma | Tok::Dot) {
            i += 1;
        }
        if i >= toks.len() {
            break;
        }
        let pred = toks[i].clone();
        i += 1;
        // `,` repeats the predicate with another object; anything else ends it.
        while let Some((obj, next)) = read_object(toks, i) {
            out.push((pred.clone(), obj));
            i = next;
            if toks.get(i).map(|t| t.tok == Tok::Comma).unwrap_or(false) {
                i += 1;
                continue;
            }
            break;
        }
    }
    out
}

fn items(inner: &[Token]) -> Vec<Obj> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some((obj, next)) = read_object(inner, i) {
        out.push(obj);
        i = next;
    }
    out
}

#[derive(Debug)]
struct Pred {
    iri: String,
    obj_iri: Option<String>,
    obj_lit: Option<String>,
    line: u32,
}

/// One node — a named subject or a blank-node property list — and the
/// predicates it carries AT ITS OWN LEVEL.
#[derive(Debug)]
struct Node {
    /// Reads like the consumer gate's own wording, e.g. "the hive:represents
    /// property shape of member 1 of the sh:xone list of
    /// hsh:PlacementDrawsOneThing", so a failure here names the same thing the
    /// gate would name.
    label: String,
    line: u32,
    preds: Vec<Pred>,
}

impl Node {
    fn has(&self, iri: &str) -> bool {
        self.preds.iter().any(|p| p.iri == iri)
    }
    fn lit(&self, iri: &str) -> Option<&str> {
        self.preds.iter().find(|p| p.iri == iri).and_then(|p| p.obj_lit.as_deref())
    }
    fn lits(&self, iri: &str) -> Vec<&str> {
        self.preds.iter().filter(|p| p.iri == iri).filter_map(|p| p.obj_lit.as_deref()).collect()
    }
    fn iri(&self, iri: &str) -> Option<&str> {
        self.preds.iter().find(|p| p.iri == iri).and_then(|p| p.obj_iri.as_deref())
    }
    fn is_a(&self, class_iri: &str) -> bool {
        self.preds.iter().any(|p| p.iri == RDF_TYPE && p.obj_iri.as_deref() == Some(class_iri))
    }
}

fn nodes_of(doc: &Doc) -> Vec<Node> {
    let mut out = Vec::new();
    for st in &doc.statements {
        let subject = &st[0];
        let label = doc.name_of(subject);
        walk(doc, label, subject.line, &st[1..], &mut out);
    }
    out
}

fn walk(doc: &Doc, label: String, line: u32, body: &[Token], out: &mut Vec<Node>) {
    let ps = pairs(body);
    let mut preds = Vec::new();
    for (p, o) in &ps {
        let (obj_iri, obj_lit) = match o {
            Obj::Single(t) => match &t.tok {
                Tok::Literal(l) => (None, Some(l.clone())),
                _ => (doc.expand(t), None),
            },
            _ => (None, None),
        };
        preds.push(Pred {
            iri: doc.expand(p).unwrap_or_else(|| doc.name_of(p)),
            obj_iri,
            obj_lit,
            line: p.line,
        });
    }
    out.push(Node { label: label.clone(), line, preds });

    for (p, o) in &ps {
        let pname = doc.name_of(p);
        match o {
            Obj::Block(inner) => {
                let child = child_label(doc, &pname, inner, &label, None);
                walk(doc, child, inner.first().map(|t| t.line).unwrap_or(line), inner, out);
            }
            Obj::Collection(inner) => {
                for (n, it) in items(inner).iter().enumerate() {
                    if let Obj::Block(b) = it {
                        let child = child_label(doc, &pname, b, &label, Some(n + 1));
                        walk(doc, child, b.first().map(|t| t.line).unwrap_or(line), b, out);
                    }
                }
            }
            Obj::Single(_) => {}
        }
    }
}

fn child_label(doc: &Doc, pred: &str, inner: &[Token], parent: &str, index: Option<usize>) -> String {
    let own = pairs(inner);
    let path = own.iter().find(|(p, _)| doc.expand(p).as_deref() == Some(&format!("{SH}path")[..]));
    if let Some((_, Obj::Single(t))) = path {
        return format!("the {} property shape of {parent}", doc.name_of(t));
    }
    let is_sparql = own.iter().any(|(p, o)| {
        doc.expand(p).as_deref() == Some(RDF_TYPE)
            && matches!(o, Obj::Single(t) if doc.expand(t).as_deref() == Some(&format!("{SH}SPARQLConstraint")[..]))
    });
    if is_sparql {
        return format!("the {pred} constraint of {parent}");
    }
    match index {
        Some(n) => format!("member {n} of the {pred} list of {parent}"),
        None => format!("an anonymous {pred} node of {parent}"),
    }
}

/// Which classes the shapes attach each property to: `sh:path` of every property
/// shape, against the `sh:targetClass` of the node shape declaring it. Walked
/// directly rather than through [`nodes_of`], because the answer is exactly the
/// parent/child link that a flat list of nodes throws away.
fn paths_by_target(doc: &Doc) -> (BTreeMap<String, BTreeSet<String>>, usize, usize) {
    let sh_target = format!("{SH}targetClass");
    let sh_property = format!("{SH}property");
    let sh_path = format!("{SH}path");
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut shapes = 0usize;
    let mut property_shapes = 0usize;

    for st in &doc.statements {
        let ps = pairs(&st[1..]);
        let target = ps.iter().find_map(|(p, o)| {
            if doc.expand(p).as_deref() != Some(&sh_target[..]) {
                return None;
            }
            match o {
                Obj::Single(t) => doc.expand(t),
                _ => None,
            }
        });
        let Some(target) = target else { continue };
        shapes += 1;
        for (p, o) in &ps {
            if doc.expand(p).as_deref() != Some(&sh_property[..]) {
                continue;
            }
            let Obj::Block(inner) = o else { continue };
            property_shapes += 1;
            for (ip, io) in pairs(inner) {
                if doc.expand(&ip).as_deref() != Some(&sh_path[..]) {
                    continue;
                }
                let Obj::Single(t) = io else { continue };
                let Some(path) = doc.expand(&t) else { continue };
                map.entry(path).or_default().insert(target.clone());
            }
        }
    }
    (map, shapes, property_shapes)
}

// ---------------------------------------------------------------- the tests

#[test]
fn both_files_scan_and_every_prefixed_name_has_a_declared_prefix() {
    for name in [NS_FILE, SHAPES_FILE] {
        let doc = load(name);

        assert!(
            doc.statements.len() >= 5,
            "{name} scanned to only {} statement(s). Either the file lost most of \
             its content, or the scanner stopped reading it part-way — and every \
             other assertion in this file is then checking a fragment and \
             passing for that reason.",
            doc.statements.len()
        );
        assert!(
            !doc.prefixes.is_empty(),
            "{name} declares no @prefix at all, so nothing below could resolve a \
             single term and every name-based check here would pass by finding \
             nothing."
        );

        let mut undeclared = Vec::new();
        let mut names = 0usize;
        for st in &doc.statements {
            for t in st {
                let Tok::Word(w) = &t.tok else { continue };
                let w = w.strip_prefix("^^").unwrap_or(w);
                if w == "a" || w.starts_with('@') {
                    continue;
                }
                let Some((p, _)) = w.split_once(':') else { continue };
                names += 1;
                if !doc.prefixes.contains_key(p) {
                    undeclared.push(format!("`{w}` on line {}", t.line));
                }
            }
        }
        assert!(
            names > 20,
            "only {names} prefixed names were found in {name}; the scanner is not \
             seeing the file, so the check below would accept anything."
        );
        assert!(
            undeclared.is_empty(),
            "{name} uses {} prefixed name(s) it never declares: {}. This is not a \
             warning anywhere downstream — an undeclared prefix is a parse error, \
             and a parse error in a vendored vocabulary takes the consumer's \
             entire validation run down rather than failing on its own line.",
            undeclared.len(),
            undeclared.join(", ")
        );

        let sparql_form: Vec<u32> = doc
            .statements
            .iter()
            .flatten()
            .filter(|t| matches!(&t.tok, Tok::Word(w) if w == "PREFIX" || w == "BASE"))
            .map(|t| t.line)
            .collect();
        assert!(
            sparql_form.is_empty(),
            "{name} uses the SPARQL-style dot-less PREFIX/BASE form on line(s) \
             {sparql_form:?}. It is legal Turtle, and this test deliberately does \
             not implement it: a directive the scanner cannot see is a prefix map \
             the rest of this file would silently check against the wrong \
             namespaces. Write the @-form, or teach the scanner both."
        );
    }
}

#[test]
fn no_property_the_shapes_attach_to_two_classes_declares_an_rdfs_domain() {
    let ns = load(NS_FILE);
    let shapes = load(SHAPES_FILE);

    let (paths, shape_count, property_count) = paths_by_target(&shapes);
    assert!(
        shape_count >= 4 && property_count >= 20,
        "the shapes walk found {shape_count} node shape(s) with an sh:targetClass \
         and {property_count} property shape(s), which is too few to be this \
         file: the walk has stopped seeing the structure, and the multi-class set \
         it produces would be empty for that reason rather than because no \
         property is shared."
    );

    let multi: BTreeMap<&String, &BTreeSet<String>> =
        paths.iter().filter(|(_, c)| c.len() > 1).collect();
    assert!(
        !multi.is_empty(),
        "no property was found attached to more than one class, so this test \
         would pass no matter what rdfs:domain ns.ttl declares. {} property paths \
         were seen across {shape_count} shapes.",
        paths.len()
    );
    for local in ["slug", "styleKey", "note"] {
        let iri = format!("{}{local}", honeycomb_core::NS);
        assert!(
            multi.contains_key(&iri),
            "hive:{local} is no longer attached to more than one class by the \
             shapes. Either a shape was dropped — in which case a class stopped \
             being checked — or the walk broke. Either way the rdfs:domain \
             decision that follows is now being taken on different evidence than \
             the one written down in ns.ttl, so re-read that comment before \
             editing this line. Multi-class properties found: {:?}",
            multi.keys().collect::<Vec<_>>()
        );
    }

    let domain = format!("{RDFS}domain");
    let mut declared: BTreeMap<String, u32> = BTreeMap::new();
    for st in &ns.statements {
        let subject = match ns.expand(&st[0]) {
            Some(s) => s,
            None => continue,
        };
        for (p, _) in pairs(&st[1..]) {
            if ns.expand(&p).as_deref() == Some(&domain[..]) {
                declared.insert(subject.clone(), p.line);
            }
        }
    }
    assert!(
        declared.contains_key(&format!("{}col", honeycomb_core::NS)),
        "hive:col declares no rdfs:domain in ns.ttl. It is the one single-class \
         property this test reads to prove it can SEE an rdfs:domain at all; \
         without it, the assertion below passes whether or not the forbidden \
         domains are there. {} domain declaration(s) were found in total.",
        declared.len()
    );

    let offenders: Vec<String> = multi
        .keys()
        .filter_map(|iri| declared.get(iri.as_str()).map(|line| format!("{} (ns.ttl line {line})", ns.short(iri))))
        .collect();
    assert!(
        offenders.is_empty(),
        "these declare an rdfs:domain while the shapes attach them to more than \
         one class: {}. rdfs:domain is a claim that everything carrying the property IS \
         an instance of that class, so a property carried by three classes has no \
         honest domain to declare. At least one real consumer reads rdfs:domain \
         entailment-free as a constraint on instance data: this is not a \
         modelling nicety there, it is a hard validation error on every CORRECT \
         document in that repository, reported one repository away from the line \
         that caused it. Delete the domain and state applicability in the \
         rdfs:comment, which is where a reader looks.",
        offenders.join(", ")
    );
}

#[test]
fn neither_file_carries_a_relative_iri_ref_or_a_base_to_resolve_one_against() {
    for name in [NS_FILE, SHAPES_FILE] {
        let doc = load(name);

        assert!(
            doc.iri_refs.len() >= 5,
            "only {} IRI ref(s) were found in {name}. A file with no `<...>` in it \
             would satisfy the check below by having nothing to check.",
            doc.iri_refs.len()
        );

        let relative: Vec<String> = doc
            .iri_refs
            .iter()
            .filter_map(|t| match &t.tok {
                Tok::Iri(s) if !has_scheme(s) => Some(format!("<{s}> on line {}", t.line)),
                _ => None,
            })
            .collect();
        assert!(
            relative.is_empty(),
            "{name} contains {} relative IRI ref(s): {}. A consumer vendors these \
             bytes and prepends one @base line to satisfy its own rules; that \
             prepend is reviewable precisely because it is INERT. A relative ref \
             makes it a rewrite instead — every term silently renamed into the \
             consumer's namespace, in a file the consumer is told not to edit.",
            relative.len(),
            relative.join(", ")
        );

        assert!(
            doc.bases.is_empty(),
            "{name} declares @base on line(s) {:?}. It must declare none: the \
             consumer's prepended @base is then the only one in its copy, and the \
             file it prepends to is unchanged. Two @base directives make which \
             one wins a question about ordering that nobody reviewing the diff \
             will think to ask.",
            doc.bases
        );
    }
}

#[test]
fn every_node_that_states_a_constraint_carries_an_sh_message() {
    let doc = load(SHAPES_FILE);
    let nodes = nodes_of(&doc);
    let message = format!("{SH}message");
    let sparql_constraint = format!("{SH}SPARQLConstraint");

    assert!(
        nodes.len() >= 30,
        "only {} node(s) were walked out of shapes.ttl. The file holds a node \
         shape per class plus a property shape per constrained predicate, so a \
         number this small means the walk is not descending into the blank-node \
         property lists — and every node it failed to reach is a node it cannot \
         report as silent.",
        nodes.len()
    );

    let mut stating = 0usize;
    let mut silent = Vec::new();
    for n in &nodes {
        let states = n.preds.iter().any(|p| {
            p.iri.strip_prefix(SH).map(|l| CONSTRAINT_PARAMS.contains(&l)).unwrap_or(false)
        }) || n.is_a(&sparql_constraint);
        if !states {
            continue;
        }
        stating += 1;
        if !n.has(&message) {
            // The line of the constraint parameter itself, not of the node: in a
            // file where a shape spans forty lines of prose, the node's first
            // line is not where the reader has to look.
            let at = n
                .preds
                .iter()
                .find(|p| p.iri.strip_prefix(SH).map(|l| CONSTRAINT_PARAMS.contains(&l)).unwrap_or(false))
                .map(|p| p.line)
                .unwrap_or(n.line);
            silent.push(format!("{} (line {at})", n.label));
        }
    }

    assert!(
        stating >= 20,
        "only {stating} node(s) in shapes.ttl were seen to state a constraint. \
         Every property shape in the file carries at least an sh:minCount, an \
         sh:maxCount or an sh:datatype, so a count this low means the constraint \
         parameters are not being recognised and nothing is being required to \
         carry a message."
    );
    assert!(
        silent.is_empty(),
        "{} node(s) in shapes.ttl state a constraint and carry no sh:message: \
         {}. What a contributor is shown when one of these fires is \
         \"Constraint Violation in MinCountConstraintComponent\" and an IRI, \
         which says neither what rule was broken nor what to type. And a \
         consumer gate that requires a message on every constraint-stating node \
         reports each of these as a violation inside bytes it has vendored and \
         must not edit — so the fix has to be made here and released before that \
         repository's pipeline can go green again.",
        silent.len(),
        silent.join(", ")
    );

    // The four messages that can never be shown. They are the exact defect a
    // SHACL gate reproduced against this design — four violations, all of them
    // inside the vendored copy — so their presence is pinned here by name
    // rather than left to the general rule above, which would go quiet if the
    // walk ever stopped descending into the sh:xone list.
    let nested: Vec<&Node> = nodes
        .iter()
        .filter(|n| n.label.contains("sh:xone list") && n.has(&format!("{SH}path")))
        .collect();
    assert_eq!(
        nested.len(),
        4,
        "expected the 4 property shapes nested inside the sh:xone list of \
         hsh:PlacementDrawsOneThing and found {}. They are unreachable at \
         validation time — pyshacl reports one result at the enclosing sh:xone \
         and these are never evaluated — but a consumer gate walks the shapes \
         graph for nodes carrying a core constraint parameter and exempts only \
         the xone MEMBERS, which these are one hop further out from. If they have \
         gone, four violations have arrived in the vendored copy. Nodes seen \
         inside that list: {:?}",
        nested.len(),
        nodes.iter().filter(|n| n.label.contains("sh:xone list")).map(|n| &n.label).collect::<Vec<_>>()
    );
    for n in nested {
        assert!(
            n.has(&message),
            "{} carries no sh:message. It is a message nobody will ever read, and \
             it is required anyway: omitting it is a violation in the consumer's \
             gate, in the one place the consumer cannot fix it.",
            n.label
        );
    }

    let messages: Vec<&str> = nodes.iter().flat_map(|n| n.lits(&message)).collect();
    assert!(
        messages.len() >= 20,
        "only {} sh:message value(s) were read out of shapes.ttl, which is fewer \
         than the file has shapes: the literals are not being scanned, and the \
         shape check below is looking at almost nothing.",
        messages.len()
    );
    let malformed: Vec<&&str> = messages
        .iter()
        .filter(|m| !(m.starts_with("rule:") && m.contains("why:") && m.contains("fix:")))
        .collect();
    assert!(
        malformed.is_empty(),
        "{} sh:message value(s) are not rule:/why:/fix:. First one: {:?}. The \
         three parts are what let a contributor who has never read SHACL act on \
         the message alone — rule: names the invariant, why: names the failure it \
         prevents, fix: says what to type. A message that only names the rule \
         sends them to read the shapes file, which is the outcome the convention \
         exists to avoid.",
        malformed.len(),
        malformed.first().map(|m| m.chars().take(120).collect::<String>())
    );
}

#[test]
fn every_prefix_the_sparql_constraints_use_is_declared_on_the_shapes_ontology() {
    let doc = load(SHAPES_FILE);
    let nodes = nodes_of(&doc);
    let sh_prefix = format!("{SH}prefix");
    let sh_namespace = format!("{SH}namespace");
    let sh_select = format!("{SH}select");
    let sh_prefixes = format!("{SH}prefixes");
    let sparql_constraint = format!("{SH}SPARQLConstraint");

    let mut declared: BTreeMap<String, String> = BTreeMap::new();
    for n in &nodes {
        if let (Some(p), Some(ns)) = (n.lit(&sh_prefix), n.lit(&sh_namespace)) {
            declared.insert(p.to_string(), ns.to_string());
        }
    }
    assert!(
        !declared.is_empty(),
        "shapes.ttl declares no sh:declare prefix mapping at all. Every SPARQL \
         constraint in the file writes hive: names; with no declaration they do \
         not resolve, and the failure is not a violation report — it is a query \
         that fails to parse inside the validator, in a message that names no \
         shape."
    );
    assert_eq!(
        declared.get("hive").map(|s| s.as_str()),
        Some(honeycomb_core::NS),
        "the `hive` prefix declared for SPARQL does not match the namespace this \
         crate writes its documents in. The constraints would then parse, run, \
         and match nothing at all — every rule in this file silently enforcing \
         nothing, with a clean validation report to show for it."
    );

    let selects: Vec<(&Node, &str)> = nodes
        .iter()
        .filter_map(|n| n.lit(&sh_select).map(|s| (n, s)))
        .collect();
    assert!(
        selects.len() >= 5,
        "only {} sh:select body/bodies were found; the SPARQL rules are what earn \
         this vocabulary, and a scan that cannot see them checks nothing here.",
        selects.len()
    );

    let mut missing = Vec::new();
    let mut used = 0usize;
    for (node, select) in &selects {
        for p in prefixes_used_in(select) {
            used += 1;
            if !declared.contains_key(&p) {
                missing.push(format!("`{p}:` in {}", node.label));
            }
        }
        let sh_declare = format!("{SH}declare");
        let points_at_declarations = node
            .iri(&sh_prefixes)
            .map(|target| {
                let named = doc.short(target);
                nodes.iter().any(|n| n.label == named && n.has(&sh_declare))
            })
            .unwrap_or(false);
        assert!(
            points_at_declarations,
            "{} does not point sh:prefixes at a subject in this file that carries \
             sh:declare. The prefix map is resolved from that subject alone: \
             pointing it anywhere else leaves the query's names unresolved, and \
             the validator fails at query-parse time rather than reporting a \
             violation anybody can read.",
            node.label
        );
        assert!(
            node.is_a(&sparql_constraint),
            "{} carries an sh:select but is not typed sh:SPARQLConstraint, so the \
             validator has no reason to run it. A rule that is never run reports \
             nothing and looks exactly like a rule that always passes.",
            node.label
        );
    }
    assert!(
        used > 0,
        "no prefixed name was found inside any sh:select body, so the check below \
         has nothing to check. The SPARQL in this file is written entirely in \
         hive: names; finding none means the literals are not being read."
    );
    assert!(
        missing.is_empty(),
        "{} prefixed name(s) used in SPARQL resolve against no sh:declare: {}. \
         This does not fail loudly — it fails at query-parse time inside the \
         validator, with a message that names no shape, and the rule that was \
         supposed to run simply does not.",
        missing.len(),
        missing.join(", ")
    );
}

/// Prefixes used as `pfx:name` inside a SPARQL body. Deliberately crude, and
/// crude in the safe direction: it can only ever ask for a declaration that is
/// already there, never excuse a missing one.
fn prefixes_used_in(select: &str) -> BTreeSet<String> {
    let chars: Vec<char> = select.chars().collect();
    let mut out = BTreeSet::new();
    for (i, c) in chars.iter().enumerate() {
        if *c != ':' {
            continue;
        }
        // `<http://...>` and `?x:y` are not prefixed names.
        let after = chars.get(i + 1).copied().unwrap_or(' ');
        if !(after.is_ascii_alphanumeric() || after == '_') {
            continue;
        }
        let mut j = i;
        while j > 0 && (chars[j - 1].is_ascii_alphanumeric() || chars[j - 1] == '_') {
            j -= 1;
        }
        if j == i {
            continue;
        }
        let before = if j == 0 { ' ' } else { chars[j - 1] };
        if matches!(before, '?' | '$' | '<' | '/' | ':') {
            continue;
        }
        out.insert(chars[j..i].iter().collect());
    }
    out
}

#[test]
fn the_version_the_tbox_states_is_the_version_of_the_crate_that_ships_it() {
    assert_eq!(
        honeycomb_core::TBOX_VERSION,
        env!("CARGO_PKG_VERSION"),
        "TBOX_VERSION is not this package's version. It exists to be compared \
         against the file, so a value typed by hand rather than taken from \
         CARGO_PKG_VERSION makes the comparison agree with nothing."
    );

    let ns = load(NS_FILE);
    let ontology = honeycomb_core::NS.trim_end_matches('#').to_string();
    let nodes = nodes_of(&ns);
    let node = nodes
        .iter()
        .find(|n| n.label == ns.short(&ontology) || n.label == format!("<{ontology}>"))
        .unwrap_or_else(|| {
            panic!(
                "ns.ttl declares no subject <{ontology}>. The ontology subject is \
                 where the version, the preferred prefix and the title live; \
                 without it a consumer has a file of terms and nothing that says \
                 what release the terms came from. Subjects found: {:?}",
                nodes.iter().map(|n| &n.label).collect::<Vec<_>>()
            )
        });

    let stated = node.lit(&format!("{}tboxVersion", honeycomb_core::NS)).unwrap_or_else(|| {
        panic!(
            "<{ontology}> states no hive:tboxVersion. A consumer that vendors \
             these bytes reads this value out of the copy to say which copy it \
             has; with nothing there, the copy is unidentifiable and the only way \
             to tell two vendored vocabularies apart is to diff them."
        )
    });
    assert_eq!(
        stated,
        honeycomb_core::TBOX_VERSION,
        "ns.ttl states hive:tboxVersion {stated:?} while the crate shipping it is \
         {}. \"A term cannot change without a version bump\" is only true if \
         something fails when it does not, and this is that something: a consumer \
         pins the version it expects in a shape of its own, so a vocabulary that \
         changed under an unchanged version number passes that pin and takes the \
         change with it.",
        honeycomb_core::TBOX_VERSION
    );

    let version_info = format!("{OWL}versionInfo");
    assert_eq!(
        node.lit(&version_info),
        Some(honeycomb_core::TBOX_VERSION),
        "the owl:versionInfo of <{ontology}> disagrees with its hive:tboxVersion. \
         Two version numbers on one subject is one of them being wrong, and a \
         reader has no way to tell which."
    );

    let shapes = load(SHAPES_FILE);
    let shapes_ontology = honeycomb_core::SHAPES_NS.trim_end_matches('#').to_string();
    let shape_nodes = nodes_of(&shapes);
    let shapes_node = shape_nodes
        .iter()
        .find(|n| n.label == shapes.short(&shapes_ontology) || n.label == format!("<{shapes_ontology}>"))
        .unwrap_or_else(|| panic!("shapes.ttl declares no subject <{shapes_ontology}> to carry its version or its sh:declare prefix map."));
    assert_eq!(
        shapes_node.lit(&version_info),
        Some(honeycomb_core::TBOX_VERSION),
        "shapes.ttl states a different owl:versionInfo than ns.ttl. The two are \
         vendored as a pair and pinned by one version; a consumer that has \
         version 0.1.0 of one and 0.1.0 of the other must be able to trust that \
         they were released together."
    );
}

#[test]
fn both_files_are_written_in_the_namespaces_this_crate_exports() {
    let ns = load(NS_FILE);
    let shapes = load(SHAPES_FILE);

    assert_eq!(
        ns.prefixes.get("hive").map(|s| s.as_str()),
        Some(honeycomb_core::NS),
        "ns.ttl binds hive: to something other than the namespace this crate \
         writes its documents in. A term whose IRI moved is a NEW term: every \
         document already written keeps naming the old one, no shape matches \
         either, and nothing anywhere records that the two were meant to be the \
         same thing."
    );
    assert_eq!(
        shapes.prefixes.get("hive").map(|s| s.as_str()),
        Some(honeycomb_core::NS),
        "shapes.ttl constrains terms in a different namespace than ns.ttl \
         defines. Every target would then select zero focus nodes — a full set of \
         rules, a clean report, and nothing checked."
    );
    assert_eq!(
        shapes.prefixes.get("hsh").map(|s| s.as_str()),
        Some(honeycomb_core::SHAPES_NS),
        "shapes.ttl binds hsh: to something other than SHAPES_NS. The shape IRIs \
         are what a validation report names and what a consumer's own shapes file \
         refers to when it exempts or extends one; renaming them orphans those \
         references without breaking anything visibly."
    );

    let ontology = honeycomb_core::NS.trim_end_matches('#').to_string();
    let nodes = nodes_of(&ns);
    let node = nodes
        .iter()
        .find(|n| n.label == ns.short(&ontology) || n.label == format!("<{ontology}>"))
        .unwrap_or_else(|| panic!("ns.ttl declares no subject <{ontology}>."));
    assert_eq!(
        node.lit(&format!("{VANN}preferredNamespaceUri")),
        Some(honeycomb_core::NS),
        "the vann:preferredNamespaceUri of <{ontology}> is not the namespace the \
         terms are actually in. That value is what a consuming tool copies into \
         its own prefix map, so it is the one field whose being wrong produces \
         confidently-written documents naming terms that do not exist."
    );
    assert_eq!(
        node.lit(&format!("{VANN}preferredNamespacePrefix")),
        Some("hive"),
        "the vann:preferredNamespacePrefix of <{ontology}> is not \"hive\". Every \
         consumer's file, every message in shapes.ttl and every comment in this \
         repository is written in hive:; a second spelling in the wild makes two \
         files that mean the same thing look different in review."
    );

    for (name, doc) in [(NS_FILE, &ns), (SHAPES_FILE, &shapes)] {
        assert!(
            !doc.text.contains("github.io"),
            "{name} still mentions github.io. The namespace was moved to \
             semantic.ds-labs.org deliberately, and the host it moved off is the \
             one place a leftover IRI will keep resolving — so a stale reference \
             does not 404, it serves an older vocabulary to whoever follows it."
        );
    }
}

/// An absolute IRI has a scheme. Nothing here dereferences one — the namespace
/// is deliberately not serving yet, and an IRI is an identifier first.
fn has_scheme(iri: &str) -> bool {
    let Some(i) = iri.find(':') else { return false };
    let (scheme, _) = iri.split_at(i);
    !scheme.is_empty()
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}
