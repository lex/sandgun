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

#[test]
fn water_falls_then_spreads_sideways() {
    let mut w = World::new(64, 64);
    w.paint(32, 60, 0, Material::Water as u8);
    w.step(); // falls
    for _ in 0..3 {
        w.step(); // reaches floor, disperses
    }
    assert_eq!(w.get(32, 60), Material::Empty);
    let spread = (0..64).any(|x| x != 32 && w.get(x, 63) == Material::Water);
    assert!(spread, "water on the floor must move horizontally");
}

#[test]
fn water_sinks_below_oil() {
    let mut w = World::new(64, 64);
    // rock walls stop the oil dispersing sideways before the swap is tested
    for y in 61..=63 {
        w.paint(9, y, 0, Material::Rock as u8);
        w.paint(11, y, 0, Material::Rock as u8);
    }
    w.paint(10, 63, 0, Material::Oil as u8);
    w.paint(10, 62, 0, Material::Water as u8);
    w.step();
    assert_eq!(w.get(10, 63), Material::Water, "denser water sinks");
    assert_eq!(w.get(10, 62), Material::Oil, "oil floats up");
}

#[test]
fn sand_sinks_through_water() {
    let mut w = World::new(64, 64);
    // walled column so the water can't slide out from under the sand
    for y in 61..=63 {
        w.paint(9, y, 0, Material::Rock as u8);
        w.paint(11, y, 0, Material::Rock as u8);
    }
    w.paint(10, 63, 0, Material::Water as u8);
    w.paint(10, 61, 0, Material::Sand as u8);
    for _ in 0..5 {
        w.step();
    }
    assert_eq!(w.get(10, 63), Material::Sand, "sand displaces the water");
    assert_eq!(w.get(10, 62), Material::Water, "water is pushed up, not deleted");
}

#[test]
fn liquids_do_not_pass_through_rock() {
    let mut w = World::new(64, 64);
    // rock cup: floor at y=50 spanning x=20..=24, walls at x=20 and x=24
    for x in 20..=24 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    for y in 45..50 {
        w.paint(20, y, 0, Material::Rock as u8);
        w.paint(24, y, 0, Material::Rock as u8);
    }
    w.paint(22, 47, 0, Material::Water as u8);
    for _ in 0..30 {
        w.step();
    }
    let inside = (21..=23).any(|x| (45..=49).any(|y| w.get(x, y) == Material::Water));
    assert!(inside, "water must still be inside the rock cup");
}
