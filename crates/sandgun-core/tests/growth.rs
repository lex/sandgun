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

#[test]
fn undisturbed_mycelium_ages_toward_maturity() {
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..300 {
        w.step();
    }
    // the original seed has been alive the whole time -> its aux age climbed
    assert!(w.cell_aux(64, 90) >= 90, "mature mycelium should have high aux age");
}

#[test]
fn mature_mycelium_fruits_a_mushroom_under_the_cap() {
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // force fruiting when eligible
    w.set_param(sandgun_core::params::P_MATURITY as u32, 10.0);
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    for _ in 0..200 {
        w.step();
    }
    assert!(w.mushroom_len() >= 1, "a mature patch should fruit");
    assert!(w.mushroom_len() <= 6, "must respect the global mushroom cap (default 6)");
}

#[test]
fn a_mushroom_grows_stem_then_cap_and_retires() {
    let mut w = World::new(64, 64);
    // open space above a floor so the mushroom has room
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    // directly enqueue a mushroom (bypass fruiting RNG) via the public try_fruit
    w.try_fruit(32, 49);
    let steps_needed = 2000;
    let mut saw_stem = false;
    for _ in 0..steps_needed {
        w.step();
        if w.get(32, 45) == Material::MushroomFlesh {
            saw_stem = true;
        }
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert!(saw_stem, "stem should have been revealed above the base");
    assert_eq!(w.mushroom_len(), 0, "completed mushroom must retire from the list");
    // A cap cell exists to the SIDE of the stem (x != 32, the stem column) -- excluding the
    // stem's own column so this genuinely checks for the cap dome, not just more stem. With
    // height in [6,16] and cap_r in [3,7] (default params), the dome (which now sits only at/
    // above the stem top) always falls within y in [26, 44] and x in [25, 39].
    let cap_present = (25..40).any(|x| {
        x != 32 && (26..45).any(|y| w.get(x, y) == Material::MushroomFlesh)
    });
    assert!(cap_present, "cap dome should have been revealed to the side of the stem");
}

#[test]
fn mushroom_cap_is_a_dome_not_a_ball() {
    // Deterministic shape: height=10, cap_r=5, base_y=49 -> stem spans y in [39, 48] at x=32,
    // and the dome (upper hemisphere) must sit AT/ABOVE the stem top (y in [34, 39]), never
    // bulging out to the sides below the stem top. A full-circle cap bug would paint flesh
    // beside the stalk (x != 32) in rows y in [40, 44] (cap_top_y+1 .. cap_top_y+r) -- that is
    // exactly what this test forbids.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor so the mushroom has room to grow up
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    w.try_fruit(32, 49); // base_y=49, height=10 -> stem top (cap_top_y) = 39

    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "mushroom must finish growing");

    // The stem itself is present at x=32 partway up (sanity check the shape actually grew).
    assert_eq!(w.get(32, 45), Material::MushroomFlesh, "stem should be present at x=32");

    // No flesh beside the stalk below the stem top (y in [40, 44] = cap_top_y+1 ..= cap_top_y+r):
    // a full-circle cap would bulge down around the stalk here; a dome must not.
    for y in 40..=44 {
        for x in 27..=37 {
            if x == 32 {
                continue; // the stem column itself
            }
            assert_eq!(
                w.get(x, y),
                Material::Empty,
                "cap must not bulge below the stem top at ({x}, {y})"
            );
        }
    }

    // The dome exists above (at/above) the stem top: the widest row (y = cap_top_y = 39) should
    // span the full cap radius, and flesh should reach up to the dome's apex near y = 34.
    let widest_row_full = (27..=37).all(|x| w.get(x, 39) == Material::MushroomFlesh);
    assert!(widest_row_full, "dome's widest row at the stem top should be fully filled");
    let dome_reaches_apex =
        (34..=38).any(|y| (27..=37).any(|x| w.get(x, y) == Material::MushroomFlesh));
    assert!(dome_reaches_apex, "dome should curve upward above the stem top");
}

