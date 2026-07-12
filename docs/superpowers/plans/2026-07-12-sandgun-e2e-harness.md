# SANDGUN — Committed e2e / Gameplay Test Harness Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A committed Playwright suite in `web/` that boots the real game and drives the avatar/gun/world, asserting observable behavior across the wasm boundary + input wiring + render loop — the integration layer the Rust unit tests can't reach, and which has been re-written as throwaway scripts ~5 times (M0/M1a/M1b/M1c/M1e/M1d acceptance).

**Architecture:** Playwright as a `web/` devDependency, a `playwright.config.js` whose `webServer` auto-starts `npm run dev`, a `web/tests-e2e/` dir, and an `npm run test:e2e` script. Tests use `page.goto('/')`, wait for the app's `window.sandgun` handle, drive inputs via synthetic events / direct `world` calls, advance frames via `requestAnimationFrame` in `page.evaluate`, and assert on the accessors the app already exposes (`avatar_center`, `avatar_xywh`, `projectile_count`, `particle_count`, `colony_count`, `tip_count`, `mushroom_count`, `burning_count`, `cells_processed`, plus `window.sandgun.fps`/`uploadedChunks`). A fixed world seed via `world.generate(SEED)` gives deterministic worlds.

**Tech Stack:** Playwright (`@playwright/test`), pinned. No app-code changes except a tiny, already-mostly-present debug-handle export.

## Global Constraints

- Tests must be **deterministic and hermetic**: each test `generate(FIXED_SEED)`s a known world (or constructs its own `WasmWorld`) rather than relying on the random default seed; drives a bounded number of frames; asserts ranges/thresholds (not exact pixel counts) where sim RNG or timing varies.
- No product-behavior change to ship tests. The only app change allowed: ensure `window.sandgun` exposes what tests need (it already exposes `{world, wasm, input, gun, fps}` + `uploadedChunks`; add `cam` if a test needs camera state). Do NOT add test-only branches to game logic.
- Tests drive through the **real** app (the running `main.js` loop), not a reconstructed harness — that's the point (catch integration regressions).
- Keep it runnable headless in CI-like conditions; document the one-time `npx playwright install chromium`. Don't gate the Rust build on e2e.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` (second `-m`). 
- **Branch:** `git checkout -b e2e-harness` before Task 1. (Do this AFTER the worldgen milestone merges, or on top of current master — the harness is independent; note that worldgen changes will change generated-world specifics, so prefer landing this after worldgen so the gameplay tests target the final world.)

## Prerequisite note

The repo currently has NO Playwright in `web/node_modules` (throwaway scripts used the global npx cache at `/Users/lex/.npm/_npx/...`). This plan makes it a real committed devDependency.

---

### Task 1: Playwright scaffold + smoke test

**Files:** Modify `web/package.json`; create `web/playwright.config.js`, `web/tests-e2e/helpers.js`, `web/tests-e2e/smoke.spec.js`; update `.gitignore` (ensure `web/test-results/` + `web/playwright-report/` ignored).

**Interfaces produced:** `npm run test:e2e` (from `web/`) boots the dev server + runs the suite headless. `helpers.js` exports `bootGame(page)` (goto + wait for `window.sandgun` ready) and `stepFrames(page, n)` (advance n rAF frames in-page) and `genWorld(page, seed)` (deterministic world).

- [ ] **Step 1: Add Playwright + script.** In `web/package.json` add `"@playwright/test"` to `devDependencies` (pin a recent version) and a script `"test:e2e": "playwright test"`. Run `cd web && npm install` then `npx playwright install chromium` (document this in the report as the one-time setup).

- [ ] **Step 2: playwright.config.js** (`web/playwright.config.js`):
```js
import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests-e2e',
  timeout: 30_000,
  fullyParallel: false,          // one shared dev server; sim is stateful
  workers: 1,
  use: { baseURL: 'http://localhost:5173', headless: true },
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
```
(If Vite picks a different port when 5173 is taken, pin it: add `--strictPort` or `server.port` in vite config, or read the port — pin it for test stability.)

- [ ] **Step 3: helpers.js** (`web/tests-e2e/helpers.js`):
```js
export async function bootGame(page) {
  const errors = [];
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', e => errors.push(String(e)));
  await page.goto('/');
  await page.waitForFunction(() => window.sandgun && window.sandgun.world && window.sandgun.world.avatar_center, null, { timeout: 15_000 });
  return { errors };
}
// Advance exactly n animation frames of the REAL app loop, in-page.
export async function stepFrames(page, n) {
  await page.evaluate((count) => new Promise(res => {
    let i = 0;
    const tick = () => { if (++i >= count) return res(); requestAnimationFrame(tick); };
    requestAnimationFrame(tick);
  }), n);
}
// Deterministic world: regenerate with a fixed seed and settle a moment.
export async function genWorld(page, seed) {
  await page.evaluate((s) => window.sandgun.world.generate(s >>> 0), seed);
}
```

- [ ] **Step 4: smoke.spec.js** (`web/tests-e2e/smoke.spec.js`):
```js
import { test, expect } from '@playwright/test';
import { bootGame, stepFrames } from './helpers.js';

test('app boots, renders, no console errors', async ({ page }) => {
  const { errors } = await bootGame(page);
  await stepFrames(page, 30);
  const state = await page.evaluate(() => ({
    hasWorld: !!window.sandgun.world,
    avatar: window.sandgun.world.avatar_center(),
    fps: window.sandgun.fps,
  }));
  expect(state.hasWorld).toBe(true);
  expect(state.avatar).toBeTruthy();      // avatar spawned
  expect(errors, `console errors: ${errors.join(' | ')}`).toEqual([]);
});
```

