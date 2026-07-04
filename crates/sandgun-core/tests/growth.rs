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
    // but mycelium COLONIZATION did NOT sprawl far up into open air above the platform (a
    // mushroom's stem legitimately growing straight up through open air, per the bounded
    // fruiting window, is expected and is not what this assertion guards against).
    assert_ne!(w.get(20, 30), Material::Mycelium, "mycelium colonization must not sprawl into open air");
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
    // An isolated mycelium seed (colonizes its one Soil neighbor, then sits exhausted with
    // permanent headroom) so its own aging is the only frontier activity. Using the full
    // soil_world() band here would put this seed's aux progress at the mercy of the shared
    // per-tick processing budget: with the bounded fruiting window (Fix 1), open soil-surface
    // mycelium across the whole band now legitimately lingers for many ticks trying to fruit,
    // which can dilute an unrelated cell's share of that budget to the point its aux barely
    // moves within this test's step count -- an emergent (and correct) consequence of Fix 1,
    // not something this test is meant to exercise.
    let mut w = World::new(64, 64);
    for x in 29..35 {
        w.paint(x, 33, 0, Material::Rock as u8); // floor (wide enough to block diagonal avalanche)
    }
    w.paint(31, 32, 0, Material::Rock as u8); // left: sealed
    w.paint(33, 32, 0, Material::Soil as u8); // right: the one-time colonizable neighbor
    w.paint(34, 32, 0, Material::Rock as u8); // seals the far side of that neighbor
    w.paint(33, 31, 0, Material::Rock as u8); // no headroom for that neighbor
    w.paint(32, 32, 0, Material::Mycelium as u8); // origin; headroom above stays Empty
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0); // no bridging into the headroom shaft

    w.seed_frontier();
    for _ in 0..300 {
        w.step();
    }
    // the original seed has been alive the whole time -> its aux age climbed
    assert!(w.cell_aux(32, 32) >= 90, "mature mycelium should have high aux age");
}

#[test]
fn mature_mycelium_fruits_a_mushroom_under_the_cap() {
    let mut w = soil_world();
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // force fruiting when eligible
    w.set_param(sandgun_core::params::P_MATURITY as u32, 10.0);
    // Disable bridging so colonize_from can't grow mycelium up into the carved headroom shaft
    // (below) faster than aux ages to maturity, which would silently destroy the headroom this
    // test depends on before the seed ever gets a chance to fruit.
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0);
    // Carve a headroom clearing above the seed (instead of the old buggy setup, which buried
    // the seed mid-band with no headroom at all) -- this keeps the test narrowly scoped to a
    // single fruiting candidate (like the original), rather than exposing the whole soil band's
    // surface, which would let dozens of cells fruit and retire well within 200 steps.
    //
    // Fix A (mushroom footprint fit-check) now requires the WHOLE footprint -- stem plus cap
    // dome, not just a shallow peek above the origin -- to be genuinely Empty before a mushroom
    // can spawn, so the clearing must be wide and deep enough for the tallest/widest shape the
    // default height (max 16) / cap radius (max 7) ranges can roll, with a small margin.
    for dy in 1..=25i32 {
        for dx in -8..=8i32 {
            w.paint(64 + dx, 90 - dy, 0, Material::Empty as u8);
        }
    }
    w.paint(64, 90, 0, Material::Mycelium as u8);
    w.seed_frontier();
    // Track whether a mushroom ever spawned, rather than only checking at the very end --
    // Fix A's stricter fit-check can shift exactly when (relative to this fixed step budget)
    // the eligible candidate's footprint clears, without changing whether it fruits at all.
    let mut fruited = false;
    for _ in 0..200 {
        w.step();
        if w.mushroom_len() >= 1 {
            fruited = true;
        }
        assert!(w.mushroom_len() <= 6, "must respect the global mushroom cap (default 6)");
    }
    assert!(fruited, "a mature patch should fruit");
}

