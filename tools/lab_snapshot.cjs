// tools/lab_snapshot.cjs — dev-only. Renders the real home page once in
// headless Chromium and writes what the liquid lab needs to sit on top of it:
//
//   site/lab/page.jpg    the page at 800 CSS px wide, 2x, effects at rest
//   site/lab/marble.png  the BACKGROUND= tile, to extend the page sideways
//   site/lab/page.json   size + the labelled rectangles of its objects
//                        (tables, pictures) in CSS px – the liquid's colliders,
//                        and the logo is the anchor for aligning live layouts
//
// Usage: (serve site/ on :8765) node tools/lab_snapshot.cjs
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');

(async () => {
  const out = path.join(__dirname, '..', 'site', 'lab');
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 800, height: 900 }, deviceScaleFactor: 2 });
  await p.goto('http://localhost:8765/');
  await p.waitForTimeout(2500); // kernel up, counter rolled, nothing destroyed yet
  const info = await p.evaluate(async () => {
    const rect = (e) => { const r = e.getBoundingClientRect(); return [r.left, r.top + scrollY, r.right, r.bottom + scrollY].map((v) => Math.round(v * 10) / 10); };
    const objs = [...document.querySelectorAll('table[border]:not([border="0"]), canvas[data-surface]')]
      .filter((e) => e.dataset.surface !== 'rainbow')
      .map((e) => ({ s: e.dataset.surface || 'table', r: rect(e) }));
    const tile = await (await fetch(document.body.background)).blob();
    const tileURL = await new Promise((res) => { const fr = new FileReader(); fr.onload = () => res(fr.result); fr.readAsDataURL(tile); });
    return { width: 800, height: Math.ceil(document.documentElement.scrollHeight), objs, tileURL };
  });
  await p.screenshot({ path: path.join(out, 'page.jpg'), type: 'jpeg', quality: 88, fullPage: true });
  fs.writeFileSync(path.join(out, 'marble.png'), Buffer.from(info.tileURL.split(',')[1], 'base64'));
  delete info.tileURL;
  fs.writeFileSync(path.join(out, 'page.json'), JSON.stringify(info));
  console.log(`page ${info.width}x${info.height}, ${info.objs.length} objects`);
  await b.close();
})();
