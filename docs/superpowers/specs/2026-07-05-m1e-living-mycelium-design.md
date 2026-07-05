# M1e — "Living Mycelium" — Sharpened Design

*Product of a /grill-me session, 2026-07-05. A full redesign of M1c's growth model, driven by Lex's playtest feedback. Where this and M1c's growth sections disagree, this governs. Feeds a writing-plans → subagent-driven implementation.*

## One sentence

Mycelium becomes a living organism: thin hyphal strands creep outward seeking food, a colony eats substrate to fill a nutrient pool, fruits mushrooms when fed and starves back when not, and anything cut off from the ground falls and dies — replacing M1c's fill-a-blob-and-randomly-fruit model.

## Why this replaces M1c growth

Playtest of M1c exposed the model's limits: mycelium fills soil as a solid blob, fruits randomly and constantly, floating mycelium/mushrooms never fall, and there's no notion of nutrients or an organism. M1e rebuilds growth around a **colony organism with a nutrient economy** and **thin food-seeking strands**. It **supersedes and largely removes** M1c's `growth.rs` frontier/colonize/bridge/maturity-fruiting/cap-puff machinery. It **keeps**: pixels-as-particles (for falling), fire interactions, and the parametric mushroom shape (stem+dome, wavy stipe, fit-check) — fruiting still *produces* that shape, but is now *triggered* by the colony economy.

## Locked decisions (from the grilling)

| Question | Decision |
|---|---|
| Strand representation | **Hybrid** — grid cells (`Mycelium` material) are the source of truth for render/collision/burn; lightweight **side structs** (tips + colonies + a connectivity structure) drive growth |
| The organism | **Explicit Colony entity** — `{ id, nutrient_pool, tips, anchored }`, owns its cells via connectivity |
| Nutrients | **Per-cell substrate richness** stored in the soil/dead-matter cell's `aux`; consumed by mycelium, added to the colony pool |
| Food-seeking | **Local gradient sampling** — a tip biases its next step toward the richest adjacent substrate, plus randomness for organic wander |
| Connectivity (colony id + merge/split + anchor support) | **Union-find maintained incrementally on growth** (cheap unions); **carve/burn triggers a budgeted LOCAL re-flood** of the affected component to detect splits and lost anchors. Anchors = soil/rock/terrain |
| Starvation death | **Tips recede from the ends** inward toward the fed core when the pool runs dry |
| Falling (unsupported) | Disconnected-from-anchor fragment **drops as pixels-as-particles and dies on landing** (inert dead matter; no re-rooting) |
| Branching | **Both** — branch toward food on a rich hit (1–2 new tips) **and** low-chance periodic random branching, all under a **hard per-colony tip cap** (~8–16); idle/exhausted tips retire |
| Fruiting | Colony fruits when `nutrient_pool` crosses a threshold; mushroom sprouts **at a well-fed tip** and **spends a chunk of the pool** (earned, paced — no more constant fruiting) |
| Mushroom decay | Aged mushrooms decay after a lifespan; **v2: rot into rich substrate** (ash/humus) that mycelium re-consumes, closing the loop (v1 decay is simple — see scope) |
| Depleted substrate | A consumed cell **stays mycelium; the soil richness is spent** (dead ground until re-enriched) |

## Data model (high level; details in the plan)

- **Cell stays 4 bytes.** No new material for basic strands (`Mycelium=6` as now). Soil/dead-matter `aux` = **substrate richness** (0–255). Mushroom flesh unchanged. No per-cell temperature. Flags bit 7 still reserved.
- **Side state on `World`** (the "hybrid" part; not in the cell):
  - `colonies: Vec<Colony>` / map — `{ id, nutrient_pool: u32, tips: Vec<TipId>, anchored: bool }`.
  - `tips: Vec<Tip>` — `{ x, y, colony, last_dir, alive }` — the bounded set of growing strand ends (few → cheap).
  - `union_find` — parent array over mycelium cells (or grid) for connectivity; `find(cell)` → colony root. Anchors seed roots.
- **Growth driver** replaces the frontier: each growth tick, process the live **tips** (bounded by the tip cap × colony count), not a fill-frontier.

## The loop (v1)

1. **Seek & extend** — each tip samples substrate richness in its neighborhood, steps toward the richest cell (with wander), laying a thin `Mycelium` cell. Union the new cell into the colony.
2. **Eat** — when a tip's new cell sits on/consumes substrate richness, draw it down and add to the colony's `nutrient_pool`; the cell's ground is now spent.
3. **Branch** — on a rich hit, spawn 1–2 tips toward the food; plus rare periodic branching; respect the per-colony tip cap.
4. **Fruit** — when `nutrient_pool ≥ threshold`, a well-fed tip sprouts a parametric mushroom (existing shape/fit-check) and the pool is drained.
5. **Starve** — if the pool is empty and tips find no food, tips recede from the ends inward (dieback); a colony with no tips and no food eventually recedes to nothing.
6. **Fall** — carving/burning triggers a local connectivity re-flood; any fragment now disconnected from an anchor detaches and drops as particles, dying on landing.

## Chunk-sleep (sacred — how it survives)

- Growth cost is **O(live tips)**, not world size — tips are few and capped. When all colonies have no live tips and nothing is falling, the growth pass early-returns → world sleeps (as M1c's `grow()` does).
- Substrate richness changes are **event-driven** (consumption, and in v2 water/ash), never a per-frame world scan.
- Connectivity is **incremental on growth**; the expensive flood runs **only on carve/burn**, budgeted and local — destruction is already an active event.
- Union-find parent array is a one-time allocation, not per-frame work.

## v1 scope & kill criterion (Core loop, polish later)

**v1 ships:** thin strands gradient-seek food, eat substrate (from **worldgen-baked baseline richness** on soil), colony fruits when fed (spending the pool), starves back via tip dieback, and floating/severed bits fall & die. Worldgen seeds a handful of **colony origins** (each with a tip or two) + baseline soil richness, instead of pre-filled mycelium veins.

**Kill criterion:** watching a colony is *mesmerizing* — it visibly creeps toward food, thickens where it eats, fruits when fed, withers back when starved, and collapses when you cut its support — **and the world still fully sleeps once it settles.** If it's not compelling to watch, or it can't sleep, diagnose before polishing.

**Deferred to v2 (explicitly cut from v1):**
- Dynamic substrate enrichment: **water seeping into soil** (moisture in aux) and **ash raising richness** over time (v1 uses static worldgen richness).
- **Mushroom rot → rich-substrate feedback** (v1 decay is simple: aged mushrooms crumble to a little ash/empty; v2 makes rot feed the economy).
- **Species as color** (Amanita red+white-specks, etc.) and **recoloring mycelium white**.
- Wider stipes / curved-directional stems are mushroom-shape polish that can ride v1 or v2.
- Falling **as a cohesive clump** (physics) — stays M2 rigid-body territory; v1 drops as loose particles.

## Open engineering calls (made in the plan, not grilled)

- Exact tip-cap, richness scale, pool threshold, dieback rate, branch chances → all **hot-reloadable params** (tuned in playtest).
- Union-find over full grid vs mycelium-cell map; carve-reflood budget size.
- How worldgen picks colony-origin count/placement.
- Determinism: all growth/branch/wander via `next_rand`/`chance`.

## Deliberately unchanged

Falling-sand `step()` sweep, fire/acid, projectiles/gun, avatar, particles system (reused for falling mycelium), the 640×384 world (big world + camera is still M1d).
