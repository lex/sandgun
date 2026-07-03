# SANDGUN: Living Fungal World — Vision & M1 Design

*Brainstormed and approved 2026-07-03. Evolves PLAN.md's vision; all previously locked decisions
remain locked (physicality first, contact reactions only — no temperature field, browser-first,
60fps on Lex's Mac, rigid bodies at M2, enemies/roguelite/avatar parked).*

## Vision

**Descend into a living fungal cavern with a gun.** The world is one huge organism — mycelium
veins the soil, giant mushrooms tower in the caves, spore clouds drift through the dark. Bullets
are physics events; chemistry triggers physicality; and the world *responds*: carve a crater and
the network slowly reclaims it, burn a vein and fire races down it like a fuse, pop a mushroom
cap and the spore burst chains into an explosion — or seeds a new colony. Tone: bioluminescent
and alive, slightly indifferent to the player. Not horror.

## Decisions made in this session

| Question | Decision |
|---|---|
| Fungus role | **Living, growing world** — growth is a real sim mechanic, not a skin |
| Sequencing | Growth ships **inside M1**; the grin test targets the living world |
| Growth pace | **Visible crawl** — watchable over seconds to tens of seconds |
| Player | **Free cursor + gun** — no avatar in M1; camera pans, you aim anywhere |
| Growth tech | **Budgeted growth frontier** (approach B below) |
| Mushrooms | Required — fruiting bodies, not just mycelium; full lifecycle |
| Fire | New in M1, central — cell-state fire, no temperature field |

## The fungal lifecycle (the one new mechanic)

### New materials
Still 4-byte cells, still contact-pair rules:

- `Soil` — powder, colonizable by mycelium
- `Mycelium` — living solid, flammable; the network/fuse
- `MushroomFlesh` — soft solid (stems, caps); carvable, burns slowly
- `SporeGas` — drifts upward, disperses, **detonates on fire contact**
- `Ash` — burn residue (powder); enriches soil (faster recolonization)
- `Smoke` — rises, thins, vanishes (fire byproduct)

Existing sand/water/rock/oil remain; oil re-flavors as decay sludge later (mechanics unchanged).

### The loop (all growth on the budgeted frontier)
1. **Colonize** — frontier mycelium converts adjacent soil at a visible crawl. Water contact
   accelerates growth; fire kills it and burns along the network.
2. **Fruit** — a mycelium patch left mature and undisturbed sprouts a mushroom: a procedural
   stem-and-cap grown cell by cell over seconds (watchable).
3. **Puff** — mature caps periodically release small spore-gas clouds; spores drift, settle on
   damp soil, seed new frontier entries. Shooting a cap releases its whole cloud at once.
4. **Die** — burned/carved fungus leaves ash; ash-enriched soil recolonizes faster — burning is
   effective now but feeds the bloom later.

### Growth tech: budgeted frontier (approach B — chosen over alternatives)
The falling-sand `step()` stays untouched. `World` keeps a bounded list of actively growing
cells (the frontier). Every few frames a budgeted slice is processed: each frontier cell tries
to colonize one eligible neighbor; new growth joins the frontier; cells with nothing left to
colonize retire. Spores landing on soil seed new entries. Growth mutates cells through the
normal wake path, so **chunk sleeping survives**: the world idles asleep except along the
visible growing edge.

Rejected: (A) growth as in-step material rules — growing regions would never sleep, destroying
the M0 chunk-sleeping win; (C) timer-based wound healing — animation pretending to be
simulation.

Hard budgets (tuned during M1): max frontier size, max conversions per growth tick, fruiting
cooldowns. Acceptance: an untouched living world must still settle to near-zero sim cost
between growth ticks.

## Fire (M1, central)

Cell-state fire, no temperature field. A burning cell shows flame, burns fuel down in `aux`,
probabilistically ignites flammable contacts, emits smoke. Flammability: sludge/oil fierce,
mycelium steady (veins are fuses through terrain), mushroom flesh slow (big ones smolder),
spore gas detonates. The **explosion primitive** (radial displacement + ignition) is shared by
spore-gas detonation and heavy ammo. Water extinguishes on contact; organics burn to ash.
Burning cells have finite fuel, so fire is naturally bounded and chunks re-sleep after burns.

## M1 gun

Free cursor; one chassis, swappable ammo:
- **Kinetic** — crater + ejecta (pixels-as-particles debris)
- **Incendiary** — ignites; the fuse-lighter
- **Acid** — dissolves terrain and stems (topple caps tactically)
- **Spore round** — the builder: seeds mycelium; near water, a fast bloom (bridges, plugs)

## World & camera (M1)

Vertical world grows to 1024×2048 (config change — dims already parameterized). WASD/edge-pan
camera. Renderer uploads only the camera window (fixes the full-world-upload scaling issue
flagged in the M0 final review). Worldgen grows the ecosystem at generation time: mycelium
veins through soil strata, mushroom groves (some giant), spore pockets, glow-toned palette.

## Dev loop (M1)

Hot-reloadable tuning without rebuilding wasm: growth rates, fire spread/fuel values, ammo
params in a JSON the browser reloads live. (PLAN.md dev-loop mandate.)

## Kill criterion (updated)

**60 seconds of shooting the living world makes Lex grin** — carve, watch it heal, light a
mycelium fuse, pop a cap, chain a spore blast. If growth isn't fun, the vision needs to know
before anything else is built on it.

## Parked (unchanged)

Player avatar/movement · enemies beyond the bloom itself · rigid-body mushroom caps (M2's
debut feature) · roguelite meta-layer · native build · audio · lighting engine (glow via
palette only).
