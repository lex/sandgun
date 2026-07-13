use sandgun_core::cell::Material;
use sandgun_core::world::World;
use sandgun_core::worldgen;

fn count(w: &World, m: Material) -> usize {
    let mut n = 0;
    for y in 0..w.height {
        for x in 0..w.width {
            if w.get(x, y) == m {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn generation_is_deterministic() {
    let mut a = World::new(256, 192);
    let mut b = World::new(256, 192);
    worldgen::generate(&mut a, 42);
    worldgen::generate(&mut b, 42);
    for y in 0..a.height {
        for x in 0..a.width {
            assert_eq!(a.get(x, y), b.get(x, y), "mismatch at ({x},{y})");
        }
    }
}

#[test]
fn different_seeds_differ() {
    let mut a = World::new(256, 192);
    let mut b = World::new(256, 192);
    worldgen::generate(&mut a, 1);
    worldgen::generate(&mut b, 2);
    let mut diff = 0;
    for y in 0..a.height {
        for x in 0..a.width {
            if a.get(x, y) != b.get(x, y) {
                diff += 1;
            }
        }
    }
    assert!(diff > 100, "seeds 1 and 2 produced nearly identical worlds ({diff} cells differ)");
}

#[test]
fn generated_world_has_terrain_air_and_materials() {
    let mut w = World::new(256, 192);
    worldgen::generate(&mut w, 7);
    let total = w.width * w.height;
    // Worldgen task 1 (2026-07-12): replaced the old uniform-rock-then-global-soil-CA fill
    // (M1e task 6, which made Soil a deliberate 3x-over-Rock majority everywhere) with
    // depth-graded biome bands -- soil-rich near the surface, rock-dominant deep (see
    // biomes() in worldgen.rs). Soil is no longer a world-wide majority over Rock by design
    // (the "soil > rock*3" assertion is gone), but it's still a substantial fraction overall
    // and some structural rock must remain.
    assert!(count(&w, Material::Rock) > 200, "some structural rock should remain");
    assert!(count(&w, Material::Soil) > total / 10, "soil should be a substantial fraction of the world");
    assert!(count(&w, Material::Empty) > total / 5, "at least 20% air");
    assert!(count(&w, Material::Sand) > 50, "sand pockets present");
    // Worldgen task 4 (2026-07-13): the cave layer became structured chambers + winding tunnels
    // (Noita-style) instead of the old uniform-density CA. That killed the salt-and-pepper look but
    // also means far fewer FLAT basins for liquid to pool in, so pool counts are lower -- especially
    // at this small 256x192 test size. Liquids are still placed (and are plentiful at the game's
    // 1024x2048 scale: ~248 water / ~207 oil / ~83 acid); these assertions verify pools are PRESENT
    // rather than pinning the old generator's density. Measured min across seeds 1..=20 here: water 6,
    // oil 7 -- the >3 floor holds with margin while still failing if pool placement breaks entirely.
    assert!(count(&w, Material::Water) > 3, "water pools present");
    assert!(count(&w, Material::Oil) > 3, "oil pockets present");
    assert!(count(&w, Material::SporeGas) > 40, "spore pockets present");
    // Mycelium/MushroomFlesh are no longer pre-grown by worldgen (see
    // generated_world_has_living_colonies below) -- the world seeds living colony origins
    // instead and grows its own mycelium/mushrooms over time via the M1e organism model.
}

#[test]
fn generated_world_has_living_colonies() {
    // M1e task 6: worldgen no longer pre-fills mycelium veins/mushroom groves -- it seeds
    // P_MY_WORLDGEN_COLONIES living colony origins on Soil instead, and the world grows its own
    // mycelium outward from them via the organism model (mycelium.rs).
    let mut w = World::new(256, 192);
    worldgen::generate(&mut w, 7);
    assert!(w.colony_count() > 0, "worldgen should seed living colonies");

    let before = count(&w, Material::Mycelium);
    for _ in 0..500 {
        w.step();
    }
    let after = count(&w, Material::Mycelium);
    assert!(
        after > before,
        "seeded colonies should actually grow into the rebalanced world's soil ({before} -> {after})"
    );
}

#[test]
fn generated_world_wakes_and_regenerates_cleanly() {
    let mut w = World::new(256, 192);
    worldgen::generate(&mut w, 7);
    w.step();
    assert!(w.cells_processed > 0, "generate must wake the world");
    worldgen::generate(&mut w, 8); // regen over a dirty world must not carry old cells
    assert!(count(&w, Material::Rock) > 0);
}

#[test]
fn generated_world_fully_settles() {
    // AKA "generated_world_settles": with M1e, colonies grow live from their worldgen origins
    // rather than starting pre-grown, so settling takes many more steps than the old
    // static-content world did -- they must grow, eat, branch, fruit, and (once local soil runs
    // out) recede before the world goes fully quiet. Verified empirically (via an instrumented
    // run) to settle by ~step 4000 for this seed and STAY settled; 40000 gives generous margin
    // without weakening the assertion below -- it's still a hard requirement that
    // cells_processed reaches exactly zero. No early-break on a transient zero: growth ticks are
    // sparse (P_MY_GROWTH_INTERVAL), so cells_processed can read 0 on an in-between frame while
    // the colonies are still actively growing/receding.
    let mut w = World::new(256, 192);
    worldgen::generate(&mut w, 7);
    for _ in 0..40_000 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "an untouched generated world must fully settle");
}

// M1e playtest fix 5 added a `worldgen_has_no_lonely_rock_specks` test (+ helper) enforcing a
// zero-tolerance declump pass over the whole final world. Worldgen task 1 (2026-07-12)
// intentionally removes that global declump pass -- the brief for this task replaces it with
// depth-graded biome bands + a local 1-2 iteration majority-smoothing CA over the fill, which
// makes bands read as cohesive masses but does not guarantee zero fully-isolated single-cell
// Rock specks (especially where the still-present cave carve, pass 3, can strand a cell by
// carving out all of its neighbors). That's an accepted, deliberate trade-off for this task --
// Task 3's set-pieces may clean up residual specks locally (e.g. soil beds), but a global
// zero-specks guarantee is no longer a worldgen invariant. Test removed rather than weakened
// since there's no meaningful non-zero threshold specified by the new design.

// --- worldgen task 1: biome bands + depth-graded base fill ---

#[test]
fn world_has_open_sky_above_the_surface() {
    let mut w = World::new(256, 512);
    worldgen::generate(&mut w, 1);
    // the very top row is open (above the surface heightline)
    let top_empty = (0..w.width).filter(|&x| w.get(x, 0) == Material::Empty).count();
    assert!(top_empty > w.width * 3 / 4, "top row should be mostly open sky");
}

#[test]
fn deeper_band_is_rockier_than_the_upper_band() {
    // soil should dominate near the surface, rock deeper -- a depth gradient, not uniform fill.
    let mut w = World::new(256, 512);
    worldgen::generate(&mut w, 2);
    let band = w.height / 4;
    let upper_rows = band..band * 2; // upper subsurface band
    let deep_rows = band * 3..band * 4; // deep band
    let ratio = |rows: std::ops::Range<usize>| {
        let (mut soil, mut rock) = (0usize, 0usize);
        for y in rows {
            for x in 0..w.width {
                match w.get(x, y) {
                    Material::Soil => soil += 1,
                    Material::Rock => rock += 1,
                    _ => {}
                }
            }
        }
        (soil, rock)
    };
    let (us, ur) = ratio(upper_rows);
    let (ds, dr) = ratio(deep_rows);
    // upper band is soil-leaning, deep band is rock-leaning
    assert!(
        us as f32 / (ur.max(1) as f32) > ds as f32 / (dr.max(1) as f32),
        "soil:rock ratio must decrease with depth (upper {us}:{ur} vs deep {ds}:{dr})"
    );
}

// --- worldgen task 2: connected caverns + guaranteed descendable path ---

// A path of Empty cells connects the top surface to the bottom of the world (you can descend).
fn descendable(w: &World) -> bool {
    use std::collections::VecDeque;
    let (wd, ht) = (w.width, w.height);
    let mut seen = vec![false; wd * ht];
    let mut q = VecDeque::new();
    // seed from every Empty cell in the top row (open sky)
    for x in 0..wd {
        if w.get(x, 0) == Material::Empty {
            seen[x] = true;
            q.push_back((x, 0));
        }
    }
    while let Some((x, y)) = q.pop_front() {
        if y == ht - 1 {
            return true; // reached the bottom
        }
        for (nx, ny) in [
            (x as i32 - 1, y as i32),
            (x as i32 + 1, y as i32),
            (x as i32, y as i32 - 1),
            (x as i32, y as i32 + 1),
        ] {
            if nx < 0 || ny < 0 || nx as usize >= wd || ny as usize >= ht {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            if !seen[ny * wd + nx] && w.get(nx, ny) == Material::Empty {
                seen[ny * wd + nx] = true;
                q.push_back((nx, ny));
            }
        }
    }
    false
}

#[test]
fn every_generated_world_is_descendable() {
    for seed in 1..=12u32 {
        let mut w = World::new(256, 512);
        worldgen::generate(&mut w, seed);
        assert!(descendable(&w), "seed {seed} produced a non-descendable world");
    }
}

// The connectivity guarantee (ensure_descendable) is size-independent, but the app runs at the
// full 1024x2048 vertical descent -- verify the top->bottom Empty path actually holds at that
// real size for a spread of seeds (worldgen task 4 acceptance).
#[test]
fn full_size_world_is_descendable() {
    for seed in [1u32, 3, 7, 42, 100] {
        let mut w = World::new(1024, 2048);
        worldgen::generate(&mut w, seed);
        assert!(descendable(&w), "full-size seed {seed} is not descendable");
    }
}

// --- worldgen task 3: set-pieces (soil beds, basin liquid pools, pockets) ---

#[test]
fn world_has_soil_beds_and_liquids_and_stays_descendable() {
    let mut w = World::new(256, 512);
    worldgen::generate(&mut w, 3);
    assert!(count(&w, Material::Soil) > 500, "should have substantial soil beds");
    let liquid = count(&w, Material::Water) + count(&w, Material::Oil) + count(&w, Material::Acid);
    assert!(liquid > 0, "should place some liquid pools");
    assert!(descendable(&w), "features must not seal the descent"); // reuse the Task 2 helper
}

#[test]
fn poured_liquids_rest_on_supported_floor() {
    // Worldgen task 3 review: pools/beds must sit on STABLE support, not on Soil that is itself
    // floating over Empty. Soil is a powder, so a liquid poured onto a Soil floor whose own
    // sub-cell is Empty would drop on the first sim step -- contradicting "poured liquid is already
    // at rest." The floor/wall tests now use is_support (Rock, or Soil held up by a non-Empty cell
    // below) instead of is_terrain. Assert the invariant the fix restores: no liquid cell rests on
    // a Soil cell that has Empty directly beneath it. (Seeds 15/20/21/25/27 were the reviewer's
    // reproducers; the descendability shaft carve after placement can still leave a liquid cell
    // over bare Empty by draining a pool -- that's a separate, accepted case, so this asserts the
    // specific powder-over-empty floor the pool/bed placement itself must never create.)
    for seed in 1..=30u32 {
        let mut w = World::new(256, 512);
        worldgen::generate(&mut w, seed);
        for y in 0..w.height.saturating_sub(2) {
            for x in 0..w.width {
                if w.get(x, y).is_liquid()
                    && w.get(x, y + 1) == Material::Soil
                    && w.get(x, y + 2) == Material::Empty
                {
                    panic!("seed {seed}: liquid at ({x},{y}) rests on Soil-over-Empty (floating floor)");
                }
            }
        }
    }
}

#[test]
fn colonies_are_seeded_on_soil() {
    let mut w = World::new(256, 512);
    worldgen::generate(&mut w, 4);
    assert!(w.colony_count() > 0, "worldgen should seed colonies");
}