#[test]
fn mycelium_buried_in_soil_does_not_fruit() {
    // Regression: fruiting had no headroom check, so mycelium buried deep inside soil (no empty
    // cell above it, ever) could fruit and try to grow a mushroom stem straight through solid
    // ground. A fully sealed rock room packed with soil guarantees no cell -- not even one at
    // the colonized edge of the pocket -- can ever see empty headroom above it (the room's
    // ceiling and walls are Rock, not Empty), so this isolates the pure headroom gate: even
    // once the whole pocket is colonized and every cell is well past maturity, none can fruit.
    let mut w = World::new(64, 64);
    for x in 19..=30 {
        w.paint(x, 19, 0, Material::Rock as u8); // ceiling
        w.paint(x, 30, 0, Material::Rock as u8); // floor
    }
    for y in 19..=30 {
        w.paint(19, y, 0, Material::Rock as u8); // left wall
        w.paint(30, y, 0, Material::Rock as u8); // right wall
    }
    for x in 20..30 {
        for y in 20..30 {
            w.paint(x, y, 0, Material::Soil as u8); // sealed interior, packed with soil
        }
    }
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // force fruiting when eligible
    w.set_param(sandgun_core::params::P_MATURITY as u32, 1.0); // mature almost immediately
    w.paint(25, 25, 0, Material::Mycelium as u8); // deep inside the sealed, soil-packed room
    w.seed_frontier();
    for _ in 0..500 {
        w.step();
    }
    assert_eq!(w.mushroom_len(), 0, "mycelium sealed away from all empty space must never fruit");
}

#[test]
fn mushroom_does_not_overwrite_soil() {
    // Regression: reveal_mushroom used to write MushroomFlesh into Soil as well as Empty,
    // letting a growing mushroom carve straight through solid ground. It must only ever
    // occupy Empty cells. The soil column is trapped in a rock channel (rock on both sides,
    // rock floor below) so it physically cannot avalanche away under gravity -- isolating the
    // pure "must not overwrite soil" behavior from unrelated powder-physics sliding.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 50, 0, Material::Rock as u8); // floor so the mushroom has room to grow up
    }
    for y in 20..50 {
        w.paint(34, y, 0, Material::Rock as u8); // channel wall
        w.paint(35, y, 0, Material::Soil as u8); // soil, trapped in the channel
        w.paint(36, y, 0, Material::Rock as u8); // channel wall
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 10.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    w.try_fruit(32, 49); // dome would normally span roughly x in [25,39], overlapping x=35

    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "mushroom must finish growing");
    for y in 20..50 {
        assert_eq!(
            w.get(35, y),
            Material::Soil,
            "mushroom growth must not overwrite soil at (35, {y})"
        );
    }
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
        // The stem wanders gently (+/-2 cells max, see try_fruit's sway_seed), so check a
        // small window around the base x rather than the exact column.
        if (30..=34).any(|x| w.get(x, 45) == Material::MushroomFlesh) {
            saw_stem = true;
        }
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert!(saw_stem, "stem should have been revealed above the base");
    assert_eq!(w.mushroom_len(), 0, "completed mushroom must retire from the list");
    // A cap cell exists to the SIDE of the stem -- excluding a window around the stem's own
    // (gently wandering, +/-2 cell) column so this genuinely checks for the cap dome, not just
    // more stem. With height in [6,16] and cap_r in [3,7] (default params) plus the +/-2 sway,
    // the dome (which sits only at/above the stem top) always falls within y in [26, 44] and
    // x in [23, 41].
    let cap_present = (23..42).any(|x| {
        !(30..=34).contains(&x) && (26..45).any(|y| w.get(x, y) == Material::MushroomFlesh)
    });
    assert!(cap_present, "cap dome should have been revealed to the side of the stem");
}

