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

// --- Check 2: emission hue. Paint guaranteed emitters at fixed on-screen positions and prove the
// coloured-emission/propagation path (not depth ambient alone) contributes light. Fire's OWN cell
// is intrinsically orange in its base world colour ([255,150-220,40]) regardless of lighting, so
// sampling on-fire-cell warmth doesn't discriminate emission from plain material colour -- it only
// proves the cell renders bright, which we still check. What actually proves fire's emission
// propagates is a sample of guaranteed-EMPTY open air immediately beside the fire (gapNear, one
// cell outside the fire's own radius): direct lightmap instrumentation (reading `u_light`, i.e.
// `window.sandgun.gfx.light.result`, instead of the composited canvas) confirms fire's diffused
// light really does land there with a strong warm skew (e.g. R-channel light ~5x the B-channel
// light one cell out). But Empty's own base colour ([26,24,32]) is not perfectly neutral -- it
// leans slightly BLUE (B=32 > R=26) -- so in the multiplicative composite (worldColour x light)
// that base tint partly cancels the light's warm skew, and an ABSOLUTE "gapNear.r > gapNear.b"
// check on the visible canvas is too weak a signal to assert reliably (measured R-B on gapNear
// hovers around -1..0, not decisively positive, even though the underlying light is genuinely
// warm). The fix: compare gapNear's warmth against gapFar, a same-depth guaranteed-unlit Empty
// baseline -- both share the identical base-colour tint and depth ambient, so any warmth/brightness
// DELTA between them isolates fire's propagated light specifically. This delta was verified stable
// (R-B delta 3-4, luminance delta 2-3) across four different world seeds. MushroomFlesh is the
// other proof: its base material is a neutral beige ([232,208,186]), so a green-dominant reading
// (G > R and G > B) can only come from its cyan-green emission -- beige alone can never produce
// that hue, so no delta-vs-baseline trick is needed there. Fungi, fire, and the fire-adjacent gap
// all sit ~160px+ from the avatar; the far baseline is placed dynamically relative to the avatar's
// actual position (see below). All of this keeps every sampled patch outside the 90px warm player
// light, so its orange tint can't manufacture the fire result nor pollute the fungi one. Repaint
// every frame (fire rises and burns out, gas/soot can drift into the gap) so the sampled cells stay
// fresh, then read the very next frame.
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
  // Fire is painted at radius 11, so the sample must sit outside that disc to guarantee it's not
  // on the fire material itself -- gapNearWX sits exactly one cell past the disc edge (fireWX+12),
  // measured to give the strongest available warm signal (light decays fast with distance: by
  // ~6 cells out it's back at the unlit baseline). Propagation is occluded by opaque terrain (soft
  // shadows), and the fixed seed can put rock/soil right next to the fire patch, so it's not enough
  // to just force the sample cell to Empty -- the whole corridor between the fire's edge and the
  // sample point must be forced open too, or the world's own terrain can block the light from ever
  // reaching the sample even though it's genuinely propagating. The single paint below (centred at
  // fireWX+26, radius 15) forces open fireWX+11..+41 -- tangent to the fire disc's edge at +11, no
  // unpainted gap in between -- covering gapNearWX and a wide margin past it.
  const gapNearWX = fireWX + 12;
  // The far baseline must be a genuinely UNLIT control: same depth (wY) as gapNear so depth ambient
  // can't explain a difference, and its base Empty colour tint matches gapNear's exactly -- but it
  // ALSO has to sit outside the avatar's own 90px warm player-light radius, which is a screen-space
  // glow independent of world-cell distance from any emitter. The avatar tracks near screen-x ~332
  // (about view centre, same as the camera-follow target), so a naive "between the two emitters"
  // pick like camX+300 sits only ~30px from the avatar -- squarely inside its glow -- and
  // intermittently picks up extra warmth/brightness from it (observed: occasional negative
  // brightness delta against gapNear even though fire's light is genuinely reaching gapNear).
  // Mirror Check 1's fix for the same problem: read the avatar's actual on-screen x and place the
  // far baseline on whichever screen edge is farther from it (edges are >280px from centre, and
  // >140px from fire/fungi's own short reach), guaranteeing it's outside every light source.
  const avatarSx = world.avatar_center()[0] - cam.x;
  const farXScreen = avatarSx < W / 2 ? W - 20 : 20;
  const gapFarWX = Math.round(cam.x + farXScreen);
  const paint = () => {
    world.paint(fungiWX,      wY, 9, 7);    // MushroomFlesh (id 7)
    world.paint(fireWX,       wY, 11, 12);  // Fire (id 12, renders as FLAME)
    world.paint(fireWX + 26,  wY, 15, 0);   // force Empty: open corridor from fire's edge past gapNear
    world.paint(gapFarWX,     wY, 3, 0);    // force Empty: guaranteed open air, unlit baseline
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
      res({
        fungi: read(fungiWX, wY), fire: read(fireWX, wY),
        gapNear: read(gapNearWX, wY), gapFar: read(gapFarWX, wY),
      });
    });
  };
  requestAnimationFrame(step);
}));

