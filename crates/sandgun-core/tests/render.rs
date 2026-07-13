use sandgun_core::cell::Material;
use sandgun_core::world::World;

// Alpha channel of the rendered RGBA carries the material id so the web lighting shader knows each
// cell's emission/opacity. Burning cells (and Fire) report a synthetic FLAME code = 13.
const FLAME: u8 = 13;

fn alpha_at(w: &World, x: usize, y: usize) -> u8 {
    let rgba = w.rgba();
    rgba[(y * w.width + x) * 4 + 3]
}

#[test]
fn alpha_encodes_material_id() {
    let mut w = World::new(64, 64);
    w.paint(10, 10, 0, Material::Rock as u8);
    w.paint(12, 10, 0, Material::Soil as u8);
    w.paint(14, 10, 0, Material::MushroomFlesh as u8);
    w.mark_all_render_dirty();
    w.render_rgba();
    assert_eq!(alpha_at(&w, 10, 10), Material::Rock as u8);
    assert_eq!(alpha_at(&w, 12, 10), Material::Soil as u8);
    assert_eq!(alpha_at(&w, 14, 10), Material::MushroomFlesh as u8);
    // an untouched empty cell reports Empty(0)
    assert_eq!(alpha_at(&w, 30, 30), Material::Empty as u8);
}

#[test]
fn burning_cell_reports_flame_code() {
    let mut w = World::new(64, 64);
    // paint oil then ignite it; a burning cell must report FLAME regardless of its base material
    w.paint(20, 20, 0, Material::Oil as u8);
    w.paint(20, 20, 0, Material::Fire as u8); // Fire itself
    w.mark_all_render_dirty();
    w.render_rgba();
    assert_eq!(alpha_at(&w, 20, 20), FLAME);
}