- [ ] **Step 5: Run** — `cd web && npm run test:e2e`. Expected: smoke test passes (boots, renders, no errors). Verify `.gitignore` covers `web/test-results/`, `web/playwright-report/`, and that `web/node_modules` stays ignored.

- [ ] **Step 6: Commit** — `test: e2e harness scaffold (Playwright) + boot smoke test (harness task 1)`

---

### Task 2: Core gameplay tests

**Files:** Create `web/tests-e2e/gameplay.spec.js` (+ extend helpers if needed).

**Interfaces produced:** committed tests for the core verbs, each driving the real app and asserting observable state. Use `genWorld(page, SEED)` for determinism; drive inputs via `window.sandgun.input`/`gun` fields or synthetic key/mouse events; assert via the exposed counts.

- [ ] **Step 1: Write the gameplay tests** (`gameplay.spec.js`). Each: boot, `genWorld(FIXED_SEED)`, act, `stepFrames`, assert. Representative set:
  - **avatar walks and stops at a wall:** set `input.right = true` (or dispatch `keydown` 'd'), step ~120 frames, assert `avatar_center().x` increased; place/confirm a wall (or walk into world edge) and assert x stops advancing (doesn't pass through solid).
  - **kinetic round craters terrain + spawns debris:** aim + fire kinetic at nearby terrain (set `gun.ammo=0`, `gun.firing=true` toward a solid spot, or call `world.fire(x,y,vx,vy,0)`), step, assert some solid cells near impact became Empty (crater) AND `particle_count()` spiked above 0 during/after.
  - **incendiary → oil → fire chain → burns out:** paint an oil pocket (`world.paint(x,y,r, OIL)`), fire incendiary into it (or `world.fire(...,1)`), step; assert `burning_count()` rises above 0, then after many steps returns toward 0 (burns out) — the fire-chain lifecycle.
  - **mycelium grows:** genWorld (seeds colonies), record mycelium cell count (count via a small in-page loop over `world.get`, or add a `mycelium_count()` accessor if cheap), step a few hundred frames, assert it increased (colonies grow).
  - **world sleeps when settled:** genWorld, step until settled (generous budget), assert `cells_processed()` returns to 0 (or a tiny value from background colonies — assert `< small threshold`), proving chunk-sleep end-to-end through the real loop.
  - Assert `errors` empty in each (bootGame collects them).

- [ ] **Step 2: Run** — `cd web && npm run test:e2e`. All gameplay tests pass. Tune thresholds to be robust (ranges, not exact counts) — these run against the real sim with its RNG; use fixed seeds + generous step budgets + threshold assertions.

- [ ] **Step 3: Commit** — `test: e2e gameplay tests (avatar/gun/fire-chain/growth/sleep) (harness task 2)`

---

### Task 3: Render/camera tests + docs

**Files:** Create `web/tests-e2e/render.spec.js`; update `web/README` or a short `web/tests-e2e/README.md`.

- [ ] **Step 1: Render/camera tests** (`render.spec.js`), asserting the M1d invariants through the real loop:
  - **settled world uploads ~zero chunks:** genWorld, step until a local region settles, read `window.sandgun.uploadedChunks` over several frames, assert it drops to a small value (only background-colony activity) — the dirty-chunk render proof.
  - **camera pan uploads ~zero:** move the avatar through already-static terrain (no cell changes) and assert `uploadedChunks` stays ~0 while the camera scrolls (camera y/x changes but uploads don't) — needs `cam` exposed on `window.sandgun` (add it in main.js if absent: `window.sandgun.cam = cam`).
  - **no console errors / no NaN camera:** after a long descent, assert `cam` x/y are finite and within `[0, world-view]`.

- [ ] **Step 2: Short docs.** `web/tests-e2e/README.md`: how to run (`npm run test:e2e`), one-time `npx playwright install chromium`, that it auto-starts the dev server, and the determinism convention (fixed seeds + threshold asserts). Note e2e is not part of the Rust `cargo test` gate (run separately / in a web CI job).

- [ ] **Step 3: Run full suite** — `cd web && npm run test:e2e` (smoke + gameplay + render all green). `cargo test -p sandgun-core` unaffected. `./scripts/build-wasm.sh` still works.

- [ ] **Step 4: Commit** — `test: e2e render/camera invariants + harness docs (harness task 3)`

---

## Self-review notes

- Value: turns the throwaway per-milestone Playwright acceptance into a committed suite covering the integration layer (wasm boundary, input, render loop, camera) that Rust unit tests can't reach.
- Determinism handled via fixed-seed `generate()` + `stepFrames` + threshold assertions; the real app loop is driven (not reconstructed), so it catches real integration regressions.
- Only app change is exposing debug handles on `window.sandgun` (mostly already there); no test-only game-logic branches.
- **Sequencing recommendation:** land this AFTER the worldgen milestone merges, so the gameplay tests target the final generated world (worldgen changes would otherwise churn the tests). If run before, keep gameplay-test setups paint-their-own-terrain rather than depending on worldgen specifics.
- Deferred: visual/screenshot snapshot testing (flaky across GPUs), CI wiring (a follow-up once the suite is stable), perf-budget assertions (headless fps is unreliable — keep fps checks qualitative).
