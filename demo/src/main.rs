//! The browser entry point.
//!
//! `render()`, not `hydrate()`. The wasm REPLACES the generated markup rather
//! than attaching to it -- see demo-ssg/src/main.rs for the measurements behind
//! that choice. `Renderer::render()` clears the root first (yew 0.23
//! app_handle.rs:32 calls `clear_element` before mounting), so this does not
//! duplicate the pre-rendered diagram, and the clear and the mount both land
//! inside one microtask checkpoint, so the browser never paints the gap.

use demo::DemoApp;

fn main() {
    let root = web_sys::window()
        .expect("window")
        .document()
        .expect("document")
        .get_element_by_id("hc-app")
        .expect("#hc-app is in index.html");

    yew::Renderer::<DemoApp>::with_root(root).render();
}
