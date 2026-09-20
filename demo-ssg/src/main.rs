//! STATIC GENERATION. Runs on the host at build time, inside Trunk's pipeline.
//!
//! This is the same shape the EONA-X portal uses today (developer.eona-x.eu,
//! crates/site/src/main.rs: `ServerRenderer::<COMP>::new().render()` into a
//! shell, written to dist/*.html). The one difference is that this page ships
//! wasm afterwards, which raises the only genuinely open question: once the
//! generated HTML is on screen, does the wasm HYDRATE it or REPLACE it?
//!
//! ============================================================================
//! VERDICT: REPLACE. `.hydratable(false)` here, `Renderer::render()` there.
//! ============================================================================
//!
//! Both work. I built and measured both against yew 0.23.0 under trunk
//! 0.22.0-beta.2 in headless Chromium, with JS expandos set on the generated
//! nodes before the wasm ran, and an rAF sampler counting what each painted
//! frame actually contained. Hydration is not broken -- it genuinely preserves
//! the nodes (`sameSvgNode: true`, markers consumed, handlers attached). It is
//! simply the worse trade here, on three measurements:
//!
//! 1. REPLACEMENT DOES NOT FLASH, which is the only thing hydration was going
//!    to buy. `AppHandle::mount_with_props` calls `clear_element(&host)`
//!    synchronously (app_handle.rs:32, :81) and then schedules the mount via
//!    `scheduler::push_component_create` -> `start()` -> `spawn_local`, a
//!    MICROTASK (scheduler.rs:313-321). Microtasks drain at the end of the
//!    current task, before the browser gets a paint opportunity, so the cleared
//!    container is never rendered. Measured across every run:
//!    `emptyFramesAfterFull: 0`, `minPolysSeen: 35` -- no sampled frame ever saw
//!    an empty board. Replacement also cannot double the content, because that
//!    same `clear_element` runs first.
//!
//! 2. THE FAILURE MODES ARE NOT COMPARABLE. I desynced the two renders on
//!    purpose (generator 8 tiles, wasm 7) and ran the identical page both ways:
//!
//!    ```text
//!      hydrate: panicked at yew-0.23.0/src/dom_bundle/btag/mod.rs:426
//!               "expected EOF, found node" + RuntimeError: unreachable.
//!               36 stale polygons still on screen, correctly styled, and the
//!               status line still read "no tile selected" after a click.
//!               A perfect-looking, permanently DEAD page. Loud in the console,
//!               silent in the window.
//!
//!      replace: prePolys 36 -> postPolys 35. Zero console errors. Status line
//!               read "selected museum". The wasm simply CORRECTED the stale
//!               markup and carried on.
//!    ```
//!
//!    Replacement is self-healing where hydration is fail-dead, and for a demo
//!    whose entire purpose is to be interactive in front of a stranger, a
//!    silently dead page is the worst available outcome.
//!
//! 3. HYDRATION WOULD TAX THE COMPONENT API, which another workflow is still
//!    designing. `VRef` and `VPortal` are hard panics under hydration
//!    (dom_bundle/bnode.rs:294-306, "VPortal is not hydratable") -- and a portal
//!    is the obvious way to lift a drag ghost above the lattice. Replacement
//!    forbids nothing. It also costs less: csr wasm 388,307 B vs hydration
//!    436,573 B (+48,266 B, +12.4%), plus the markers themselves, which on the
//!    portal's own output run to 22% of the delivered HTML.
//!
//! `.hydratable(false)` below is what drops the markers. It also makes this
//! build immune to the one risk that would otherwise stalk it: Trunk's
//! `minify_html` never sets `keep_comments`, so turning minification on strips
//! hydration markers and kills a hydrating page silently. With no markers to
//! strip, there is nothing to break -- and the placeholder assert below turns
//! that same setting into a red build instead of a dead page.

use demo::{AppProps, DemoApp};

