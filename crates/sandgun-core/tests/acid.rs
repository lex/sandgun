use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn acid_eats_through_a_sand_plug() {
    // Amended: the original floating shelf fell by gravity, passing vacuously.
    // This plug rests on the world floor between rock walls — only acid can shrink it.
    let mut w = World::new(64, 64);
    for y in 56..64 {
        w.paint(28, y, 0, Material::Rock as u8);
        w.paint(36, y, 0, Material::Rock as u8);
    }
    for x in 29..36 {
        for y in 58..64 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    let initial: usize = (29..36).map(|x| (58..64).filter(|&y| w.get(x, y) == Material::Sand).count()).sum();
    assert_eq!(initial, 42);
    w.paint(32, 56, 1, Material::Acid as u8);
    for _ in 0..1500 {
        w.step();
    }
    let sand: usize = (0..64).map(|x| (0..64).filter(|&y| w.get(x, y) == Material::Sand).count()).sum();
    assert!(sand < 35, "acid must corrode well into the plug ({sand}/42 cells left)");
}

#[test]
fn acid_spends_its_charges_and_vanishes() {
    // NOTE (amended after implementation feedback): acid that tunnels free and ends up
    // isolated correctly RESTS as an inert puddle with unspent charges — charges bound
    // activity, not existence. To test full spend, the acid must never run out of food:
    // a deep full-width sand bed.
    let mut w = World::new(64, 64);
    for x in 0..64 {
        for y in 40..64 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    w.paint(32, 38, 1, Material::Acid as u8);
    for _ in 0..3000 {
        w.step();
    }
    let acid_left = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Acid));
    assert!(!acid_left, "every acid cell must spend its charges into the sand bed");
    for _ in 0..300 {
        w.step();
    }
    w.step();
    assert_eq!(w.cells_processed, 0, "world must settle after the acid is spent");
}

#[test]
fn acid_barely_touches_rock() {
    let mut w = World::new(64, 64);
    for x in 20..=40 {
        w.paint(x, 40, 0, Material::Rock as u8);
    }
    // walls so it can't slide off the shelf
    w.paint(29, 39, 0, Material::Rock as u8);
    w.paint(31, 39, 0, Material::Rock as u8);
    w.paint(30, 39, 0, Material::Acid as u8);
    for _ in 0..200 {
        w.step();
    }
    // one acid cell at rock etch-rate can nibble at most a couple of cells in 200 ticks
    let rocks: usize = (20..=40).filter(|&x| w.get(x, 40) == Material::Rock).count();
    assert!(rocks >= 19, "rock must mostly survive brief acid contact ({rocks}/21 left)");
}
