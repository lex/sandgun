# M1c — "The World Grows Back" — Sharpened Design

*Product of a /grill-me session, 2026-07-04. Feeds a writing-plans → subagent-driven implementation, like M1a/M1b. Where this and the M1/M1.5 sections of PLAN.md disagree, this and the mushroom-vision spec govern.*

## One sentence

The fungal cavern comes alive: mycelium creeps across soil and bridges the craters you shoot, matured patches fruit mushrooms cell-by-cell, caps puff spores that reseed new colonies — all on a budgeted frontier so the sleeping-world optimization survives.

## Scope: FULL lifecycle in M1c

The complete loop ships in one milestone (user call, eyes open to the size): **colonize → fruit → puff → reseed**. Not cut into sub-milestones.

## Locked decisions

| Question | Decision |
|---|---|
| Scope | **Full lifecycle** — colonize + fruit + puff + reseed, all in M1c |
| Gun × growth | **Growth reclaims damage** — the frontier creeps back into carved craters; you damage, the world heals. This is the core loop |
| World | **Current 640×384** — the big 1024×2048 world + follow-cam stays M1d |
| Starting world | **Animate existing M1a worldgen** — seed the frontier from existing mycelium edges at load; no worldgen changes |
| Growth cadence | **Every N frames, hard budget** — a budgeted slice of the frontier processed every N frames (N + slice size hot-reloadable); mutations `wake()` their chunks; empty frontier = zero cost |
| Crater reclaim | **Mycelium bridges empty** — a living solid that slowly grows across empty space, not just soil, so wounds visibly close |
| Bridge leash | **Range-limited from soil** — mycelium only bridges empty cells within a max reach of solid soil/mycelium mass; heals wounds and gaps, won't sprawl across open caverns |
| Mushroom shape | **Parametric stem+cap** — a few tunable numbers + `next_rand` jitter; revealed cell-by-cell bottom-up over ~5-10s; deterministic |
| Fruiting trigger | **Maturity timer + global cap** — mycelium ages while undisturbed; past a threshold it's a fruiting candidate with a low per-tick chance; capped at max N simultaneous growing mushrooms |
| Maturity state | **`aux` byte on mycelium** — free because `aux` only means "fuel" once `FLAG_BURNING` is set, and burning mycelium is dying |
| Shooting a cap | **Dumps the whole spore cloud** — releases accumulated SporeGas at once: drifts + reseeds, OR detonates near fire (reuses M1a spore-detonation) |
| Kill criterion | **Watchable + reclaims craters + still sleeps** — shoot a hole, watch it visibly creep back over ~10-30s, and the world still fully sleeps once growth settles. If regrowth is boring to watch or kills the framerate, diagnose before building more |

## Engineering reconciliations (made, not grilled)

- **Two counters, one byte — resolved.** Reach-from-soil is a growth-time concern that lives on the **frontier entry** (transient): a new bridged cell's reach = parent's reach + 1; a cell that grows into soil resets reach to 0; when reach exceeds the max, empty neighbors aren't enqueued. Maturity is a persistent per-cell fact in **`aux`**. They never share the byte.
- **Determinism:** all growth randomness (mushroom jitter, fruiting chance, puff timing, colonize order) via `World::next_rand`/`chance`. No wall-clock, no `Math.random` in the sim.
- **Chunk-sleep is still sacred.** Growth mutates cells and `wake()`s their chunks so falling-sand reacts (new mycelium is solid, freed spores drift). The frontier **must retire** cells that have nothing left to colonize so it can empty — a growth run has to reach a terminal state. Convergence is guaranteed by: finite soil, range-limited bridging, and the mushroom cap. **Extend the M1b combined chunk-sleep regression test**: settled world + empty frontier ⇒ `cells_processed == 0`.
- **Frontier structure:** a bounded `Vec<FrontierCell>` on `World`, processed by a new `World::grow()` step invoked on the N-frame cadence. Entries carry cell coords + reach. Hard cap on frontier size (oldest/lowest-priority entries drop if exceeded — and `log`/count what's dropped, no silent truncation).
- **Fire interaction (from M1a) still holds:** fire kills mycelium and races along it as a fuse; a burning cell is removed from the frontier and its `aux` becomes fuel. Growth and fire are opposites on the same material.
- **Spore reseed** reuses the existing `SporeGas` material + upward drift from M1a: spores that settle on damp/soil cells seed new frontier entries.

## The lifecycle, concretely

1. **Colonize** — frontier mycelium converts adjacent soil at a visible crawl; bridges adjacent empty within reach. Water contact accelerates (spec). New growth joins the frontier; exhausted cells retire.
2. **Fruit** — mycelium ages in `aux` while undisturbed; a mature cell, under the global mushroom cap, may become a mushroom origin. A parametric stem+cap is revealed cell-by-cell (`MushroomFlesh`) bottom-up over seconds.
3. **Puff** — mature caps periodically release small `SporeGas` clouds; shooting a cap dumps its whole cloud at once.
4. **Reseed** — spores settling on soil seed new frontier entries — growth reaches disconnected patches, the world is self-sustaining.

## Hot-reloadable budgets/tunables (all tuned during M1c, via `params.json` + `p`)

Growth-tick interval N · conversions per growth tick · max frontier size · max reach into empty · water-acceleration factor · maturity threshold · max simultaneous growing mushrooms · fruiting chance/tick · mushroom stem-height & cap-width ranges · mushroom reveal rate · puff interval · spores per puff.

## Deliberately deferred

Big world + follow-cam (M1d) · enemies · roguelite layer · native build · audio.