#[test]
fn mycelium_cell_fruits_at_most_once() {
    // A mycelium cell (origin) with a Soil neighbor to its left and right (isolated everywhere
    // else, and floored so the soil can't avalanche), with open sky above so its own mushroom
    // stem grows straight up and never collides with the neighbors. With P_MATURITY=1 and
    // P_FRUIT_CHANCE=1.0, origin becomes mature on the very first growth tick it's processed
    // while it still has an un-consumed Soil neighbor -- a multi-tick window where the buggy
    // code (no "already fruited" guard) re-rolls fruiting on every subsequent tick it remains in
    // the frontier, stacking a second overlapping mushroom at the exact same (x, y) before it
    // exhausts both neighbors and retires. Each of the two Soil neighbors, once colonized, is
    // itself fully isolated (no other Soil/reachable-Empty neighbor, P_MAX_REACH=0 disables
    // bridging) so it can fruit at most once too -- that's correct, expected behavior, not the
    // bug. So the only way this scenario can ever produce MORE than 3 total mushrooms (origin +
    // its left neighbor + its right neighbor) is if the origin cell fruited more than once --
    // exactly the bug under test.
    let mut w = World::new(64, 64);
    for x in 29..35 {
        w.paint(x, 33, 0, Material::Rock as u8); // floor (wide enough to block diagonal avalanche)
    }
    w.paint(31, 32, 0, Material::Soil as u8); // left neighbor
    w.paint(33, 32, 0, Material::Soil as u8); // right neighbor
    w.paint(32, 32, 0, Material::Mycelium as u8); // origin; open sky above for its own stem

    w.set_param(sandgun_core::params::P_MATURITY as u32, 1.0);
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0);
    w.set_param(sandgun_core::params::P_MAX_MUSHROOMS as u32, 20.0); // cap must not be the limiter
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0); // no bridging into empty
    w.set_param(sandgun_core::params::P_GROWTH_INTERVAL as u32, 1.0); // one growth tick per step

    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "setup: origin should enter the frontier");

    let mut total_spawned: usize = 0;
    let mut prev_len = w.mushroom_len();
    for _ in 0..2000 {
        w.step();
        let len = w.mushroom_len();
        if len > prev_len {
            total_spawned += len - prev_len;
        }
        prev_len = len;
    }
    assert_eq!(w.frontier_len(), 0, "setup: growth should fully settle (all 3 cells exhausted)");
    assert_eq!(
        total_spawned, 3,
        "exactly 3 mushrooms total (origin + its 2 isolated neighbors), each cell fruiting \
         at most once -- more means the origin cell re-fruited"
    );
}

#[test]
fn spore_adjacent_to_soil_reseeds_a_colony() {
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_RESEED_CHANCE as u32, 1.0); // force reseed
    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so the soil can't avalanche under gravity
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    w.paint(20, 39, 0, Material::SporeGas as u8); // a spore resting on the soil surface
    // seed_frontier finds no mycelium, but puff_and_reseed still runs each growth tick
    w.seed_frontier();
    let before = mycelium_count(&w);
    for _ in 0..60 {
        w.step();
    }
    assert!(mycelium_count(&w) > before, "a spore on soil should seed new mycelium");
    assert!(w.frontier_len() >= 1, "the reseeded cell should join the frontier");
}

