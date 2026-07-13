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
await p.evaluate(() => new Promise(r => { let i=0; const t=()=>{ if(++i>=30) return r(); requestAnimationFrame(t); }; requestAnimationFrame(t); }));
const glErr = await p.evaluate(() => { const g = document.getElementById('view').getContext('webgl2'); return g.getError(); });
console.log('gl error:', glErr, '| console errors:', errs.length, errs.join(' | '));
if (glErr !== 0 || errs.length) process.exit(1);
console.log('OK'); await b.close();
