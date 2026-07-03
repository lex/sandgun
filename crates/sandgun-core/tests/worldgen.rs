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
    assert!(count(&w, Material::Rock) > total / 5, "at least 20% rock");
    assert!(count(&w, Material::Empty) > total / 5, "at least 20% air");
    assert!(count(&w, Material::Sand) > 50, "sand pockets present");
    assert!(count(&w, Material::Water) > 50, "water pools present");
    assert!(count(&w, Material::Oil) > 50, "oil pockets present");
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
