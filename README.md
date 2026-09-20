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

### Language tags

`rdfs:label "Mairie"@fr` and `rdfs:comment "…"@fr` keep their tag through a save.
The model carries a label as `Text` — the words plus an optional language — the
reader parses the tag, the writer emits it again, and a second export is byte
for byte the first one.

**Only where the shapes allow one.** `rdfs:label` is constrained with
`sh:minLength` and deliberately no `sh:datatype`, and `rdfs:comment` carries no
property shape at all, so a tagged literal conforms. Every other string property
here — `hive:slug`, `hive:note`, `hive:generator`, `hive:pinnedRevision`,
`hive:formatVersion`, `hive:generatedAt` — pins a datatype, and a language-tagged
literal is `rdf:langString`, which is none of them. A tag on one of those is
**refused by name**, naming the predicate, rather than dropped: reading a tag and
writing the value back without it produces a file the author did not write.

`hive:note` is the one that will be asked for. It is prose printed under a
heading and a reasonable thing to want in French; it cannot carry a tag today
because both `shapes.ttl` and `ns.ttl` say `xsd:string`, and changing that is a
vocabulary release rather than a reader's decision.

Nothing this editor writes is ever tagged unless the document it read was, so no
file already in the wild changes on its next save.

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
| a palette item, onto an empty cell | that tile is added there, joining the group it names |
| a hexagon, dragged clear of the board | that tile is removed, content and all |
| a hexagon, dragged to another **while Link is on** | a line is drawn between them |

There is **no modifier key**. There used to be — `Alt` narrowed a group drag to
one tile — and it was wrong twice over: GNOME's window manager claims `Alt`+drag
before the page ever sees it, and a rule you can only find by holding a key is a
rule nobody finds.

Every gesture has a keyboard equivalent, because a drag-only editor fails WCAG
2.1 SC 2.1.1 outright. Arrow keys rove the selection, `Space` grabs and drops,
arrows move what is held, `Escape` cancels. Each group's region is a tab stop
with its own accessible name, so `Tab` to it and `Space` is the group drag. With
a palette item armed, the arrows choose a cell and `Space` places it; `Delete`
or `Backspace` on a selected tile takes it off.

## Links

A `hive:Link` joins two ends of one diagram, directed, with one of three
routings — `hive:straight`, `hive:latticePath`, `hive:arc`. Routing is a term
rather than an opaque style key because where a line goes is a question about the
lattice, and two hosts drawing one document must answer it identically or they
are drawing different diagrams; colour, width and arrowheads remain the host's.

An end is an `Endpoint`: a placement, or a **whole group**. A line meets a group
at whichever of its member cells is nearest the other end, recomputed as either
end moves and stored nowhere — the same reason a group's region is derived from
its members rather than written beside them. Three refusals keep the widening
honest, and they add up to one sentence: *the board never reaches a state where
a link exists that cannot be drawn.*

* an **empty** group may not be an endpoint — it has no cell, so the line draws
  nothing, which is indistinguishable from a link that was never there;
* a group may not link to **one of its own members** — containment says it
  already, and it is the likeliest mis-drag: press on a ground, release on one
  of its own hexagons;
* the **last member** of a linked group cannot leave it, by removal, detach or
  attach elsewhere — the refusal names the links in the way, which is
  actionable because links can be removed.

The README used to say this crate would never have any of these, and about the
LATTICE it still holds: a link moves nothing, reserves no cell, and a diagram
without them is exactly the diagram it was. What it adds is the one thing a
honeycomb cannot say by arrangement, because adjacency on a packed grid is a
consequence of packing rather than of meaning.

Linking is a MODE — `linking: bool` and `on_link: Callback<(Endpoint, Endpoint)>`
— because this editor has no modifier keys and a verb this different from moving
something deserves a visible toggle rather than an invisible one. The component
reports the two ends; the host decides what the link is.

The GESTURE draws all four combinations. A press on a hexagon starts a line at
that placement and a press on a ground or a heading starts one at the whole
region, by pointer or by Space on the focused element; a release resolves the
same way, and an empty cell one step outside a membership — where the ground is
painted and the heading sits — names the region rather than nothing. While the
mode is on, a group therefore cannot be dragged, which is the trade a hexagon
has always made.

Where a line MEETS a region is never stored: it is whichever member faces the
other end, recomputed on every render against the arrangement on screen — so a
line follows the ghosts through a drag and swaps member when the drag carries
the region past the other end. All three routings work from it, and the lattice
one needs no special case: the nearest member is on the near side, so no
shortest path out of a region can cross it.

Removing a tile that still has links is REFUSED, naming them.

## Editing the groups

`DeclareGroup`, `EditGroup` and `RemoveGroup`, all undoable. `EditGroup` carries
the WHOLE group rather than one field, so its inverse is the whole previous
value and a form that wrote a label cannot silently drop the note beside it.
`RemoveGroup` is refused while anything is still in the group, naming the
members — the tiles would otherwise be left pointing at a group the diagram does
not declare, which is exactly what `hsh:GroupBelongsToItsDiagram` catches in a
file.

Membership moves with `Attach` and `Detach`, which have been in the core since
the beginning with no gesture able to reach them. That is deliberate: a drag
never changes which group a tile is in, so a form is the way to change it.

**A group may have no label.** The ground is often the whole signal, and a
heading beside an obvious cluster says the same thing twice. The writer OMITS
`rdfs:label` rather than writing `""` — an empty string claims the name IS empty
rather than that there is none, and a reader cannot tell those apart. A TILE
still may not: one with no label draws as an empty hexagon.

## The palette

Adding and removing are opt-in and host-driven. The component owns no palette:
it takes a `pending: Option<Pending>` — the tile the host has armed — reports
through `on_pending` when that tile is placed or cancelled, and takes tiles off
the board only when `removable` is true, which it is not by default. The host
draws its own chips and decides what one means.

Two contracts no type can enforce, and both are silent failures:

- **Do not call `setPointerCapture` on a palette chip.** Captured, the board
  receives no `pointermove` at all and the drag is dead with no error.
- **`touch-action` is a choice, not a default.** A chip meant to be dragged
  needs `touch-action: none`; a chip in a scrolling drawer needs
  `manipulation`, or a finger cannot scroll past it. The demo takes the second,
  so on touch a chip is tap-to-arm and the board is tap-to-place — the same path
  the keyboard uses.

Adding, removing, moving and swapping are all undoable, and every recorded
inverse is a VALUE rather than a rule for deriving one — a `Restore` names the
tiles and the cells it puts back, so it moves exactly what its command moved
whatever has happened to the diagram in between.

`Command::Add` refuses a group the diagram has not declared, exactly as
`Command::Attach` does. A host that wants a group to be joinable before anything
is in it declares it empty: nothing in the vocabulary or the shapes requires a
group to have members, and `a_declared_group_with_no_members_survives_write_read_write`
pins that through the writer and the reader.

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
