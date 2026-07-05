use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn soil_richness_roundtrips() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Soil as u8);
    w.set_soil_richness(10, 10, 100);
    assert_eq!(w.soil_richness(10, 10), 100);
}

#[test]
fn spawn_colony_makes_a_colony_with_one_tip() {
    let mut w = World::new(64, 64);
    let id = w.spawn_colony(32, 32);
    assert_eq!(w.get(32, 32), Material::Mycelium);
    assert_eq!(w.colony_count(), 1);
    assert_eq!(w.tip_count(), 1);
    assert!(id >= 1);
}

#[test]
fn no_tips_means_grow_is_noop_and_world_sleeps() {
    let mut w = World::new(128, 128);
    // some settled sand, no colonies
    for x in 0..128 { w.paint(x, 100, 0, Material::Rock as u8); }
    for _ in 0..50 { w.step(); }
    w.step();
    assert_eq!(w.cells_processed, 0);
    assert_eq!(w.tip_count(), 0);
}
