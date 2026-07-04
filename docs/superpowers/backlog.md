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