/// The placeholder the generated body replaces. An HTML comment, so a developer
/// opening demo/index.html sees a valid, complete document.
const PLACEHOLDER: &str = "<!--HC-SSG-->";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Trunk hands hooks exactly six env vars. TRUNK_STAGING_DIR is the staging
    // copy of the finished index.html: Trunk writes it there (html.rs:186) BEFORE
    // running post_build hooks (html.rs:200), and only afterwards moves staging
    // into dist/. So editing it here lands in the published output.
    //
    // Writing only into staging is also why there is no `[watch] ignore` stanza
    // in Trunk.toml. A generated file inside the watched source tree makes
    // `trunk serve` see its own output as a source change and rebuild forever.
    let staging = std::env::var("TRUNK_STAGING_DIR").unwrap_or_else(|_| {
        // Running by hand outside Trunk, for debugging.
        eprintln!("demo-ssg: TRUNK_STAGING_DIR unset, falling back to demo/dist");
        "dist".to_string()
    });
    let dir = std::path::PathBuf::from(&staging);
    let index = dir.join("index.html");

    // hydratable(false): no <!--<[..]>--> markers in the output. See the header.
    let body = yew::ServerRenderer::<DemoApp>::with_props(AppProps::default)
        .hydratable(false)
        .render()
        .await;

    let shell = std::fs::read_to_string(&index)
        .unwrap_or_else(|e| panic!("demo-ssg: read {}: {e}", index.display()));

    // If this ever fires, the usual cause is minification: Trunk's minifier
    // drops HTML comments, and this placeholder is one. Failing the build here
    // is the entire point -- the alternative is publishing a page with an empty
    // container that only fills in once the wasm lands.
    assert!(
        shell.contains(PLACEHOLDER),
        "demo-ssg: {PLACEHOLDER} not found in {}.\n\
         The at-rest HTML cannot be generated without it. If Trunk.toml has \
         gained a `minify` setting, remove it: the minifier strips HTML comments.",
        index.display()
    );

    std::fs::write(&index, shell.replace(PLACEHOLDER, &body))
        .unwrap_or_else(|e| panic!("demo-ssg: write {}: {e}", index.display()));

    // The .ttl the page's "Download .ttl" link points at. A plain <a download>
    // to a real file, so it works with JavaScript switched off -- and once the
    // wasm is live it rewrites that href to a Blob of the CURRENT arrangement.
    //
    // THAT SECOND HALF USED TO BE FICTION. This comment claimed the upgrade from
    // the day it was written and nothing anywhere performed it, so after the
    // first drag the button handed out the generated document while the page
    // displayed a different one. The upgrade is in `DemoApp` now (see
    // `blob_url`), which is what makes this paragraph true rather than
    // aspirational.
    let ttl = demo::data::turtle(&demo::data::town_plan());
    let ttl_path = dir.join("honeycomb-demo.ttl");
    std::fs::write(&ttl_path, &ttl)
        .unwrap_or_else(|e| panic!("demo-ssg: write {}: {e}", ttl_path.display()));

    // GitHub Pages runs Jekyll over an artifact unless told not to, and Jekyll
    // silently drops files and directories whose names begin with an underscore.
    // Trunk's wasm-bindgen output does not use leading underscores today, but
    // this costs one empty file and removes the whole class of problem.
    std::fs::write(dir.join(".nojekyll"), "")
        .unwrap_or_else(|e| panic!("demo-ssg: write .nojekyll: {e}"));

    eprintln!(
        "demo-ssg: generated {} bytes of HTML into {} (+ honeycomb-demo.ttl, {} bytes)",
        body.len(),
        index.display(),
        ttl.len()
    );
}

/// WHAT THE GENERATED PAGE ACTUALLY SAYS, asserted against the same renderer
/// `main` above runs at build time.
///
/// THE ONLY PLACE `DemoApp` IS RENDERED ON THE HOST, which is why the demo's
/// own rendering is pinned from here and not from `demo`'s test module. `demo`
/// is built with `csr` by default and its component cannot be rendered to a
/// string at all under that feature; this crate depends on it with `ssr` and
/// nothing else, so the renderer is in hand for free. What comes out is the
/// delivered HTML — the whole of the page for a reader with no JavaScript, and
/// what a crawler indexes — so a claim proved here is a claim about what is
/// published rather than about what the wasm would eventually do.
#[cfg(test)]
mod generated {
    /// The generated body, rendered exactly the way `main` renders it.
    async fn page() -> String {
        yew::ServerRenderer::<crate::DemoApp>::with_props(crate::AppProps::default)
            .hydratable(false)
            .render()
            .await
    }

