use sandgun_core::cell::Material;
use sandgun_core::world::World;

// A world with a solid rock floor and a band of soil above it, no worldgen.
fn soil_world() -> World {
    let mut w = World::new(128, 128);
    for x in 0..128 {
        w.paint(x as i32, 100, 0, Material::Rock as u8);
        for y in 80..100 {
            w.paint(x as i32, y as i32, 0, Material::Soil as u8);
        }
    }
    w
}

#[test]
fn empty_frontier_grow_is_noop() {
    let mut w = soil_world();
    // no mycelium anywhere -> nothing to seed, grow() must do nothing
    w.seed_frontier();
    assert_eq!(w.frontier_len(), 0);
    for _ in 0..50 {
        w.step();
    }
    // settled world with an empty frontier costs nothing
    w.step();
    assert_eq!(w.cells_processed, 0);
}

#[test]
fn mycelium_colonizes_adjacent_soil_over_ticks() {
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8); // one seed in the soil band
    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "seed should enter the frontier");
    let before = mycelium_count(&w);
    for _ in 0..120 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before + 5, "mycelium should spread into soil ({before} -> {after})");
}

#[test]
fn growth_settles_and_frontier_drains() {
    // small fully-enclosed soil pocket: growth must terminate and the world sleep
    let mut w = World::new(64, 64);
    for x in 20..30 {
        for y in 20..30 {
            w.paint(x, y, 0, Material::Soil as u8);
        }
    }
    w.paint(25, 25, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..2000 {
        w.step();
    }
    assert_eq!(w.frontier_len(), 0, "frontier must retire to empty");
    w.step();
    assert_eq!(w.cells_processed, 0, "settled grown world must sleep");
}

fn mycelium_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Mycelium {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn mycelium_bridges_a_small_gap_but_not_open_air() {
    // soil platform with a 3-wide empty notch carved in it; mycelium should bridge the notch
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so the platform can't avalanche under gravity
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    for x in 30..33 {
        w.paint(x, 40, 0, Material::Empty as u8); // the notch
    }
    w.paint(20, 40, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..600 {
        w.step();
    }
    // it reached across the notch (a cell in the former gap is now mycelium)
    let bridged = (30..33).any(|x| w.get(x, 40) == Material::Mycelium);
    assert!(bridged, "mycelium should bridge the small empty notch");
    // but it did NOT grow far up into open air above the platform
    assert_eq!(w.get(20, 30), Material::Empty, "must not sprawl into open air");
}

#[test]
fn bridging_respects_max_reach() {
    // Full-width rock floor at y=50, with a 4-wide soil block resting directly on it
    // at y=49 (x=10..=13). update_powder only moves a powder cell into an Empty cell
    // straight down or down-diagonally; here every cell in row 50 across the whole
    // width is Rock, so for every soil cell in the block, straight-down AND both
    // down-diagonals are non-empty (Rock). No soil cell can ever fall or avalanche —
    // this isolates the pure reach-counter cap with no soil-shedding side channel.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 2.0);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // solid floor across the whole width
    }
    for x in 10..=13 {
        w.paint(x, 49, 0, Material::Soil as u8); // a 4-wide soil block resting on the floor
    }
    // Sanity: the block is stationary before we seed growth.
    for _ in 0..10 {
        w.step();
    }
    for x in 10..=13 {
        assert_eq!(w.get(x, 49), Material::Soil, "soil block must not avalanche before seeding");
    }

    w.paint(13, 49, 0, Material::Mycelium as u8); // seed at the block's right edge
    w.seed_frontier();
    for _ in 0..800 {
        w.step();
    }
    // The seed sits at x=13 (part of the soil mass). With P_MAX_REACH=2, mycelium may
    // bridge empty cells at most 2 cells beyond the mass: x=14 (reach 1) and x=15
    // (reach 2). x=16 (reach would be 3) and x=17 (reach would be 4) are both beyond
    // the cap and must remain Empty — a non-coincidental margin on both sides of the cap.
    assert_eq!(w.get(14, 49), Material::Mycelium, "bridging must have happened at all");
    assert_eq!(w.get(16, 49), Material::Empty, "reach cap must stop bridging beyond 2 cells");
    assert_eq!(w.get(17, 49), Material::Empty, "reach cap must stop bridging beyond 2 cells");
}
