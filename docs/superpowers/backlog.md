# SANDGUN — parked ideas backlog

Ideas raised mid-work to revisit deliberately later (not lost, not yet scoped).

## Air / wind layer + omnidirectional gas drift (raised by Lex, 2026-07-04)

**Observation:** SporeGas (and smoke) currently only rises straight up. Gases should
disperse in all directions, and the world would feel more alive with an actual
**wind / air-pressure simulation** driving them (and dust, fire lean, spore travel).

**Lex's instinct (agreed):** don't run air in the same cell grid as the material sim.
A pressure/velocity field wants to be its own **coarser, lower-resolution grid** (e.g.
one air cell per 8×8 or 16×16 material chunk) holding a 2D velocity + pressure vector,
advected cheaply, then sampled by gas/particle cells to bias their movement. Reasons:
- Air is continuous and low-frequency; per-material-cell air would be huge and pointless.
- Keeps the material grid's 4-byte cell untouched (no room for a velocity vector anyway).
- A coarse field can update on its own budget/cadence, and gases just read a lerp'd
  wind vector at their position to pick drift direction — cheap per gas cell.

**Open questions for a future grilling:**
- Is this M1-era or a later milestone? (Leaning later — it's a cross-cutting feature.)
- Chunk-sleep interaction: a global air field that always updates would fight the sacred
  sleeping-world optimization. Does air sleep too (only simulate near active gas/fire)?
- Does wind affect only gases/particles, or also fire spread lean and powder drift?
- Player-facing: is wind ambient flavor, or a mechanic (blow your own spore clouds, fire
  spreads downwind into oil)? The latter makes it gun-relevant.
- Boundary conditions in a bounded world; do craters/tunnels channel airflow?

**Cheap interim (no air sim):** just make SporeGas/Smoke disperse omnidirectionally
(up-biased random walk → symmetric random walk with slight buoyancy) so gases spread
sideways too. Delivers most of the visual payoff of "flies in all directions" without
the field. Could slot into M1c's puff/reseed task or a tiny follow-up.

## Fire vs maturity aux collision (raised in M1c T3 review, 2026-07-04)

Mycelium's `aux` byte holds its maturity clock (M1c) but is ALSO overwritten with fire
fuel when it ignites (`ignite_blast`/`ignite_neighbors`). A cell aging toward fruiting
that catches fire and is then extinguished loses its maturity progress. Not a bug —
arguably good gameplay (fire sets back regrowth) — but flagged so it's a deliberate
decision, not an accident. Revisit if fruiting-after-fire feels wrong in playtest.

## Seed the live sim RNG from the world seed (raised by Lex, 2026-07-04)

`World.rng` (xorshift32, drives all runtime sim randomness: fire, spores, particles,
growth) is initialized to a FIXED constant `0x9E3779B9` in `World::new` and is NOT
reseeded by `generate(seed)` — only worldgen's separate `GenRng` uses the seed. So the
live sim replays the same RNG stream every session regardless of world seed. Fine for
determinism/tests, but different seeds don't produce different emergent sim behavior.
Cheap fix when convenient: in `generate()`, set `self.rng = seed_mixed | 1` so the live
sim stream varies by seed too. Do this before/with any smoke/spore/wind randomness work.

## Moisture + explosions + water-dispersal growth chain (raised by Lex, 2026-07-04)

The emergent loop Lex wants: **explosion near water → flings water across the area →
water lands on dry soil → soil becomes MOIST → mycelium colonizes moist soil faster than
dry soil.** A satisfying gun→terrain→ecosystem chain.

Three pieces, each usable on its own:

1. **Moist soil.** A soil cell that has absorbed water grows mycelium faster.
   - *Recommended impl:* moisture = the Soil cell's `aux` byte as a level that DECAYS over
     time (dry-out). Water contact raises it; `colonize_from` gives extra attempts / higher
     priority when the target soil's aux (moisture) is high. Reuses aux (Soil's aux is
     currently unused), no new material id, no cell-layout change. Note the existing
     `P_WATER_ACCEL` already speeds mycelium ADJACENT to standing water — moisture is the
     richer, persistent version (soil stays fertile after the water is gone, then dries out).
   - *Alt:* a distinct `MoistSoil` material id (simpler rules, but no gradient/decay and
     costs a material slot).
   - Interacts with the existing powder physics: moist soil could resist falling / clump
     (optional flavor).