console.log('gl error:', glErr, '| console errors:', errs.length, errs.join(' | '));
console.log('lum near:', lum.nearLum.toFixed(1), 'far:', lum.farLum.toFixed(1),
  '| near@', lum.sx + ',' + lum.nearGlY, 'far@', lum.farX + ',' + lum.farGlY);
console.log('fire    rgb:', hue.fire.r, hue.fire.g, hue.fire.b, ' (R-B =', hue.fire.r - hue.fire.b, ') @screen', hue.fire.sx + ',' + hue.fire.sy);
console.log('fungi   rgb:', hue.fungi.r, hue.fungi.g, hue.fungi.b, ' (G-R =', hue.fungi.g - hue.fungi.r, 'G-B =', hue.fungi.g - hue.fungi.b, ') @screen', hue.fungi.sx + ',' + hue.fungi.sy);
console.log('gapNear rgb:', hue.gapNear.r, hue.gapNear.g, hue.gapNear.b, ' (R-B =', hue.gapNear.r - hue.gapNear.b, ') @screen', hue.gapNear.sx + ',' + hue.gapNear.sy);
console.log('gapFar  rgb:', hue.gapFar.r, hue.gapFar.g, hue.gapFar.b, ' (R-B =', hue.gapFar.r - hue.gapFar.b, ') @screen', hue.gapFar.sx + ',' + hue.gapFar.sy);

const fail = [];
if (glErr !== 0) fail.push('GL error ' + glErr);
if (errs.length) fail.push('console errors');
if (!(lum.farLum < lum.nearLum)) fail.push('farLum !< nearLum');
if (!(lum.farLum < 160)) fail.push('farLum not dark (>=160)');
// Fire's own cell: bright (its intrinsic material colour is already orange, so this alone does
// NOT prove emission -- see gapNear below for the check that does).
if (!(hue.fire.r >= 60)) fail.push('fire not bright (R<60)');
if (!(hue.fire.r - hue.fire.b >= 25)) fail.push('fire not warm (R-B<25)');
// Fungi: green-dominant glow on a beige material (a hue neither depth ambient nor a warm player
// light could produce) -- proves the coloured-emission path.
if (!(hue.fungi.g - hue.fungi.r >= 10)) fail.push('fungi not green>red (G-R<10)');
if (!(hue.fungi.g - hue.fungi.b >= 10)) fail.push('fungi not green>blue (G-B<10)');
// gapNear: open EMPTY air beside the fire, compared against gapFar (same Empty material, same
// depth, guaranteed unlit). Empty's base colour ([26,24,32]) leans slightly blue, so an ABSOLUTE
// R>B check on gapNear alone is not reliable (that base tint partly cancels the light's warmth in
// the multiplicative composite). Comparing the DELTA against gapFar -- which shares the identical
// base tint and ambient -- isolates fire's propagated light specifically. Measured stable at a
// R-B delta of 3-4 and a luminance delta of 2-3 across several world seeds; thresholds below sit
// safely under that with margin. This is what actually proves fire's emission propagates.
const gapNearLum = (hue.gapNear.r + hue.gapNear.g + hue.gapNear.b) / 3;
const gapFarLum = (hue.gapFar.r + hue.gapFar.g + hue.gapFar.b) / 3;
const gapNearRB = hue.gapNear.r - hue.gapNear.b, gapFarRB = hue.gapFar.r - hue.gapFar.b;
console.log('gapNear vs gapFar: deltaRB =', gapNearRB - gapFarRB, ' deltaLum =', (gapNearLum - gapFarLum).toFixed(1));
if (!(gapNearRB - gapFarRB >= 2)) fail.push('gapNear not warmer than gapFar baseline (deltaRB<2)');
if (!(gapNearLum - gapFarLum >= 1.5)) fail.push('gapNear not brighter than gapFar baseline (deltaLum<1.5)');
if (fail.length) { console.log('FAIL:', fail.join('; ')); await b.close(); process.exit(1); }
console.log('OK'); await b.close();