#[test]
fn mushroom_cap_is_a_dome_not_a_ball() {
    // Deterministic shape: height=10, cap_r=5, base_y=49 -> stem spans y in [39, 48] around
    // x=32 (with a gentle +/-2 cell wander), and the dome (upper hemisphere) must sit AT/ABOVE
    // the stem top (y in [34, 39]), never bulging out to the sides below the stem top. A
    // full-circle cap bug would paint flesh beside the stalk in rows y in [40, 44]
    // (cap_top_y+1 .. cap_top_y+r) -- that is exactly what this test forbids.
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

    // The stem itself is present near x=32 partway up (sanity check the shape actually grew;
    // the stem may wander +/-2 cells from the base x, hence the small window).
    assert!(
        (30..=34).any(|x| w.get(x, 45) == Material::MushroomFlesh),
        "stem should be present near x=32"
    );

    // Locate the dome's ACTUAL center via its widest row (cap_top_y = 39): it must be one
    // contiguous run of width 2*cap_r+1 = 11, wherever the (possibly wandered) stem top put it.
    let cap_top_y = 39usize;
    let dome_xs: Vec<i32> =
        (20..=44).filter(|&x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh).collect();
    assert_eq!(dome_xs.len(), 11, "dome's widest row at the stem top should be fully filled (width 11)");
    let (dmin, dmax) = (*dome_xs.iter().min().unwrap(), *dome_xs.iter().max().unwrap());
    assert_eq!(dmax - dmin + 1, 11, "dome's widest row must be one contiguous run, not scattered");
    let dome_center = (dmin + dmax) / 2;

    // No flesh beside the stalk below the stem top (y in [40, 44] = cap_top_y+1 ..= cap_top_y+r),
    // except within a small window around the stem's own (gently wandering) column: a
    // full-circle cap would bulge flesh out to dome_center +/- r here; a dome must not. The
    // wander amplitude is capped at 2, and cap_r=5 here, so the excluded window (5 cells wide)
    // can never swallow the whole dome width (11 cells).
    for y in 40..=44 {
        for x in (dome_center - 5)..=(dome_center + 5) {
            if (30..=34).contains(&x) {
                continue; // the stem's own (slightly wandering) column
            }
            assert_eq!(
                w.get(x as usize, y),
                Material::Empty,
                "cap must not bulge below the stem top at ({x}, {y})"
            );
        }
    }

    // The dome curves upward above the stem top toward its apex near y = 34.
    let dome_reaches_apex = (34..=38)
        .any(|y| (dome_center - 5..=dome_center + 5).any(|x| w.get(x as usize, y) == Material::MushroomFlesh));
    assert!(dome_reaches_apex, "dome should curve upward above the stem top");
}

#[test]
fn mushroom_cap_reveals_bottom_row_before_apex() {
    // Regression: cap_dome() used to be ordered top-down (dy from -r to 0), so the very FIRST
    // cap cell revealed was the apex -- a single flesh cell floating above the stem before the
    // rest of the dome filled in. It must reveal bottom-up so the cap stays grounded/connected
    // to the stem top throughout its growth.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    let height: i32 = 5;
    let cap_r: i32 = 5;
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, height as f32);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, height as f32);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, cap_r as f32);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, cap_r as f32);
    w.set_param(sandgun_core::params::P_MUSH_REVEAL as u32, 1.0); // exactly one cell revealed per growth tick
    w.set_param(sandgun_core::params::P_GROWTH_INTERVAL as u32, 1.0); // one growth tick per step

    let base_x = 32i32;
    let base_y = 59i32;
    w.try_fruit(base_x as usize, base_y as usize);

    // `height` steps fully reveal the stem (progress == stem == height); the (height+1)th
    // step reveals exactly the FIRST entry of cap_dome(cap_r) -- nothing else.
    for _ in 0..(height + 1) {
        w.step();
    }

    let cap_top_y = (base_y - height) as usize;
    let apex_y = (cap_top_y as i32 - cap_r) as usize;
    let window = cap_r + 2; // covers the stem's +/-2 sway plus the dome radius

    let bottom_row_filled = (base_x - window..=base_x + window)
        .any(|x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh);
    let apex_filled = (base_x - window..=base_x + window)
        .any(|x| w.get(x as usize, apex_y) == Material::MushroomFlesh);

    assert!(
        bottom_row_filled,
        "the first cap cell revealed should be on the widest row, flush against the stem top"
    );
    assert!(!apex_filled, "the apex must not be revealed before the widest row grounds the cap");
}

#[test]
fn mushrooms_do_not_overlap() {
    // Regression: reveal_mushroom only ever writes into Empty cells (won't overwrite), but
    // nothing checked at FRUITING time that the whole footprint was clear, so adjacent
    // mushrooms could be enqueued back-to-back and interleave/merge as they both grew into the
    // same space. A fit-check at fruiting time must reject a mushroom whose footprint would
    // land on top of an already-grown one.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor, open headroom above
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);

    assert!(w.try_fruit(32, 59), "first mushroom should spawn freely in open space");
    // Let the first mushroom fully grow before the second tries to fruit into overlapping
    // ground, so the fit-check has real flesh cells (not just a pending plan) to detect.
    for _ in 0..2000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "setup: first mushroom should finish growing");

    // Second mushroom's base is only 2 cells away -- with cap_r=5 (diameter 11) its footprint
    // necessarily overlaps the first mushroom's now-solid flesh, regardless of stem sway.
    let spawned = w.try_fruit(34, 59);
    assert!(!spawned, "a mushroom whose footprint overlaps another must not spawn");
    assert_eq!(w.mushroom_len(), 0, "an overlapping mushroom must not be added to the growing list");
}

