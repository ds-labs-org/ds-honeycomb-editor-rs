//! The demo page, as ONE component rendered twice: once on the host by
//! `demo-ssg` at build time, once in the browser by `src/main.rs` at boot.
//!
//! Both go through `DemoApp` and `data::town_plan()` from this same lib, which
//! is the only reason the two renders agree.

pub mod data;

use honeycomb_yew::{HoneycombEditor, Lattice, Refusal};
use yew::prelude::*;

pub const REPO: &str = "https://github.com/ds-labs-org/ds-honeycomb-editor-rs";

#[derive(Properties, PartialEq, Default)]
pub struct AppProps {}

#[function_component(DemoApp)]
pub fn demo_app(_props: &AppProps) -> Html {
    let lattice = use_state(data::town_plan);
    let status = use_state(|| Status::Rest);

    let on_change = {
        let lattice = lattice.clone();
        let status = status.clone();
        Callback::from(move |next: Lattice| {
            status.set(Status::Moved);
            lattice.set(next);
        })
    };
    let on_refused = {
        let status = status.clone();
        Callback::from(move |r: Refusal| status.set(Status::Refused(r.message())))
    };
    let on_reset = {
        let lattice = lattice.clone();
        let status = status.clone();
        Callback::from(move |_: MouseEvent| {
            status.set(Status::Rest);
            lattice.set(data::town_plan());
        })
    };

    let ttl = lattice.to_turtle();
    let triples = ttl.lines().filter(|l| l.trim_end().ends_with([';', '.'])).count();

    html! {
        <>
        <header class="hc-head">
            <div>
                <h1>{ "Honeycomb Editor" }<span class="hc-head__tag">{ "demo" }</span></h1>
                <p class="hc-head__sub">
                    { "A hex-lattice diagram you can rearrange. Every name on this page is invented." }
                </p>
            </div>
            <a class="hc-head__repo" href={REPO}>{ "Source ↗" }</a>
        </header>

        // Rendered by the generator, so it is in the delivered HTML and a
        // visitor can read what the page is for before any wasm exists.
        <ol class="hc-try">
            <li><b>{ "Drag Museum" }</b>{ " to an empty cell." }</li>
            <li><b>{ "Drag Library" }</b>{ " — the whole Civic Quarter follows." }</li>
            <li><b>{ "Drop Museum onto Cinema" }</b>{ " — it refuses, and says why." }</li>
            <li><b>{ "Watch the Turtle" }</b>{ " change as you go." }</li>
        </ol>

        <noscript>
            <p class="hc-noscript">
                { "JavaScript is off, so this diagram is a picture. The tiles won't move. \
                   The Turtle below is the real serialisation of exactly what you see." }
            </p>
        </noscript>

        <main class="hc-main">
            <section class="hc-board-pane">
                <HoneycombEditor lattice={(*lattice).clone()} {on_change} {on_refused} />
                <p class={classes!("hc-status", status.css())} role="status">{ status.text() }</p>
            </section>

            <aside class="hc-ttl-pane">
                <div class="hc-ttl-head">
                    <h2>{ "honeycomb-demo.ttl" }</h2>
                    <span class="hc-ttl-count">{ format!("{triples} triples") }</span>
                </div>
                <div class="hc-ttl-actions">
                    // A real link to a real file the generator writes, so it
                    // works with no JavaScript at all.
                    <a class="hc-btn" href="honeycomb-demo.ttl" download=true>{ "Download .ttl" }</a>
                    <button class="hc-btn" onclick={on_reset}>{ "Reset" }</button>
                </div>
                <pre class="hc-ttl"><code>{ ttl }</code></pre>
            </aside>
        </main>

        <details class="hc-listing">
            <summary>{ "Diagram contents as a list" }</summary>
            <table>
                <thead><tr><th>{ "Tile" }</th><th>{ "Group" }</th><th>{ "Cell" }</th></tr></thead>
                <tbody>
                { for lattice.tiles.iter().map(|t| html! {
                    <tr>
                        <td>{ &t.label }</td>
                        <td>{ t.group.as_deref().and_then(|g| lattice.group(g))
                                .map(|g| g.label.clone()).unwrap_or_else(|| "—".into()) }</td>
                        <td>{ honeycomb_yew::cell_ref(t.q, t.r) }</td>
                    </tr>
                }) }
                </tbody>
            </table>
        </details>

        <footer class="hc-foot">
            <p>
                { "Vocabulary: " }<code>{ honeycomb_yew::NS }</code>
                { " · Apache-2.0 · " }
                <a href={REPO}>{ "ds-labs-org/ds-honeycomb-editor-rs" }</a>
            </p>
        </footer>
        </>
    }
}

#[derive(Clone, PartialEq)]
enum Status {
    Rest,
    Moved,
    Refused(String),
}

impl Status {
    fn text(&self) -> String {
        match self {
            // What the generated HTML says, and therefore what a visitor with
            // no JavaScript reads. It must be true in that state.
            Status::Rest => "Drag a tile to move it. A district moves as one.".into(),
            Status::Moved => "Moved. The Turtle on the right is up to date.".into(),
            Status::Refused(m) => m.clone(),
        }
    }
    fn css(&self) -> &'static str {
        match self {
            Status::Refused(_) => "hc-status--refused",
            _ => "hc-status--rest",
        }
    }
}
