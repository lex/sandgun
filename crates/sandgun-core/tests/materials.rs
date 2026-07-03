use sandgun_core::cell::{Cell, Material, FLAG_BURNING};
use sandgun_core::params::{Params, P_FLAM_MYCELIUM};
use sandgun_core::world::World;

#[test]
fn material_ids_roundtrip() {
    for id in 0..=12u8 {
        assert_eq!(Material::from_u8(id) as u8, id, "id {id} must roundtrip");
    }
    assert_eq!(Material::from_u8(200), Material::Empty);
}

#[test]
fn material_classes_are_disjoint() {
    for id in 0..=12u8 {
        let m = Material::from_u8(id);
        let classes = [m.is_liquid(), m.is_powder(), m.is_gas(), m.is_solid()];
        assert!(classes.iter().filter(|&&c| c).count() <= 1, "{m:?} is in multiple classes");
    }
    assert!(Material::Soil.is_powder());
    assert!(Material::Ash.is_powder());
    assert!(Material::Acid.is_liquid());
    assert!(Material::SporeGas.is_gas());
    assert!(Material::Smoke.is_gas());
    assert!(Material::Mycelium.is_solid());
    assert!(Material::MushroomFlesh.is_solid());
}

#[test]
fn cell_new_sets_initial_aux() {
    assert_eq!(Cell::new(Material::Fire, 0).aux, 40);
    assert_eq!(Cell::new(Material::Acid, 0).aux, 10);
    assert_eq!(Cell::new(Material::Sand, 0).aux, 0);
    assert_eq!(Cell::new(Material::Fire, 0).flags & FLAG_BURNING, 0);
}

#[test]
fn params_default_and_lookup() {
    let p = Params::default();
    assert!(p.flammability(Material::SporeGas) >= 1.0);
    assert!(p.flammability(Material::Rock) == 0.0);
    assert!(p.fuel(Material::Mycelium) > 0);
    assert!(p.values[P_FLAM_MYCELIUM] > 0.0);
}

#[test]
fn painted_acid_gets_charges() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Acid as u8);
    // charges live in aux; verified indirectly: world exposes get() only, so assert via
    // a step not consuming it in isolation — the cell must still be acid after one step.
    w.step();
    let acid_somewhere = (0..64).any(|x| (0..64).any(|y| w.get(x, y) == Material::Acid));
    assert!(acid_somewhere, "fresh acid must not instantly vanish");
}