#[test]
fn mushroom_needs_clear_footprint() {
    // Regression: fruiting had no whole-footprint check, so a mycelium cell whose cap would
    // land on an obstruction (ceiling, another mushroom, etc.) could still fruit and grow a
    // stem straight into it, letting the cap dome carve through or interleave with solid cells.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 5.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 5.0);
    // With base_y=59 and height=5, the stem occupies rows y in [54, 58]; the cap dome (radius 5)
    // occupies rows y in [49, 54]. Block a wide rock ceiling across rows [49, 53] -- pure dome
    // territory, wide enough to cover the stem's +/-2 sway -- while leaving the stem's own rows
    // open, isolating "cap footprint obstructed" from "stem footprint obstructed".
    for x in 20..45 {
        for y in 49..=53 {
            w.paint(x, y, 0, Material::Rock as u8);
        }
    }

    let spawned = w.try_fruit(32, 59);
    assert!(!spawned, "a mushroom whose cap footprint is obstructed must not spawn");
    assert_eq!(w.mushroom_len(), 0, "an obstructed mushroom must not be added to the growing list");
}

#[test]
fn colony_reliably_fruits() {
    // Regression: grow() used to retire an exhausted, mature, room-having frontier cell the
    // instant it exhausted (aux >= maturity), which meant it got essentially ONE fruiting
    // roll (default P_FRUIT_CHANCE = 0.02) before retiring for good -- whole colonies could
    // (and empirically did, ~10% of the time) produce zero mushrooms. A cell that still has
    // fruiting room should get a bounded but generous window of rolls (aux climbing from
    // maturity up to the u8 ceiling of 255) before giving up.
    //
    // Setup: a single mycelium seed with exactly one Soil neighbor (so it enters the
    // frontier) that gets colonized on the very first growth tick, after which the seed has
    // no more colonizable neighbors (walled in by Rock on the other sides) but permanent
    // empty headroom above it -- i.e. it becomes exhausted almost immediately while still
    // being a perfectly viable fruiting candidate for the rest of the test.
    let mut w = World::new(64, 64);
    for x in 29..35 {
        w.paint(x, 33, 0, Material::Rock as u8); // floor (wide enough to block diagonal avalanche)
    }
    w.paint(31, 32, 0, Material::Rock as u8); // left: sealed
    w.paint(33, 32, 0, Material::Soil as u8); // right: the one-time colonizable neighbor
    // Seal the far side + headroom of that neighbor so it can't itself cascade or fruit,
    // keeping this test narrowly scoped to the origin cell's own fruiting window.
    w.paint(34, 32, 0, Material::Rock as u8);
    w.paint(33, 31, 0, Material::Rock as u8);
    w.paint(32, 32, 0, Material::Mycelium as u8); // origin; headroom above (32,31..28) stays Empty

    w.set_param(sandgun_core::params::P_MATURITY as u32, 20.0); // matures quickly
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 0.02); // default-ish, low odds per roll
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0); // no bridging into the headroom shaft
    w.set_param(sandgun_core::params::P_GROWTH_INTERVAL as u32, 1.0); // one growth tick per step

    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "setup: origin should enter the frontier");

    for _ in 0..300 {
        w.step();
    }

    assert!(
        w.mushroom_len() >= 1 || mushroom_flesh_exists(&w),
        "a mature, room-having colony should reliably fruit within its aging window"
    );
}

fn mushroom_flesh_exists(w: &World) -> bool {
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::MushroomFlesh {
                return true;
            }
        }
    }
    false
}

