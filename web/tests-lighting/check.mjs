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

console.log('gl error:', glErr, '| console errors:', errs.length, errs.join(' | '));
console.log('lum near:', lum.nearLum.toFixed(1), 'far:', lum.farLum.toFixed(1),
  '| near@', lum.sx + ',' + lum.nearGlY, 'far@', lum.farX + ',' + lum.farGlY);

const fail = [];
if (glErr !== 0) fail.push('GL error ' + glErr);
if (errs.length) fail.push('console errors');
if (!(lum.farLum < lum.nearLum)) fail.push('farLum !< nearLum');
if (!(lum.farLum < 160)) fail.push('farLum not dark (>=160)');
if (fail.length) { console.log('FAIL:', fail.join('; ')); await b.close(); process.exit(1); }
console.log('OK'); await b.close();
