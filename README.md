# ds-honeycomb-yew-rs

A [Yew](https://yew.rs) component for editing a **hexagonal honeycomb lattice
diagram**: tiles drag from one hex cell to another, groups stick and follow
their members, overlap is refused, and the whole diagram serialises to
RDF/Turtle whose vocabulary the component defines.

**Demo: <https://ds-labs-org.github.io/ds-honeycomb-yew-rs/>**

Dependency-free beyond the floor: `yew`, `web-sys`, and `wasm-bindgen`
transitively. No CSS framework, no npm packages, no Trunk `[[node_packages]]`.
The styling is plain CSS this repo owns.

## Layout

| path | what it is |
|---|---|
| `honeycomb-yew/` | the component, and the RDF vocabulary it defines |
| `demo/` | the Trunk app: `DemoApp`, the dummy fixture, the wasm entry point |
| `demo-ssg/` | a host-only binary that renders the page to static HTML at build time |

## The demo page is statically generated

The published page is **not** rendered per request — GitHub Pages serves files
and there is no server. `demo-ssg` runs `yew::ServerRenderer` on the host during
`trunk build` and writes the finished diagram into the HTML, the same way the
EONA-X developer portal generates its pages. A visitor receives the complete
lattice — every hex, every label, the Turtle, the contents table — as real
markup, before any WebAssembly is fetched.

Once the wasm arrives it **replaces** that markup rather than hydrating it. The
reasoning, and the measurements behind it, are in the header comment of
[`demo-ssg/src/main.rs`](demo-ssg/src/main.rs).

With JavaScript switched off the page stays exactly what it already was: a
correct, readable, laid-out diagram that does not respond to dragging, with a
`<noscript>` line saying so and a `Download .ttl` link that still works because
it is a plain link to a real file.

## Building

```sh
cargo test --workspace
cd demo && trunk serve                 # http://localhost:8080
cd demo && trunk build --release --public-url /ds-honeycomb-yew-rs/
```

`trunk serve` runs the generator on every rebuild, so local and published builds
produce the same artifact.

## Licence

Apache-2.0.
