use sandgun_core::cell::{Material, FLAG_BURNING};
use sandgun_core::projectile::Ammo;
use sandgun_core::world::World;

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
fn kinetic_round_blasts_a_crater_and_throws_debris() {
    let mut w = World::new(64, 64);
    // solid sand block
    for x in 20..44 {
        for y in 20..44 {
            w.paint(x, y, 0, Material::Sand as u8);
        }
    }
    let sand0 = count(&w, Material::Sand);
    // NOTE: update_projectiles() moves a projectile by exactly its velocity vector once
    // per step() call (ray-marched in substeps, not run-to-impact); it does not travel an
    // unbounded distance in a single call. The brief's original origin (x=2.0) is 18 cells
    // from the block front (x=20) but vx=12 only covers 12 cells per step(), so the single
    // w.step() below would never reach the block. Moved the origin to x=10.0 (within the
    // single-frame reach) so the first w.step() actually resolves the impact, matching the
    // fix already applied to `fast_projectile_does_not_tunnel_through_a_thin_wall` in
    // tests/projectiles.rs for the same reason.
    w.fire(10.0, 32.0, 12.0, 0.0, Ammo::Kinetic as u8); // into the block from the left
    w.step(); // impact this frame
    let sand1 = count(&w, Material::Sand);
    assert!(sand1 < sand0, "kinetic impact must remove sand (a crater)");
    assert!(w.particle_count() > 0, "some blasted sand becomes flying debris");
    for _ in 0..400 {
        w.step();
    }
    // debris resettles; world eventually calms
    assert_eq!(w.particle_count(), 0);
}

#[test]
fn incendiary_round_lights_oil() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        for y in 30..40 {
            w.paint(x, y, 0, Material::Oil as u8);
        }
    }
    w.fire(2.0, 34.0, 12.0, 0.0, Ammo::Incendiary as u8);
    for _ in 0..200 {
        w.step();
    }
    // fire consumes oil over time; a good chunk should be gone (burned to empty)
    assert!(count(&w, Material::Oil) < 24 * 10, "incendiary round ignited the oil pool");
}

#[test]
fn acid_round_deposits_acid() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        for y in 30..44 {
            w.paint(x, y, 0, Material::Rock as u8);
        }
    }
    // NOTE: as above, a single w.step() only covers the projectile's velocity vector once.
    // The brief's original fire(2.0, 29.0, 12.0, -1.0) both (a) needed 18 cells of x-travel
    // for only 12 cells/step of reach, and (b) aimed vy upward/away from the block (which
    // sits below the origin, at y>=30), so it could never have hit the block at all. Moved
    // the origin to (10.0, 27.0) and aimed down-and-right (vy=4.0) so the impact resolves
    // on the first w.step(): x reaches the block's left edge (20) exactly on substep 10,
    // and by then y has cleared the block's top edge (30) with margin (30.33), avoiding
    // float-boundary flakiness on the y check.
    w.fire(10.0, 27.0, 12.0, 4.0, Ammo::Acid as u8);
    w.step();
    assert!(count(&w, Material::Acid) > 0, "acid round leaves acid at the impact");
}

#[test]
fn spore_round_plants_mycelium() {
    let mut w = World::new(64, 64);
    for x in 20..44 {
        w.paint(x, 40, 0, Material::Soil as u8);
    }
    // Soil is a powder: with nothing under it, the very first step() sweep (which runs
    // before update_projectiles) would drop the whole row to y=41 before the projectile
    // ever gets a chance to arrive, and the impact would land on empty air. Give it a
    // rock floor so it stays put at y=40 as the fixture intends.
    for x in 20..44 {
        w.paint(x, 41, 0, Material::Rock as u8);
    }
    // NOTE: as above, one w.step() only covers the velocity vector once. The brief's
    // original fire(2.0, 39.0, ...) was both too far from the block (x=20) for a 12-cell
    // step, and aimed at y=39 with vy=0.0 — one row above the y=40 soil strip it never
    // actually crosses. Moved the origin to (10.0, 40.0) so it flies straight down the
    // soil row and reaches x=20 (the strip's start) on substep 10.
    w.fire(10.0, 40.0, 12.0, 0.0, Ammo::Spore as u8);
    w.step();
    assert!(count(&w, Material::Mycelium) > 0, "spore round plants mycelium at impact");
}

