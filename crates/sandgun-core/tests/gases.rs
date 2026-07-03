use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn spore_gas_rises_and_pools_at_ceiling() {
    let mut w = World::new(64, 64);
    // sealed box: ceiling at y=20, walls x=20/x=30, floor y=30
    for x in 20..=30 {
        w.paint(x, 20, 0, Material::Rock as u8);
        w.paint(x, 30, 0, Material::Rock as u8);
    }
    for y in 20..=30 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(30, y, 0, Material::Rock as u8);
    }
    w.paint(25, 28, 1, Material::SporeGas as u8); // low in the box
    for _ in 0..300 {
        w.step();
    }
    assert_eq!(w.get(25, 28), Material::Empty, "gas must leave the floor area");
    let at_ceiling = (21..30).any(|x| w.get(x, 21) == Material::SporeGas);
    assert!(at_ceiling, "gas must pool under the ceiling");
    w.step();
    assert_eq!(w.cells_processed, 0, "pooled gas must come to rest");
}

#[test]
fn smoke_fades_away_and_world_settles() {
    let mut w = World::new(64, 64);
    w.paint(32, 40, 2, Material::Smoke as u8);
    for _ in 0..400 {
        w.step();
    }
    let any_smoke = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Smoke));
    assert!(!any_smoke, "smoke must fully dissipate");
    w.step();
    assert_eq!(w.cells_processed, 0);
}
