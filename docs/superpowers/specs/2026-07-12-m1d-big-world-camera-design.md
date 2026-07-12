# M1d — "Big World + Camera" — Sharpened Design

*Product of a /grill-me session, 2026-07-12. Feeds a writing-plans → subagent-driven implementation, like the prior milestones.*

## One sentence

Grow the world to a real 1024×2048 vertical-descent cavern and add a smoothed follow-camera, with a **dirty-chunk GPU render path** so an offscreen, settled world costs ~zero to draw — the same "only pay for what moved" philosophy chunk-sleep already gives the sim.

## Why this milestone

Everything so far runs in a single 640×384 screen. M1d is the plumbing that makes SANDGUN a *place you descend through* rather than one screen. The one genuinely hard axis is **rendering at scale**: today the renderer fills and uploads the entire RGBA buffer to the GPU every frame (renderer.js `texSubImage2D` of the whole texture + a fullscreen triangle). At ~2M cells that's an ~8 MB CPU fill + upload every frame regardless of what changed. M1d fixes that; camera and worldgen-scaling ride along.

## Locked decisions (from the grilling)

| Question | Decision |
|---|---|
| World size | **1024×2048** (vertical descent, ~2M cells). Keep `World::new(w,h)` parameterized; no streaming |
| Render path | **Dirty-chunk GPU upload** — one persistent full-world texture; each frame re-upload ONLY the chunks that changed (reuse the sim's active-chunk bitmap); a settled offscreen region uploads nothing |
| Viewport | **~one screenful (~640×384 cells) at 1:1**, scroll through the tall world. Preserves the current crisp chunky-pixel look; cheapest to sample |
| Camera | **Smoothed follow (lerp) + slight downward lead + dead-zone, clamped to world edges.** Sits a bit above center so you see what you're descending into |
| Worldgen | **Tech only — scale the current noise/cave/soil worldgen to 2048 tall.** Noita-style predefined formations stay their own later milestone |
| Kill criterion | **Descend the full 1024×2048 top-to-bottom at ~60fps** — walking/falling, shooting, growing mycelium along the way — AND a settled offscreen region costs ~zero to render (dirty-chunk upload proven via the debug overlay). If it can't hold framerate at scale, diagnose before polishing |

## Render architecture (the core of M1d)

- **Persistent full-world texture:** one `RGBA8` texture sized to the world (1024×2048 = 8 MB GPU). Allocated once, never fully re-uploaded after the initial fill.
- **Dirty-chunk upload:** the sim already tracks which 64×64 chunks are active/woken each step (the `active`/`active_next` bitmap driving chunk-sleep, already exposed via `active_ptr`/`active_len`/`chunks_x`/`chunks_y`). Each frame, `render_rgba` refreshes only the changed chunks' regions of the persistent CPU RGBA buffer, and JS `texSubImage2D`s only those chunk sub-rects to the GPU texture. First frame uploads all chunks; a fully-settled screen uploads none.
- **Camera window in the shader:** the fragment shader samples the visible `[camX, camX+viewW] × [camY, camY+viewH]` window from the full texture. **Panning the camera is a free UV-window change — zero upload.** So scrolling through a static world is nearly free; only cell *changes* cost anything.
- **Moving entities (projectiles, particles, avatar):** they still stamp into the RGBA after the grid (as today), but because they move, the chunks they occupy must be marked render-dirty each frame so their cells refresh (old position clears, new position draws). Few entities → cheap. (A future option — drawing the avatar as a separate overlay sprite, per the "bigger animated avatar" backlog item — is deferred; M1d keeps the stamp-into-texture approach.)
- **Debug overlay:** extend it to show render-dirty chunk count / uploaded-bytes so the kill criterion ("settled offscreen ≈ zero upload") is observable, alongside the existing active-chunk boxes and FPS.

## Camera (engineering detail)

- Camera offset in **world cells** as `f32` (for smooth lerp). `cam = lerp(cam, target, k)`; `target = avatar_center - viewport/2 + downward_lead`; clamp so the window stays within `[0, world - viewport]`. Dead-zone: don't move `target` until the avatar leaves a central box.
- **Screen→world mapping** (aim + debug paint) gains the camera offset: `world = screen / scale + cam`. Projectiles/particles/avatar already live in world coords — only the *input mapping* and the *render window* change; the sim is camera-agnostic.
- Avatar spawns at the top; descent points down (gravity + progression aligned, per PLAN.md).

## Deliberately unchanged / deferred

Sim `step()`, chunk-sleep, growth (M1e), fire/acid, gun, avatar physics — all camera-agnostic, untouched. World stays a single bounded grid (no streaming). Predefined-formation worldgen, bigger/animated avatar sprite, zoom — all separate backlog items.

## v1 scope note

M1d is pure tech/feel: bigger world, camera, efficient render. No new gameplay. The kill criterion is a framerate + rendering-cost gate, judged by descending the whole world on Lex's Mac.