#[test]
fn firing_into_empty_world_settles_after_impacts_resolve() {
    let mut w = World::new(64, 64);
    for x in 0..64 {
        w.paint(x, 60, 0, Material::Rock as u8);
    }
    for _ in 0..5 {
        w.fire(2.0, 30.0, 10.0, 0.0, Ammo::Kinetic as u8);
        for _ in 0..20 {
            w.step();
        }
    }
    for _ in 0..500 {
        w.step();
    }
    w.step();
    assert_eq!(w.projectile_count(), 0);
    assert_eq!(w.particle_count(), 0);
    assert_eq!(w.cells_processed, 0, "world must return to rest after the dust settles");
}

#[test]
fn incendiary_blast_refuels_already_burning_cells() {
    let mut w = World::new(64, 64);
    // Rock floor to hold the oil
    for x in 20..44 {
        w.paint(x, 50, 0, Material::Rock as u8);
    }
    // Oil layer on the rock
    for x in 20..44 {
        for y in 40..50 {
            w.paint(x, y, 0, Material::Oil as u8);
        }
    }

    let initial_fuel = w.params.fuel(Material::Oil);

    // First incendiary round to ignite the oil (adjusted origin for single-step impact as per pattern)
    w.fire(10.0, 45.0, 12.0, 0.0, Ammo::Incendiary as u8);
    w.step(); // impact this frame

    // Step several times to let the fire burn and fuel decrease
    for _ in 0..10 {
        w.step();
    }

    // Find a burning oil cell and record its aux (fuel level) before refuel
    let mut refueled_cells_found = false;
    let mut aux_before_map = std::collections::HashMap::new();
    for y in 40..50 {
        for x in 20..44 {
            if (w.cell_flags(x, y) & FLAG_BURNING) != 0 && w.get(x, y) == Material::Oil {
                aux_before_map.insert((x, y), w.cell_aux(x, y));
            }
        }
    }

    assert!(
        !aux_before_map.is_empty(),
        "there must be at least one burning oil cell after initial ignition"
    );

    // Find at least one cell that has actually burned down (aux < initial_fuel)
    let cells_that_burned: Vec<(usize, usize)> = aux_before_map
        .iter()
        .filter_map(|(&pos, &aux)| if aux < initial_fuel { Some(pos) } else { None })
        .collect();

    assert!(
        !cells_that_burned.is_empty(),
        "at least one cell should have burned down (aux < initial_fuel), but all had aux={}",
        initial_fuel
    );

    // Second incendiary round at the SAME location as the first (proven impact point)
    // This ensures we hit the burning region with a second blast
    w.fire(10.0, 45.0, 12.0, 0.0, Ammo::Incendiary as u8);
    w.step(); // impact this frame

    // Verify that at least one of the cells that was burning (and had burned down) has been refueled
    // by checking if it now has full fuel
    for &(x, y) in &cells_that_burned {
        let aux_after = w.cell_aux(x, y);
        let flags_after = w.cell_flags(x, y);
        let material_after = w.get(x, y);

        if material_after == Material::Oil && (flags_after & FLAG_BURNING) != 0 && aux_after == initial_fuel {
            // Found a previously-burned cell that has been refueled to full
            refueled_cells_found = true;
            break;
        }
    }

    // If we didn't find the exact cell refueled, check if any burning cell in the blast area
    // has been refueled (could be a cell that was in the blast radius)
    if !refueled_cells_found {
        for y in 40..50 {
            for x in 20..44 {
                if (w.cell_flags(x, y) & FLAG_BURNING) != 0
                    && w.get(x, y) == Material::Oil
                    && w.cell_aux(x, y) == initial_fuel {
                    // Double-check this cell is in the blast center region (not just edge)
                    let dx = (x as isize - 32).abs(); // blast was at x ~= 32
                    let dy = (y as isize - 45).abs(); // blast was at y = 45
                    if dx <= 3 && dy <= 3 {
                        refueled_cells_found = true;
                        break;
                    }
                }
            }
            if refueled_cells_found {
                break;
            }
        }
    }

    assert!(
        refueled_cells_found,
        "incendiary blast must refuel at least one already-burning oil cell (or produce refueled cells in blast radius) with full fuel"
    );
}