#[test]
fn reseed_consumes_spore_without_gas_rewake() {
    // Guards against dispatching update_gas on a stale material: once try_reseed()
    // consumes the spore (setting the cell to Empty), update_cell must not go on to
    // call update_gas() using the pre-reseed material for that now-empty cell.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_RESEED_CHANCE as u32, 1.0); // force reseed
    for x in 0..64 {
        w.paint(x, 41, 0, Material::Rock as u8); // floor so the soil can't avalanche under gravity
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    w.paint(20, 39, 0, Material::SporeGas as u8); // a spore resting on the soil surface
    w.step();
    assert_eq!(
        w.get(20, 39),
        Material::Empty,
        "the spore cell should be consumed (Empty) once it reseeds"
    );
    assert_eq!(
        w.get(20, 40),
        Material::Mycelium,
        "the adjacent soil should become the new mycelium seed"
    );
}

#[test]
fn shooting_mushroom_flesh_releases_spores() {
    let mut w = World::new(64, 64);
    // a slab of mushroom flesh
    for x in 28..36 {
        for y in 28..36 {
            w.paint(x, y, 0, Material::MushroomFlesh as u8);
        }
    }
    let spores_before = spore_count(&w);
    // fire a kinetic round into the slab
    w.fire(5.0, 32.0, 12.0, 0.0, 0); // Kinetic = 0
    for _ in 0..30 {
        w.step();
    }
    assert!(spore_count(&w) > spores_before, "popping mushroom flesh should release spore gas");
}

#[test]
fn burning_flesh_puffs_non_burning_spores() {
    // Regression: carve_crater must fully reset a carved MushroomFlesh cell's state when it
    // leaves SporeGas behind. If flags (specifically FLAG_BURNING) survive the conversion, the
    // fresh spore cell has aux=0 (SporeGas's initial aux) but is still flagged burning, so the
    // very next tick's dispatch checks FLAG_BURNING before material, routes to update_burning,
    // sees aux==0 ("fuel spent"), and detonates SporeGas -> Fire with no real fire nearby.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_GUNFIRE_SPORE_CHANCE as u32, 1.0); // always puff on carve
    w.set_param(sandgun_core::params::P_FLAM_FLESH as u32, 1.0); // guarantee ignition on contact

    // A single mushroom flesh cell, ignited via the normal fire-spread path (not by poking
    // flags directly) so this reproduces the real "fire already spread to a colony" scenario.
    w.paint(25, 32, 0, Material::MushroomFlesh as u8);
    w.paint(24, 32, 0, Material::Fire as u8); // adjacent flame catches the flesh next step
    w.step();
    assert_ne!(w.cell_flags(25, 32) & sandgun_core::cell::FLAG_BURNING, 0, "setup: flesh should have caught fire");

    // Remove the igniting fire so nothing but the burning flesh cell remains nearby.
    w.paint(24, 32, 0, Material::Empty as u8);

    // Shoot a Kinetic round into the still-burning flesh cell (travels 20 world units in a
    // single ray-march call, so the impact + carve resolve within this next step()).
    w.fire(5.0, 32.0, 20.0, 0.0, 0); // Kinetic = 0
    w.step();

    // Immediately after the impact resolves (before any spore could legitimately meet fire),
    // the carved cell must not be a pre-burning spore.
    assert_eq!(w.get(25, 32), Material::SporeGas, "carving a burning flesh cell should leave spore gas");
    assert_eq!(
        w.cell_flags(25, 32) & sandgun_core::cell::FLAG_BURNING,
        0,
        "a freshly carved spore cell must not inherit FLAG_BURNING from the flesh it replaced"
    );

    // And it must not phantom-detonate on the following tick with no real fire nearby (checked
    // world-wide since a rising spore gas cell may have drifted off its carve coordinates).
    w.step();
    assert_eq!(fire_count(&w), 0, "a carved spore cell must not self-detonate absent real fire");
}

fn fire_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Fire {
                n += 1;
            }
        }
    }
    n
}

fn spore_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::SporeGas {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn frontier_cap_is_a_hard_bound_and_counts_drops() {
    // A tiny cap with plenty of colonizable soil/mycelium to grow into: colonize_from and
    // seed_frontier must never let the frontier exceed the cap, and must count drops instead
    // of silently truncating (Task 3 review carry-forward).
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_MAX_FRONTIER as u32, 3.0);
    for x in (60..76).step_by(2) {
        w.paint(x, 90, 0, Material::Mycelium as u8);
    }
    w.seed_frontier();
    assert!(w.frontier_len() <= 3, "seed_frontier must respect the cap too");
    for _ in 0..80 {
        w.step();
        assert!(w.frontier_len() <= 3, "frontier must never exceed the hard cap");
    }
    assert!(w.frontier_drops() > 0, "hitting the cap should be counted, not silently truncated");
}

