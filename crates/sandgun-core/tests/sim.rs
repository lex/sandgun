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
