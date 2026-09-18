//! The dummy dataset: a town plan. Twelve tiles, three districts, 28 cells.
//!
//! Generic on purpose -- no product names, no real deployment data, nothing
//! anyone could mistake for a record of something. "A district moves as one"
//! needs no explaining to a stranger, which is the whole job of demo data.
//!
//! It is a plain `fn` of nothing: no clock, no randomness, no environment, no
//! locale. That is a hard requirement, not a habit. `demo-ssg` calls this on the
//! HOST at build time and the browser calls it again at boot; if the two could
//! disagree, the page a visitor sees before the wasm lands would be a picture of
//! a diagram that never existed.
//!
//! The placement is rigged so each behaviour is one short drag away:
//!
//!   * Museum (E4) sits alone with free cells all round -- a plain move.
//!   * Museum is adjacent to Cinema: (3,3) + (+1,-1) = (4,2). The shortest
//!     possible refused drop, and both tiles are ungrouped, so the refusal is
//!     not confounded by a group also moving.
//!   * Station (G1) sits directly above Grocer (G2), so dragging Market Row up
//!     one row puts Grocer onto Station -- a GROUP move refused because ONE
//!     member collides, which is the actual rule.
//!   * Civic Quarter is a 2x2 rhombus; moving it down two rows puts School onto
//!     Orchard.

use honeycomb_yew::{Cell, Group, Lattice, Tile};

fn tile(id: &str, label: &str, group: Option<&str>, q: i32, r: i32) -> Tile {
    Tile {
        id: id.into(),
        label: label.into(),
        group: group.map(Into::into),
        q,
        r,
    }
}

pub fn town_plan() -> Lattice {
    // 4 rows of 7. Rows are staggered by the axial coordinates themselves, so
    // the silhouette comes out of the geometry rather than being drawn in.
    let cells = (0..4)
        .flat_map(|r| (0..7).map(move |q| Cell { q, r }))
        .collect();

    Lattice {
        id: "plan".into(),
        label: "Town plan (demo)".into(),
        cells,
        groups: vec![
            Group { id: "civic".into(),  label: "Civic Quarter".into(), slot: 1 },
            Group { id: "green".into(),  label: "Green Belt".into(),    slot: 2 },
            Group { id: "market".into(), label: "Market Row".into(),    slot: 3 },
        ],
        tiles: vec![
            tile("hall",    "Town Hall", Some("civic"),  1, 0),
            tile("library", "Library",   Some("civic"),  2, 0),
            tile("school",  "School",    Some("civic"),  1, 1),
            tile("clinic",  "Clinic",    Some("civic"),  2, 1),
            tile("bakery",  "Bakery",    Some("market"), 4, 1),
            tile("grocer",  "Grocer",    Some("market"), 5, 1),
            tile("florist", "Florist",   Some("market"), 6, 1),
            tile("park",    "Park",      Some("green"),  0, 3),
            tile("orchard", "Orchard",   Some("green"),  1, 3),
            tile("station", "Station",   None,           5, 0),
            tile("cinema",  "Cinema",    None,           4, 2),
            tile("museum",  "Museum",    None,           3, 3),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture is the contract between the build-time render and the
    /// browser render. If it ever stops being a pure function of nothing, this
    /// is what says so.
    #[test]
    fn fixture_is_deterministic() {
        assert_eq!(town_plan(), town_plan());
    }

    #[test]
    fn rigged_drags_behave_as_documented() {
        let l = town_plan();
        // Museum onto Cinema is refused, and names Cinema.
        let moving = l.moving_set("museum");
        assert_eq!(moving.len(), 1, "Museum is ungrouped");
        let refusal = l.check_move(&moving, 1, -1).unwrap_err();
        assert_eq!(refusal.blocker.as_deref(), Some("Cinema"));
        // Market Row up one row is refused because Grocer hits Station.
        let market = l.moving_set("bakery");
        assert_eq!(market.len(), 3, "Market Row has three members");
        let refusal = l.check_move(&market, 0, -1).unwrap_err();
        assert_eq!(refusal.blocker.as_deref(), Some("Station"));
        // Museum one cell east is free.
        assert!(l.check_move(&moving, 1, 0).is_ok());
    }
}