#[test]
fn clear_resets_frontier_drops() {
    // Ensure that World::clear() resets the frontier_drops counter alongside other
    // growth state, so a regenerated world doesn't carry stale drop counts.
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_MAX_FRONTIER as u32, 3.0);
    for x in (60..76).step_by(2) {
        w.paint(x, 90, 0, Material::Mycelium as u8);
    }
    w.seed_frontier();
    for _ in 0..80 {
        w.step();
    }
    // frontier cap has been hit, drops counter is nonzero
    assert!(w.frontier_drops() > 0, "setup: frontier cap should have been hit");

    // After clear(), frontier_drops must reset to 0 (alongside frontier, mushrooms, etc.)
    w.clear();
    assert_eq!(w.frontier_drops(), 0, "clear() must reset frontier_drops to 0");
}

#[test]
fn burning_mycelium_does_not_fruit() {
    // Regression: grow()'s fruiting check reads self.cells[ci].aux >= maturity without
    // gating on FLAG_BURNING. aux holds MATURITY on live mycelium but holds FIRE FUEL
    // once a cell ignites (P_FUEL_MYCELIUM default 130 > P_MATURITY default 90), so a
    // freshly-ignited mycelium cell's fuel value alone satisfies the fruiting threshold
    // and can spuriously fruit a mushroom while burning -- fire should kill growth, not
    // trigger it.
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // force fruiting whenever eligible
    w.set_param(sandgun_core::params::P_FLAM_MYCELIUM as u32, 1.0); // guarantee ignition on contact

    w.paint(64, 90, 0, Material::Mycelium as u8); // seed in the soil band
    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "setup: seed should enter the frontier");

    // Ignite it via real fire spread (not by poking flags directly), same approach as
    // burning_flesh_puffs_non_burning_spores.
    w.paint(63, 90, 0, Material::Fire as u8);
    w.step();
    assert_ne!(
        w.cell_flags(64, 90) & sandgun_core::cell::FLAG_BURNING,
        0,
        "setup: mycelium should have caught fire"
    );
    // Fuel (130) must exceed maturity (90) for this test to actually exercise the bug.
    assert!(
        w.cell_aux(64, 90) >= sandgun_core::params::Params::default().values[sandgun_core::params::P_MATURITY] as u8,
        "setup: burning cell's fuel aux must be >= maturity threshold"
    );

    // Remove the igniting fire so nothing else nearby could trigger anything.
    w.paint(63, 90, 0, Material::Empty as u8);

    for _ in 0..30 {
        w.step();
        assert_eq!(w.mushroom_len(), 0, "a burning mycelium cell must never fruit");
    }
}

#[test]
fn spore_ammo_plants_living_mycelium() {
    // Regression: on_impact's Ammo::Spore arm painted mycelium via inject_blob but never
    // registered those cells in the growth frontier, leaving shot-in mycelium permanently
    // inert (never spreads/ages/fruits) unlike worldgen/colonized/bridged/reseeded mycelium.
    let mut w = soil_world();
    w.fire(5.0, 85.0, 12.0, 0.0, 3); // Ammo::Spore = 3, fired into the soil band
    w.step();
    assert!(w.frontier_len() > 0, "planted mycelium must be seeded into the growth frontier");

    let before = mycelium_count(&w);
    for _ in 0..300 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before, "planted mycelium should actually spread ({before} -> {after})");
}

#[test]
fn full_lifecycle_world_still_sleeps_after_settling() {
    // avatar + projectile + particles + active growth all at once, then everything must settle to sleep.
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    w.spawn_avatar(60.0, 70.0);
    w.fire(5.0, 85.0, 12.0, 0.0, 0);
    w.spawn_particle(40.0, 60.0, 0.5, 0.0, Material::Sand as u8);
    // soil_world()'s soil band is ~2500 cells; the budgeted grow() intentionally throttles
    // colonization (P_GROWTH_BUDGET cells every P_GROWTH_INTERVAL frames) so the world never
    // busy-spins on a single frame. Verified by trace: this scenario fully drains (frontier and
    // mushrooms both to 0, soil fully consumed) by ~step 27000. 40000 gives comfortable margin
    // without weakening the assertion below — it's still a hard requirement that everything
    // reaches exactly zero.
    for _ in 0..40_000 {
        w.step();
    }
    assert_eq!(w.frontier_len(), 0, "growth must terminate");
    assert_eq!(w.mushroom_len(), 0, "mushrooms must finish");
    w.step();
    assert_eq!(w.cells_processed, 0, "full living world must return to sleep once settled");
}
