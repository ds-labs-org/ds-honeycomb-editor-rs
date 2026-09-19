# ds-honeycomb-editor-rs

A [Yew](https://yew.rs) component for editing a **hexagonal honeycomb lattice
diagram**. The thing under the pointer is the thing that moves: **drag a tile
and it moves alone; drag a group's ground or heading and the whole group moves
with it.** Two tiles of one group trade places when you drop one on the other;
every other overlap is refused with its evidence. The whole diagram serialises
to RDF/Turtle in a vocabulary this repository owns.

**Demo: <https://ds-labs-org.github.io/ds-honeycomb-editor-rs/>**

## Layout

| path | what it is |
|---|---|
| `ns.ttl`, `ns` | the vocabulary. Two byte-identical copies, so the namespace IRI can dereference to the terms once the host is standing |
| `shapes.ttl` | the SHACL contract for it |
| `honeycomb-core/` | the lattice, the model and the serialisation. **Zero dependencies** |
| `honeycomb-yew/` | the component: pointer drag, keyboard equivalents, group grips, same-group swap, refusal |
| `demo/` | the Trunk app: `DemoApp`, the dummy fixture, the wasm entry point |
| `demo-ssg/` | a host-only binary that renders the page to static HTML at build time |

`honeycomb-core` has no dependencies at all — `cargo tree -p honeycomb-core` is
one line, and it is meant to stay that way. That is what lets a host-side
generator lay a diagram out and write its Turtle without linking a UI framework,
and it is why both the writer and the parser are hand-rolled rather than built
on an RDF crate. (The obvious one reaches wasm32 through `getrandom` and does
not compile there at all, so a parser built on it could not read back what the
browser half writes.)

## The vocabulary

`hive:` is <https://semantic.ds-labs.org/vocab/honeycomb#> and `hsh:` is
<https://semantic.ds-labs.org/shapes/honeycomb#>.

**Two modes, one vocabulary, and `hive:mode` says which.**

- **`hive:pinned` — layout only.** Every `hive:Placement` names an external
  subject with `hive:represents` and carries nothing else, so regenerating that
  source changes what the diagram draws without touching the file.
  `hive:Placement` is the one `sh:closed` shape in the contract and declares no
  content property at all, so a writer *cannot* cache a label into it. Drift is
  not a rule a contributor can forget; it is a file that will not validate.
  `PinnedTile` in `honeycomb-core` says the same thing a step earlier — it has
  no content field, so no line of code here could write one.
- **`hive:standalone` — self-contained.** Placements name a `hive:Tile` in the
  same document. Nothing regenerates it and nothing can make it stale: that is
  the trade it makes.

A document that mixes the two is rejected — by `hsh:ModeIsHonest` in a graph
consumer's gate, and by `read_turtle` before a file is ever loaded.

What the vocabulary refuses to know: colours, themes, layers, sizes, deployment
targets, and what a tile *is*. Everything a host means by an appearance is one
opaque `hive:styleKey` IRI that this vocabulary never dereferences, and
everything a tile is lives behind `hive:represents` in the host's own namespace.

### The namespace does not dereference yet

`semantic.ds-labs.org` is not serving at the time of writing, and that is
recorded rather than an oversight. An IRI is an identifier first; **`ns.ttl` in
this repository is the authoritative bytes**, and standing the host up is a
separate job that changes nothing here when it happens. Minting the namespace
from whatever host happened to be convenient is how a vocabulary ends up named
after a hosting decision it later regrets.

### Consumers

`ns.ttl` and `shapes.ttl` are the one artefact of this repository that leaves it
**verbatim**. A consumer vendors both files into its own tree and runs them,
unmodified, over its own corpus in its own validation gate. Two properties are
load-bearing for that, and `honeycomb-core/tests/vocabulary.rs` asserts both
before the bytes can leave:

- **Neither file declares `@base`, and neither contains a relative IRI ref.** A
  consumer that must vendor a copy prepends its own `@base` to satisfy its own
  rules. That prepend stays a pure, reviewable, mechanical transform only while
  there is nothing here for it to silently rewrite.
- **No property the shapes attach to more than one class declares an
  `rdfs:domain`.** `rdfs:domain` is a claim that anything carrying the property
  *is* an instance of that class, so `hive:slug` — carried by `hive:Diagram`,
  `hive:Group` **and** `hive:Tile` — has no honest domain to declare. At least
  one real consumer reads `rdfs:domain` entailment-free as a constraint on
  instance data, where a domain here would be a hard validation error on every
  *correct* document, reported a repository away from the line that caused it.
  Applicability is stated in each term's `rdfs:comment`, which is where a reader
  looks. Adding a domain to `hive:slug`, `hive:styleKey` or `hive:note` is a
  breaking change.

`hive:tboxVersion` says what a copy *is* and cannot say whether it is current: a
consumer reads it out of its own vendored copy, so it reports what the copy says
about itself. The way to make it detect staleness is for the consumer to pin the
value it expects in a shape of its own, so that bumping the copy without bumping
the pin fails that consumer's build.

## The demo page is statically generated

The published page is **not** rendered per request — GitHub Pages serves files
and there is no server. `demo-ssg` runs `yew::ServerRenderer` on the host during
`trunk build` and writes the finished diagram into the HTML. A visitor receives
the complete lattice — every hex, every label, the Turtle, the contents table —
as real markup, before any WebAssembly is fetched.

Once the wasm arrives it **replaces** that markup rather than hydrating it. The
reasoning, and the measurements behind it, are in the header comment of
[`demo-ssg/src/main.rs`](demo-ssg/src/main.rs).

The fixture is a pure function of nothing — no clock, no randomness — and the
demo diagram carries no `hive:generatedAt` for that reason. The build-time
render and the browser's first render have to produce the same bytes, and a
clock is the one thing that guarantees they cannot.

With JavaScript switched off the page stays exactly what it already was: a
correct, readable, laid-out diagram that does not respond to dragging, with a
`<noscript>` line saying so and a `Download .ttl` link that still works because
it is a plain link to a real file.

## The gesture

| you press | what moves |
|---|---|
| a hexagon | that hexagon, alone — its group stays put and is reported as split |
| a group's ground or heading | every tile in that group, rigidly |
| a hexagon, onto another of the same group | the two trade places |
| a hexagon, onto anything else | nothing: refused, naming the blocker |

There is **no modifier key**. There used to be — `Alt` narrowed a group drag to
one tile — and it was wrong twice over: GNOME's window manager claims `Alt`+drag
before the page ever sees it, and a rule you can only find by holding a key is a
rule nobody finds.

Every gesture has a keyboard equivalent, because a drag-only editor fails WCAG
2.1 SC 2.1.1 outright. Arrow keys rove the selection, `Space` grabs and drops,
arrows move what is held, `Escape` cancels. Each group's region is a tab stop
with its own accessible name, so `Tab` to it and `Space` is the group drag.

## What this is not

- **Not a graph editor.** It arranges hexagons on a lattice. There are no edges,
  no ports and no routing.
- **Not a renderer of anything in particular.** The host draws the inside of
  every hexagon and the component never learns what one means.
- **Not a SHACL engine.** `shapes.ttl` is data. `cargo test` checks the
  properties a consumer's gate depends on; running the shapes over a corpus is
  that consumer's job.
- **Not a clipboard.** The demo's export is a plain `<a download>` to a file the
  generator wrote — which is why it still works with JavaScript off — whose
  `href` the wasm rewrites to the current arrangement once it is running.

## Building

```sh
cargo test --workspace
cargo tree -p honeycomb-core --edges normal   # must stay one line
cd demo && trunk serve                        # http://localhost:8080
cd demo && trunk build --release --public-url /ds-honeycomb-editor-rs/
```

`trunk serve` runs the generator on every rebuild, so local and published builds
produce the same artifact.

## Licence

Apache-2.0.
