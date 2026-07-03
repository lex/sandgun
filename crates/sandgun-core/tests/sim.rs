use sandgun_core::cell::Material;
use sandgun_core::world::World;

#[test]
fn world_starts_empty() {
    let w = World::new(64, 64);
    assert_eq!(w.get(0, 0), Material::Empty);
    assert_eq!(w.get(63, 63), Material::Empty);
}

#[test]
fn paint_sets_a_single_cell_at_radius_zero() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Sand as u8);
    assert_eq!(w.get(10, 10), Material::Sand);
    assert_eq!(w.get(11, 10), Material::Empty);
}

#[test]
fn paint_circle_and_out_of_bounds_is_safe() {
    let mut w = World::new(64, 64);
    w.paint(0, 0, 5, Material::Water as u8); // clips off all four... two edges, must not panic
    assert_eq!(w.get(0, 0), Material::Water);
    assert_eq!(w.get(3, 0), Material::Water);
    assert_eq!(w.get(10, 10), Material::Empty);
}

#[test]
fn sand_falls_one_cell_per_step() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Sand as u8);
    w.step();
    assert_eq!(w.get(10, 10), Material::Empty);
    assert_eq!(w.get(10, 11), Material::Sand);
    w.step();
    assert_eq!(w.get(10, 12), Material::Sand);
}

#[test]
fn sand_settles_flat_on_the_floor() {
    // Classic loose-sand rule (user decision 2026-07-03): a grain blocked below
    // always tries the diagonals, so a lone 2-stack collapses flat on the floor.
    let mut w = World::new(64, 64);
    w.paint(10, 62, 0, Material::Sand as u8);
    w.paint(10, 60, 0, Material::Sand as u8);
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.get(10, 63), Material::Sand, "first grain rests on the floor");
    assert!(
        w.get(9, 63) == Material::Sand || w.get(11, 63) == Material::Sand,
        "second grain topples to a floor neighbor"
    );
    assert_eq!(w.get(10, 62), Material::Empty, "no 2-stack survives on open floor");
}

#[test]
fn sand_slides_off_a_peak_diagonally() {
    let mut w = World::new(64, 64);
    // three stacked on the floor: the top one must topple left or right
    w.paint(32, 63, 0, Material::Sand as u8);
    w.paint(32, 62, 0, Material::Sand as u8);
    w.paint(32, 61, 0, Material::Sand as u8);
    for _ in 0..10 {
        w.step();
    }
    assert_eq!(w.get(32, 61), Material::Empty);
    assert!(
        w.get(31, 63) == Material::Sand || w.get(33, 63) == Material::Sand,
        "top grain should have toppled to a floor neighbor"
    );
}

#[test]
fn rock_never_moves() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Rock as u8);
    for _ in 0..5 {
        w.step();
    }
    assert_eq!(w.get(10, 10), Material::Rock);
    assert_eq!(w.get(10, 11), Material::Empty);
}