2. **General explosion primitive.** The spec already names a shared "explosion primitive
   (radial displacement + ignition)" used by spore-gas detonation and heavy ammo. Formalize
   it as one `explode(x, y, radius, force)` that: displaces/ejects cells as particles
   (pixels-as-particles already exists), ignites flammables, AND flings nearby liquids
   (water/oil) outward as particles. Reuse for a dedicated explosive ammo/round.

3. **Water dispersal from explosions.** When `explode` hits water, convert those water
   cells into fast-moving particles scattered across the blast radius; where they land on
   dry soil, they raise its moisture (feeding piece 1). This is the payoff chain.

**Sequencing:** piece 1 (moist soil) is small and slots naturally into a growth follow-up or
M1c.x. Pieces 2-3 (explosion primitive + water fling) are a combat/physics feature — likely
their own milestone alongside the wind/air work, since "explosions fling particles" and
"wind moves particles" share the particle system. Grill before building.

## Mushroom species variety + other vegetation (raised by Lex, 2026-07-04)

Current growth has ONE mushroom archetype (parametric stem+cap). Lex wants variety:
- **Multiple mushroom species** — a `MushroomKind` enum driving different parametric shapes
  and growth rules; `GrowingMushroom` gains a `kind` field, `try_fruit` picks a kind by
  context (substrate, moisture, RNG).
- **Bracket / shelf / conch fungi that grow on WOOD** (esp. wet wood) — these anchor to a
  vertical solid surface and grow HORIZONTALLY outward in shelves, unlike the ground-up
  stem+cap. Different reveal geometry (anchored sideways), triggered by mycelium/spores on
  wood rather than soil. Ties into the moisture idea (wet wood → faster/preferred).
- **Mosses / low vegetation** — a cheap surface-colonizer: a thin creeping layer over
  exposed soil/rock surfaces (a 1-cell "skin"), maybe its own material or a Mycelium
  cousin. Ambient life/color, not a fruiting body. Could reuse the frontier with a
  surface-only spread rule.

**Blocker/prereq — there is NO Wood material yet.** Current material set stops at Fire=12
(Empty/Rock/Sand/Water/Oil/Soil/Mycelium/MushroomFlesh/SporeGas/Smoke/Ash/Acid/Fire). The
M1 plan referenced wood as an ammo target but it was never added. Wood-dwelling fungi (and
"wet wood") require adding a `Wood` material first (flammable, solid, maybe moisture-bearing
via aux like the moist-soil idea). Worth doing Wood as its own small addition — it also
unlocks the burn/structure-collapse fantasies (wood supports → M2 rigid bodies).

**Sequencing:** species variety is a natural M1c.x / M1-growth-polish follow-up once the
core lifecycle is proven fun. Wood material is a prerequisite and small; mosses are cheap
ambient flavor. None are blocking for M1c's grin test.

**Refinement (Lex, 2026-07-04):** substrate does NOT dictate shape — ordinary stem+cap
species (e.g. psilocybes) grow on wood too, not only bracket/shelf fungi. So the model is:
`MushroomKind` (shape/rules) is chosen semi-independently of substrate (soil vs wood vs wet
wood), with substrate/moisture biasing WHICH kinds are likely and how fast — not a 1:1
"wood ⇒ bracket only" mapping. Both normal mushrooms and brackets can be wood-borne.

**Further refinement (Lex, 2026-07-04):** species should also drive different MYCELIUM and
FLESH variants, not just cap shape — i.e. a `MushroomKind` colors/behaves its whole
lifecycle (its mycelium network + its fruiting-body flesh look/props differ by species).
Impl options given the u8 material set: (a) keep single Mycelium/MushroomFlesh material ids
but store species in `shade` (render-only variation) or a few `aux` bits; (b) add distinct
material ids per species (costs slots, simplest rules). Leaning (a) — species tag in shade,
render picks palette, growth rules read the tag — avoids material-id explosion.