#[test]
fn fruited_cell_does_not_refruit_even_with_restored_room() {
    // Regression guard for FLAG_FRUITED itself. `mycelium_cell_fruits_at_most_once` (below)
    // is confounded: once a cell's own mushroom starts growing, its stem fills the exact
    // headroom column `has_fruiting_room` checks, so the cell stops being a fruiting
    // candidate (and, post bounded-window fix, retires) regardless of whether FLAG_FRUITED
    // is ever consulted. That test would keep passing even if FLAG_FRUITED were deleted.
    //
    // This test isolates the flag by exploiting the fact that `aux` and cell flags live on
    // the CELL, not on its (transient) frontier entry: once the origin fruits and its
    // mushroom finishes growing, it retires from the frontier (either for lack of room, or
    // -- with the bounded-window fix -- because it's already fruited). We then bulldoze the
    // grown mushroom back to Empty (restoring headroom) and give the origin a fresh Soil
    // neighbor, then call `seed_frontier()` again: it re-scans the world and re-enqueues any
    // Mycelium cell with a colonizable neighbor WITHOUT touching aux/flags, so the origin
    // goes back on the frontier still mature and still (if the fix is present) FLAG_FRUITED.
    // Without that flag check, this re-entry would fruit a second time; with it, it must not.
    let mut w = World::new(64, 64);
    let (ox, oy) = (32i32, 32i32);
    let nx = 33i32; // the one-time colonizable neighbor slot, reused for the re-seed later

    for x in 29..35 {
        w.paint(x, 33, 0, Material::Rock as u8); // floor (wide enough to block diagonal avalanche)
    }
    w.paint(31, 32, 0, Material::Rock as u8); // left: sealed
    w.paint(nx, 32, 0, Material::Soil as u8); // right: the one-time colonizable neighbor
    // Seal the far side + headroom of that neighbor so it can't itself cascade or fruit.
    w.paint(34, 32, 0, Material::Rock as u8);
    w.paint(nx, 31, 0, Material::Rock as u8);
    w.paint(ox, oy, 0, Material::Mycelium as u8); // origin; headroom above stays Empty

    w.set_param(sandgun_core::params::P_MATURITY as u32, 3.0);
    w.set_param(sandgun_core::params::P_FRUIT_CHANCE as u32, 1.0); // deterministic: fruit the instant eligible
    w.set_param(sandgun_core::params::P_MAX_MUSHROOMS as u32, 20.0);
    w.set_param(sandgun_core::params::P_MAX_REACH as u32, 0.0); // no bridging into the headroom shaft
    w.set_param(sandgun_core::params::P_GROWTH_INTERVAL as u32, 1.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 3.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 3.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 2.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 2.0);

    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "setup: origin should enter the frontier");

    let mut total_spawned: usize = 0;
    let mut prev_len = w.mushroom_len();
    for _ in 0..300 {
        w.step();
        let len = w.mushroom_len();
        if len > prev_len {
            total_spawned += len - prev_len;
        }
        prev_len = len;
    }
    assert_eq!(w.frontier_len(), 0, "setup: the colony should fully settle");
    assert_eq!(w.mushroom_len(), 0, "setup: the mushroom should finish growing");
    assert_eq!(total_spawned, 1, "setup: exactly one mushroom should have grown");
    assert_ne!(
        w.cell_flags(ox as usize, oy as usize) & sandgun_core::cell::FLAG_FRUITED,
        0,
        "setup: origin must carry FLAG_FRUITED after fruiting"
    );

    // Restore the origin's fruiting room: bulldoze the grown mushroom back to Empty, while
    // the origin is still mature, unburned mycelium. Only actual MushroomFlesh cells are
    // cleared (not the whole bounding box) so the neighbor's own headroom-blocking Rock seal
    // survives -- otherwise that neighbor could gain real headroom and legitimately fruit on
    // its own, confounding the total_spawned count this test relies on.
    for dy in 1..=10i32 {
        for dx in -6..=6i32 {
            let (cx, cy) = ((ox + dx) as usize, (oy - dy) as usize);
            if w.get(cx, cy) == Material::MushroomFlesh {
                w.paint(cx as i32, cy as i32, 0, Material::Empty as u8);
            }
        }
    }
    assert_eq!(w.get(ox as usize, oy as usize), Material::Mycelium, "origin must remain mature mycelium after the carve");
    assert_ne!(
        w.cell_flags(ox as usize, oy as usize) & sandgun_core::cell::FLAG_FRUITED,
        0,
        "origin must still carry FLAG_FRUITED after the carve"
    );

    // Give the origin a fresh colonizable neighbor and re-seed it into the frontier.
    // seed_frontier() only pushes a coordinate reference -- it never touches the cell's
    // aux or flags, so origin re-enters still mature and still FLAG_FRUITED (if present).
    w.paint(nx, oy, 0, Material::Soil as u8);
    w.seed_frontier();
    assert!(w.frontier_len() >= 1, "origin (with its fresh neighbor) should re-enter the frontier");

    for _ in 0..300 {
        w.step();
        let len = w.mushroom_len();
        if len > prev_len {
            total_spawned += len - prev_len;
        }
        prev_len = len;
    }
    assert_eq!(
        total_spawned, 1,
        "a fruited cell with FLAG_FRUITED set must never re-fruit, even with restored room"
    );
}

