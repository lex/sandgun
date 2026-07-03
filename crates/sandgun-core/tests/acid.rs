use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn acid_eats_through_a_sand_shelf() {
    let mut w = World::new(64, 64);
    for x in 20..=40 {
        for y in 40..=43 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    w.paint(30, 38, 1, Material::Acid as u8);
    for _ in 0..1200 {
        w.step();
    }
    let hole = (40..=43).any(|y| w.get(30, y) == Material::Empty)
        || (44..64).any(|y| w.get(30, y) == Material::Acid);
    assert!(hole, "acid must corrode into or through the shelf");
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
