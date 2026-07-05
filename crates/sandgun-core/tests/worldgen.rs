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
    // M1e task 6 rebalance: Lex playtest feedback was that the old worldgen was too rock-heavy
    // for mycelium to have anywhere to grow. The subsurface is now predominantly Soil, with Rock
    // kept only as a structural minority (crust/cave walls + leftover chunky pockets from the
    // soilification pass in worldgen::generate) -- the inverse of the old "mostly rock, thin soil
    // crust" balance, hence the flipped assertions below relative to pre-M1e.
    assert!(count(&w, Material::Rock) > 200, "some structural rock should remain");
    assert!(count(&w, Material::Soil) > total / 10, "soil should be a substantial fraction of the world");
    assert!(
        count(&w, Material::Soil) > count(&w, Material::Rock) * 3,
        "soil must be a clear majority over rock (soil={}, rock={})",
        count(&w, Material::Soil),
        count(&w, Material::Rock)
    );
    assert!(count(&w, Material::Empty) > total / 5, "at least 20% air");
    assert!(count(&w, Material::Sand) > 50, "sand pockets present");
    assert!(count(&w, Material::Water) > 50, "water pools present");
    assert!(count(&w, Material::Oil) > 50, "oil pockets present");
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