#[test]
fn mycelium_cell_fruits_at_most_once() {
    // A mycelium cell (origin) with a Soil neighbor to its left and right (isolated everywhere
    // else, and floored so the soil can't avalanche), with open sky above so its own mushroom
    // stem can grow. With P_MATURITY=1 and P_FRUIT_CHANCE=1.0, origin becomes mature on the very
    // first growth tick it's processed while it still has an un-consumed Soil neighbor -- a
    // multi-tick window where the buggy code (no "already fruited" guard) re-rolls fruiting on
    // every subsequent tick it remains in the frontier, stacking a second overlapping mushroom
    // at the exact same (x, y) before it exhausts both neighbors and retires. Each of the two
    // Soil neighbors, once colonized, is itself fully isolated (no other Soil/reachable-Empty
    // neighbor, P_MAX_REACH=0 disables bridging) so it can fruit at most once too -- that's
    // correct, expected behavior, not the bug. So the only way this scenario can ever produce
    // MORE than 3 total mushrooms (origin + its left neighbor + its right neighbor) is if the
    // origin cell fruited more than once -- exactly the bug under test. (Fewer than 3 can happen
    // legitimately: the origin's stem wanders +/-2 cells as it rises -- see try_fruit's
    // sway_seed -- and since the neighbors sit only 1 cell away, the wandering stem can
    // occasionally occupy a neighbor's headroom column before that neighbor matures, correctly
    // denying it fruiting per the headroom rule. That's not the bug either.)
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
    assert!(total_spawned >= 1, "setup: the origin cell itself should fruit at least once");
    assert!(
        total_spawned <= 3,
        "at most 3 mushrooms total (origin + its 2 isolated neighbors), each cell fruiting \
         at most once -- more means the origin cell re-fruited (got {total_spawned})"
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

#[test]
fn burning_mushroom_leaves_sparse_ash() {
    // Regression: burnt-out Mycelium/MushroomFlesh always converted 1:1 to Ash. That's far too
    // much ash for a burned mushroom colony -- it should mostly disappear (Empty), leaving only
    // some ash behind.
    let mut w = World::new(64, 64);
    w.set_param(sandgun_core::params::P_FLAM_FLESH as u32, 1.0); // guarantee ignition on contact
    w.set_param(sandgun_core::params::P_FUEL_FLESH as u32, 1.0); // burn out almost immediately

    let mut flesh_count = 0usize;
    for x in 20..30 {
        for y in 20..30 {
            w.paint(x, y, 0, Material::MushroomFlesh as u8);
            flesh_count += 1;
        }
    }
    w.paint(19, 25, 0, Material::Fire as u8); // ignite the slab from one edge

    for _ in 0..200 {
        w.step();
    }

    let ash = ash_count(&w);
    assert!(ash > 0, "some ash should form");
    assert!(
        (ash as f32) < 0.6 * flesh_count as f32,
        "ash should be sparse, not 1:1 with burnt flesh ({ash} ash / {flesh_count} flesh)"
    );
}

fn ash_count(w: &World) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == Material::Ash {
                n += 1;
            }
        }
    }
    n
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
fn painted_mycelium_grows() {
    // Regression: World::paint just set cells directly, so painted Mycelium never joined the
    // growth frontier -- unlike worldgen/colonized/bridged/reseeded/spore-ammo mycelium, it
    // stayed a permanently inert blob (same class of bug as the spore-ammo case already fixed
    // via seed_frontier_around).
    let mut w = soil_world();
    w.paint(64, 90, 3, Material::Mycelium as u8); // paint a blob of mycelium into the soil band
    assert!(w.frontier_len() > 0, "painting mycelium should seed it into the growth frontier");

    let before = mycelium_count(&w);
    for _ in 0..300 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before, "painted mycelium should actually spread ({before} -> {after})");
}

#[test]
fn painted_mycelium_on_soil_grows() {
    // Painted mycelium adjacent to Soil must join the growth frontier and spread, same as
    // worldgen/colonized/bridged/reseeded/spore-ammo mycelium.
    let mut w = soil_world();
    w.paint(64, 90, 0, Material::Mycelium as u8); // paint a single seed into the soil band
    assert!(w.frontier_len() > 0, "painted mycelium next to soil should join the growth frontier");

    let before = mycelium_count(&w);
    for _ in 0..300 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before, "painted mycelium on soil should spread ({before} -> {after})");
}