    /// Everything the renderer emitted for ONE line, from the component's own
    /// `data-link` wrapper up to whatever it opened next.
    ///
    /// A SUBSTRING WALK AND NOT A PARSER, deliberately: pulling in an HTML
    /// parser to read three attributes would put a dependency in this crate
    /// for the benefit of its tests alone, and `data-link` is the component's
    /// own stable hook — the browser tests in `honeycomb-yew` select on the
    /// identical attribute.
    fn line_markup<'a>(html: &'a str, id: &str) -> &'a str {
        let needle = format!("data-link=\"{id}\"");
        let start = html
            .find(&needle)
            .unwrap_or_else(|| panic!("the generated page draws no line called {id}"));
        let rest = &html[start + needle.len()..];
        let end = ["data-link=", "data-tile="]
            .iter()
            .filter_map(|n| rest.find(n))
            .min()
            .unwrap_or(rest.len());
        &rest[..end]
    }

    /// A LINE THAT MEETS A WHOLE DISTRICT MUST NOT LOOK LIKE A LINE THAT MEETS
    /// ONE BUILDING, and on the published page today it looks exactly like
    /// one.
    ///
    /// THE MISREADING THIS IS ABOUT IS SPECIFIC. The component trims a line
    /// back to `0.92r` from the anchor CELL's centre, and a district's ground
    /// is its members' hexagons grown to `1.16r` — so a line to a district
    /// stops well INSIDE the coloured region, a hair off one member's edge,
    /// in precisely the place a line to that one member would stop. A reader
    /// looking at the greenway sees a stroke ending at Orchard and concludes
    /// the Museum is connected to Orchard. It is connected to the Green Belt,
    /// and which building it touches changes the moment the district moves.
    ///
    /// MARKERS AND NOT GEOMETRY, because the host cannot redo the geometry.
    /// `LinkView::path` is a straight segment, a quadratic or a polyline
    /// depending on the routing, and re-trimming any of those to a region's
    /// outline would be a second copy of arithmetic this repository keeps in
    /// exactly one place. An SVG marker is oriented by the path's own tangent
    /// at the vertex it sits on, so one declaration works for all three
    /// routings with no arithmetic in the host at all.
    #[tokio::test]
    async fn a_line_that_ends_on_a_district_is_drawn_differently_from_one_that_ends_on_a_building()
    {
        let html = page().await;

        // The two new terminators are declared once each, beside the plain
        // arrowhead, in the one `<defs>` the frame callback owns. Once: a
        // second element with the same id makes "which one wins" a question
        // about statement order, which is the reason the arrowhead lives
        // there rather than in the per-tile callback.
        for id in ["hc-arrow", "hc-arrow-district", "hc-bar-district"] {
            assert_eq!(
                html.matches(&format!("id=\"{id}\"")).count(),
                1,
                "the generated page declares {id} other than exactly once, so which marker a \
                 line gets is a question about statement order"
            );
        }

        // BUILDING TO DISTRICT. The arrowhead says the direction and the
        // crossbar behind it says the thing it arrives at is a region.
        let commute = line_markup(&html, "commute");
        assert!(
            commute.contains("marker-end=\"url(#hc-arrow-district)\""),
            "the commute arrives at the whole Civic Quarter and is drawn with the plain \
             building arrowhead:\n{commute}"
        );
        assert!(
            commute.contains("marker-start=\"none\""),
            "the commute leaves from Station, one building, and something has capped it:\n\
             {commute}"
        );

        // DISTRICT TO BUILDING, the mirror — and the one line on the page
        // where a reader can see both terminators in a single stroke and work
        // out which is which without being told.
        let greenway = line_markup(&html, "greenway");
        assert!(
            greenway.contains("marker-start=\"url(#hc-bar-district)\""),
            "the greenway leaves the whole Green Belt and nothing says so:\n{greenway}"
        );
        assert!(
            greenway.contains("marker-end=\"url(#hc-arrow)\""),
            "the greenway arrives at the Museum, one building, and is drawn as a district \
             end:\n{greenway}"
        );

        // DISTRICT TO DISTRICT: capped at both ends.
        let errands = line_markup(&html, "errands");
        assert!(
            errands.contains("marker-start=\"url(#hc-bar-district)\"")
                && errands.contains("marker-end=\"url(#hc-arrow-district)\""),
            "errands runs between two whole districts and neither end says so:\n{errands}"
        );

        // AND A CLASS ON EACH END, because a marker cannot be restyled from a
        // stylesheet per line and the demo's whole point is that the host owns
        // every pixel of paint. `styles.css` reaches the terminator through
        // these.
        assert!(
            errands.contains("is-from-district") && errands.contains("is-to-district"),
            "styles.css has no handle on a district-ended line:\n{errands}"
        );
        assert!(
            !commute.contains("is-from-district") && commute.contains("is-to-district"),
            "the commute's two ends are not distinguished in its classes:\n{commute}"
        );
    }

    /// THE DELETE BUTTON MUST NOT SEND A READER DOWN A PATH THAT ENDS IN
    /// ANOTHER REFUSAL, which is the whole reason `Rejection` asks
    /// `GroupStillLinked` BEFORE `GroupInUse` — see that variant's own doc.
    /// Empty the district first is sound advice for an unlinked one and a
    /// dead end for a linked one: decision 4 refuses taking the LAST building
    /// out of a district a line reaches, so a reader who follows the
    /// instruction gets three buildings out and is then stopped by a rule the
    /// button never mentioned.
    ///
    /// Every district on the plan is now an end of some line, so the old
    /// advice must not appear on the page at all.
    #[tokio::test]
    async fn the_delete_button_on_a_linked_district_names_the_lines_not_the_buildings() {
        let html = page().await;

        assert!(
            !html.contains("Move its buildings out first"),
            "the page still tells a reader to empty a district that a line reaches, which \
             decision 4 will refuse them at the last building"
        );

        // NAMED, not counted. `StillLinked` and `LastMemberStillLinked` both
        // name their links for the same reason and the page's own `refusal`
        // prints those names; a button that said "some lines" would be the one
        // place on this page a reader is told there is a problem without
        // being told which thing to go and remove.
        for advice in [
            "Remove the lines that reach it first: commute, errands.",
            "Remove the line that reaches it first: greenway.",
            "Remove the line that reaches it first: errands.",
        ] {
            assert!(
                html.contains(advice),
                "the districts drawer never says {advice:?}, so the reader is not told which \
                 line is in the way"
            );
        }
    }
}
