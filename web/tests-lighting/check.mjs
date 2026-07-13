let chromium;
try {
  ({ chromium } = await import('playwright'));
} catch {
  ({ chromium } = (await import('/Users/lex/.npm/_npx/e41f203b7505f1fb/node_modules/playwright/index.js')).default);
}
const b = await chromium.launch(); const p = await b.newPage();
const errs = []; p.on('console', m => m.type()==='error' && errs.push(m.text()));
p.on('pageerror', e => errs.push(String(e)));
await p.goto('http://localhost:5173/');
await p.waitForFunction(() => window.sandgun?.world?.avatar_center, null, { timeout: 15000 });

// Fixed seed for a deterministic scene, then let the frame loop render lit for ~60 frames.
await p.evaluate(() => window.sandgun.world.generate(7));
await p.evaluate(() => new Promise(r => { let i=0; const t=()=>{ if(++i>=60) return r(); requestAnimationFrame(t); }; requestAnimationFrame(t); }));

const glErr = await p.evaluate(() => { const g = document.getElementById('view').getContext('webgl2'); return g.getError(); });

// --- Check 1: depth-ambient contrast (far/deep is darker than near the avatar). ---
// Sample luminance from the composited (lit) canvas. readPixels is bottom-origin, in
// drawing-buffer pixels (640x384). Read inside a rAF so the just-drawn frame is intact.
const lum = await p.evaluate(() => new Promise(res => {
  requestAnimationFrame(() => {
    const view = document.getElementById('view');
    const g = view.getContext('webgl2');
    const W = view.width, H = view.height; // 640 x 384
    const cam = window.sandgun.cam;
    const c = window.sandgun.world.avatar_center();
    // avatar top-down screen pixels -> readPixels bottom-origin
    const sx = Math.max(2, Math.min(W - 3, Math.round(c[0] - cam.x)));
    const syTop = Math.round(c[1] - cam.y);
    const nearGlY = Math.max(2, Math.min(H - 3, H - syTop));
    // far point: opposite horizontal side + near the bottom of the view (deepest world =
    // darkest depth ambient), well outside the 90px player-light radius.
    const farX = sx < W / 2 ? W - 3 : 2;
    const farGlY = 2; // GL bottom row = deepest visible world cell
    const read = (x, y) => {
      const px = new Uint8Array(4);
      g.readPixels(x, y, 1, 1, g.RGBA, g.UNSIGNED_BYTE, px);
      return (px[0] + px[1] + px[2]) / 3;
    };
    res({ nearLum: read(sx, nearGlY), farLum: read(farX, farGlY),
          sx, nearGlY, farX, farGlY });
  });
}));

// --- Check 2: emission hue. Paint guaranteed emitters at fixed on-screen positions and prove
// the coloured-emission path (not depth ambient / material colour alone) contributes: FIRE
// glows warm (R > B) and bright; MushroomFlesh glows green-dominant (G > R and G > B). Both
// patches sit ~160px from the avatar, outside the 90px warm player light, so its orange tint
// can't manufacture the fire result nor pollute the fungi one. Repaint every frame (fire rises
// and burns out) so the sampled centre is a fresh emitter, then read the very next frame.
const hue = await p.evaluate(() => new Promise(res => {
  const view = document.getElementById('view');
  const g = view.getContext('webgl2');
  const W = view.width, H = view.height; // 640 x 384; 1 screen px == 1 world cell
  const cam = window.sandgun.cam;
  const world = window.sandgun.world;
  // Fix patches to WORLD cells (snapshot cam once) so a panning camera -- the avatar may still be
  // falling -- can't slide the sample off the patch: we repaint the same world cells every frame
  // and sample where they currently are on screen (worldCell - currentCam). Fungi left, fire right,
  // mid height; ~160 cells from view centre so both stay clear of the 90px warm player light.
  const fungiWX = Math.round(cam.x + 160), fireWX = Math.round(cam.x + 480), wY = Math.round(cam.y + 192);
  const paint = () => {
    world.paint(fungiWX, wY, 9, 7);   // MushroomFlesh (id 7)
    world.paint(fireWX,  wY, 11, 12); // Fire (id 12, renders as FLAME)
  };
  let i = 0;
  const step = () => {
    paint();               // repaint before the frame's sim step consumes/moves it
    if (++i < 8) { requestAnimationFrame(step); return; }
    // One more frame renders the freshest paint, then sample it via the CURRENT camera.
    requestAnimationFrame(() => {
      const clampX = x => Math.max(2, Math.min(W - 3, x));
      const clampY = y => Math.max(2, Math.min(H - 3, y));
      const read = (wx, wy) => {
        const sx = clampX(Math.round(wx - cam.x));   // world cell -> top-down screen px
        const sy = clampY(Math.round(wy - cam.y));
        const px = new Uint8Array(4);
        g.readPixels(sx, H - sy, 1, 1, g.RGBA, g.UNSIGNED_BYTE, px); // top-down -> bottom-origin
        return { r: px[0], g: px[1], b: px[2], sx, sy };
      };
      res({ fungi: read(fungiWX, wY), fire: read(fireWX, wY) });
    });
  };
  requestAnimationFrame(step);
}));

console.log('gl error:', glErr, '| console errors:', errs.length, errs.join(' | '));
console.log('lum near:', lum.nearLum.toFixed(1), 'far:', lum.farLum.toFixed(1),
  '| near@', lum.sx + ',' + lum.nearGlY, 'far@', lum.farX + ',' + lum.farGlY);
console.log('fire  rgb:', hue.fire.r, hue.fire.g, hue.fire.b, ' (R-B =', hue.fire.r - hue.fire.b, ') @screen', hue.fire.sx + ',' + hue.fire.sy);
console.log('fungi rgb:', hue.fungi.r, hue.fungi.g, hue.fungi.b, ' (G-R =', hue.fungi.g - hue.fungi.r, 'G-B =', hue.fungi.g - hue.fungi.b, ') @screen', hue.fungi.sx + ',' + hue.fungi.sy);

const fail = [];
if (glErr !== 0) fail.push('GL error ' + glErr);
if (errs.length) fail.push('console errors');
if (!(lum.farLum < lum.nearLum)) fail.push('farLum !< nearLum');
if (!(lum.farLum < 160)) fail.push('farLum not dark (>=160)');
// Fire: bright + warm (orange). Red must clearly beat blue.
if (!(hue.fire.r >= 60)) fail.push('fire not bright (R<60)');
if (!(hue.fire.r - hue.fire.b >= 25)) fail.push('fire not warm (R-B<25)');
// Fungi: green-dominant glow (a hue neither ambient nor a warm player light can produce).
if (!(hue.fungi.g - hue.fungi.r >= 10)) fail.push('fungi not green>red (G-R<10)');
if (!(hue.fungi.g - hue.fungi.b >= 10)) fail.push('fungi not green>blue (G-B<10)');
if (fail.length) { console.log('FAIL:', fail.join('; ')); await b.close(); process.exit(1); }
console.log('OK'); await b.close();