#[test]
fn painted_mycelium_in_open_bridges() {
    // Regression: seed_frontier_around (called by World::paint for Mycelium) only enqueued a
    // painted cell via has_colonizable_neighbor (Soil-only neighbors). Mycelium painted where
    // its only neighbors are Empty (open air) never entered the frontier and stayed a
    // permanently inert blob, unlike normal colonization which can bridge into empty space.
    let mut w = World::new(64, 64);
    // A mycelium blob painted entirely in open air: no Soil, no pre-existing mycelium nearby.
    w.paint(32, 32, 2, Material::Mycelium as u8);
    assert!(
        w.frontier_len() > 0,
        "painted mycelium with only empty neighbors should still join the frontier (bridging)"
    );

    let before = mycelium_count(&w);
    for _ in 0..300 {
        w.step();
    }
    let after = mycelium_count(&w);
    assert!(after > before, "painted mycelium in open air should bridge and spread a little ({before} -> {after})");
}

#[test]
fn mushroom_stems_vary_and_stay_connected() {
    // Regression: mushroom stems used to be a perfectly straight column at the mushroom's base
    // x. They should now wander gently (deterministically, seeded per-mushroom) while staying
    // visually connected, and the cap dome must follow the stem's ACTUAL top, not the base x.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8); // floor, wide open headroom above
    }
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MIN as u32, 12.0);
    w.set_param(sandgun_core::params::P_MUSH_HEIGHT_MAX as u32, 12.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MIN as u32, 4.0);
    w.set_param(sandgun_core::params::P_MUSH_CAP_MAX as u32, 4.0);

    // Two mushrooms far enough apart to never interact; try_fruit draws sway_seed from the sim
    // RNG each call, so the two mushrooms get different sway even with identical height/cap.
    w.try_fruit(15, 59);
    w.try_fruit(45, 59);

    for _ in 0..3000 {
        w.step();
        if w.mushroom_len() == 0 {
            break;
        }
    }
    assert_eq!(w.mushroom_len(), 0, "both mushrooms must finish growing");

    let mut all_stems = Vec::new();
    for base_x in [15i32, 45i32] {
        // Pure-stem rows: p = 0..=10 (height=12, so p=11 is the shared stem-top/dome row,
        // checked separately below).
        let mut stem_xs = Vec::new();
        for p in 0..=10i32 {
            let y = 59 - 1 - p;
            let found = (base_x - 4..=base_x + 4)
                .find(|&x| w.get(x as usize, y as usize) == Material::MushroomFlesh);
            stem_xs.push(found.expect("stem cell missing at expected height"));
        }
        // (b) connected: consecutive stem cells differ by at most 1 in x -- no gaps.
        for pair in stem_xs.windows(2) {
            assert!((pair[0] - pair[1]).abs() <= 1, "stem must stay connected: {stem_xs:?}");
        }
        all_stems.push((base_x, stem_xs));
    }

    // (a) at least one stem is NOT perfectly straight (some stem cell x != base x).
    let any_wavy = all_stems.iter().any(|(base_x, xs)| xs.iter().any(|&x| x != *base_x));
    assert!(any_wavy, "at least one stem should wander away from its base x");

    // (c) the cap dome sits atop the stem's ACTUAL top, not the mushroom's base x.
    for (base_x, xs) in &all_stems {
        let cap_top_y = (59 - 12) as usize; // base_y - height = 47
        let dome_xs: Vec<i32> = (base_x - 8..=base_x + 8)
            .filter(|&x| w.get(x as usize, cap_top_y) == Material::MushroomFlesh)
            .collect();
        assert!(!dome_xs.is_empty(), "dome's widest row should be present at the stem top");
        let (dmin, dmax) = (*dome_xs.iter().min().unwrap(), *dome_xs.iter().max().unwrap());
        assert_eq!(dmax - dmin + 1, dome_xs.len() as i32, "dome widest row should be one contiguous run");
        let dome_center = (dmin + dmax) / 2;
        let last_stem_x = *xs.last().unwrap();
        assert!(
            (dome_center - last_stem_x).abs() <= 1,
            "cap dome (centered at {dome_center}) must sit atop the stem's actual top \
             ({last_stem_x}), not just the base x ({base_x})"
        );
    }
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
