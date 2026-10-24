// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Headless-browser smoke test for the studio's guided demo (the Demo button): all 13 steps load, Save stays inert while the demo
// is loaded (button and Ctrl+S), the user's project comes back byte for byte, and a tab closed with the demo loaded is recovered.
//
//   cargo build -p osatura-cli && node osatura-cli/tests/studio_demo_smoke.js
//
// Environment: OSATURA_BIN (default target/debug/osatura), PLAYWRIGHT_CORE (default /opt/npm-tools/node_modules/playwright-core),
// CHROME (default /opt/pw-browsers/chromium-1194/chrome-linux/chrome). Only Chromium, a mouse and a keyboard are exercised.
const { spawn, execFileSync } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path');
const root = path.resolve(__dirname, '..', '..');
const BIN = process.env.OSATURA_BIN || path.join(root, 'target/debug/osatura');
const { chromium } = require(process.env.PLAYWRIGHT_CORE || '/opt/npm-tools/node_modules/playwright-core');
const CHROME = process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
const PORT = 8890 + Math.floor(Math.random() * 90);
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'osatura-dms-'));
process.on('exit', () => { try { fs.rmSync(tmp, { recursive: true, force: true }); } catch (e) { } });
const PROJ = path.join(tmp, 'mine.json');
execFileSync(BIN, ['demo', PROJ]);
const onDisk = fs.readFileSync(PROJ, 'utf8');
let pass = 0, fail = 0;
const check = (name, ok, extra) => { if (ok) pass++; else fail++; console.log((ok ? 'PASS ' : 'FAIL ') + name + (ok || extra === undefined ? '' : '  -> ' + JSON.stringify(extra))); };
const sleep = ms => new Promise(r => setTimeout(r, ms));

(async () => {
  const server = spawn(BIN, ['studio', PROJ, '--port', String(PORT)], { stdio: 'ignore' });
  const browser = await chromium.launch({ executablePath: CHROME });
  const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const errs = [];
  const open = async () => { const p = await ctx.newPage(); p.on('pageerror', e => errs.push(String(e))); p.on('console', m => { if (m.type() === 'error') errs.push(m.text()); }); await p.goto(`http://127.0.0.1:${PORT}/`); await sleep(1500); return p; };
  const project = p => p.evaluate(async () => JSON.stringify(await (await fetch('/api/project')).json()));
  const saveOff = p => p.evaluate(() => document.getElementById('save').disabled);
  try {
    for (let i = 0; i < 60; i++) { try { await fetch(`http://127.0.0.1:${PORT}/api/state`); break; } catch (e) { await sleep(100); } }
    let p = await open();
    const orig = await project(p);
    await p.click('#demobtn'); await sleep(200);
    check('the panel opens with the start button focused', await p.evaluate(() => document.activeElement.id) === 'dm-start');
    await p.keyboard.press('Escape'); await sleep(100);
    check('Escape closes the panel and returns focus to the Demo button', await p.evaluate(() => getComputedStyle(document.getElementById('dmp')).display === 'none' && document.activeElement.id === 'demobtn'));
    await p.click('#demobtn'); await p.click('#dm-start'); await sleep(1200);
    check('Save is off once the demo starts', await saveOff(p));
    const steps = await p.evaluate(() => document.querySelectorAll('#dm-steps button').length);
    check('13 steps', steps === 13, steps);
    const seen = [];
    for (let i = 0; i < steps; i++) {
      await p.click(`#dm-steps button[data-k="${i}"]`); await sleep(500);
      const j = JSON.parse(await project(p));
      seen.push([(j.layers || []).length, (j.bones || []).length, (j.channels || []).length]);
      await p.click('#dm-ts');
      const sc = await p.evaluate(() => ({ vis: !document.getElementById('dm-ts').hidden, txt: document.getElementById('dm-term').textContent }));
      const nn = String(Math.min(i + 1, 11)).padStart(2, '0'); // the last two steps (lint/stats, export) work on stage-11.json
      const body = sc.txt.replace(/^\$ cat [^\n]*\n/, '');
      let okJson = false; try { okJson = JSON.parse(body).name === j.name; } catch (e) { okJson = /more lines/.test(sc.txt) && body.trimStart().startsWith('{'); }
      check(`step ${i + 1}: Script tab shows stage-${nn}.json`, sc.vis && sc.txt.startsWith(`$ cat stage-${nn}.json\n`) && okJson, sc.txt.slice(0, 60));
      await p.click('#dm-tt');
    }
    check('stages grow: layers 0 → 41, then bones 42, then channels 0 → 48', seen[0][0] === 0 && seen[4][0] === 41 && seen[4][1] === 0 && seen[5][1] === 42 && seen[5][2] === 0 && seen[10][2] === 48 && seen[12][2] === 48, seen);
    await p.keyboard.press('Control+S'); await sleep(400);
    check('Ctrl+S writes nothing while the demo is loaded', fs.readFileSync(PROJ, 'utf8') === onDisk);
    // close the tab with the demo loaded: the next page keeps Save off and offers the way back
    await p.close(); p = await open();
    check('a fresh tab keeps Save off and shows the demo panel', await saveOff(p) && await p.evaluate(() => getComputedStyle(document.getElementById('dmp')).display === 'block'));
    await p.keyboard.press('Control+S'); await sleep(300);
    check('Ctrl+S in the fresh tab writes nothing', fs.readFileSync(PROJ, 'utf8') === onDisk);
    await p.click('#dm-restore'); await sleep(1200);
    check('the user project is back byte for byte and Save works again', (await project(p)) === orig && !(await saveOff(p)));
    // a stale marker for another project must not lock Save
    await p.evaluate(() => localStorage.setItem('osatura.demo', '1')); await p.reload(); await sleep(1500);
    check('a stale marker is ignored and cleared', !(await saveOff(p)) && await p.evaluate(() => localStorage.getItem('osatura.demo') === null));
    check('the palette command exists', await p.evaluate(() => typeof CMD['demo.panel']) === 'function');
    check('no page errors or console errors', errs.length === 0, errs);
  } finally { await browser.close(); server.kill(); }
  console.log(`\n${pass} passed, ${fail} failed`);
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
