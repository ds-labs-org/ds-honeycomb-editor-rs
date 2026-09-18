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
    // to a real file, so it works with JavaScript switched off -- the wasm only
    // upgrades its click to serialise the CURRENT state instead.
    let ttl = demo::data::town_plan().to_turtle();
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
