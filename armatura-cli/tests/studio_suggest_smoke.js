// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Headless-browser + HTTP test for the pose suggestion panel and POST /api/suggest (R-112).
//
//   cargo build -p armatura-cli && node armatura-cli/tests/studio_suggest_smoke.js
//
// Environment: ARMATURA_BIN, PLAYWRIGHT_CORE, CHROME as in studio_ui_smoke.js. Only Chromium with a mouse and a
// keyboard is exercised. The suggestions are classical (PCA, kernels); the test checks wiring and numbers on the
// small demo library, not how useful the suggestions are to an animator.
const { spawn, execFileSync } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
const root = path.resolve(__dirname, '..', '..');
const BIN = process.env.ARMATURA_BIN || path.join(root, 'target/debug/armatura');
const { chromium } = require(process.env.PLAYWRIGHT_CORE || '/opt/npm-tools/node_modules/playwright-core');
const CHROME = process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
const PORT = 8990 + Math.floor(Math.random() * 90);
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'armatura-sgs-'));
process.on('exit', () => { try { fs.rmSync(tmp, { recursive: true, force: true }); } catch (e) { } }); // never leave the temp dir behind (also on a crash or exit code)
const UICFG = path.join(tmp, 'cfg', 'ui.json'), PROJ = path.join(tmp, 'demo.json');
execFileSync(BIN, ['demo', PROJ]);
let pass = 0, fail = 0;
const check = (name, ok, extra) => { if (ok) pass++; else fail++; console.log((ok ? 'PASS ' : 'FAIL ') + name + (ok || extra === undefined ? '' : '  -> ' + JSON.stringify(extra))); };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const req = (method, p, body) => new Promise((res, rej) => { const rq = http.request({ host: '127.0.0.1', port: PORT, path: p, method, headers: { 'Content-Type': 'application/json' } }, r => { const bufs = []; r.on('data', d => bufs.push(d)); r.on('end', () => res({ status: r.statusCode, buf: Buffer.concat(bufs) })); }); rq.on('error', rej); rq.end(body === undefined ? undefined : JSON.stringify(body)); });
const getJ = async p => JSON.parse((await req('GET', p)).buf.toString());
const postJ = async (p, b) => { const r = await req('POST', p, b); return { status: r.status, body: JSON.parse(r.buf.toString()) }; };
let server;
function startServer() {
  server = spawn(BIN, ['studio', PROJ, '--port', String(PORT), '--ui-config', UICFG], { stdio: 'ignore' });
  return (async () => { for (let i = 0; i < 60; i++) { try { await getJ('/api/state'); return; } catch (e) { await sleep(100); } } throw new Error('server did not start'); })();
}
const stopServer = () => new Promise(r => { server.once('exit', r); server.kill(); });
const track = async (target) => { const p = await getJ('/api/project'); const c = p.channels.find(c => c.target === target && c.prop === 'bone_angle'); return c ? c.track.keys.map(k => [k.frame, k.value]) : null; };

(async () => {
  await startServer();

  // ---------- HTTP ----------
  const b = await postJ('/api/suggest', { kind: 'between', a: 0, b: 12, t: 0.5, top: 4 });
  check('between returns ranked candidates', b.status === 200 && b.body.ok && b.body.suggestions.candidates.length >= 3 && b.body.suggestions.candidates.every((c, i, a) => i === 0 || a[i - 1].score >= c.score), b.body);
  const c0 = b.body.suggestions.candidates[0];
  check('candidate carries bone values, a key frame, scores', c0.values.length === 3 && Math.abs(c0.frame - 6) < 1e-6 && c0.score > 0 && c0.plausibility > 0, c0);
  check('the response names the library size and PCA axes', b.body.suggestions.poses_in_library >= 4 && b.body.suggestions.pca_axes >= 1, b.body.suggestions);
  const cm = await postJ('/api/suggest', { kind: 'complete', frame: 5, constraints: [{ kind: 'angle', bone: 'arm_low', value: 30 }] });
  const pls = cm.body.suggestions.candidates.find(c => c.method === 'pca-ls');
  check('complete hits a fixed angle', cm.body.ok && pls && Math.abs(pls.values.find(v => v.bone === 'arm_low').angle - 30) < 0.5, cm.body);
  const rc = await postJ('/api/suggest', { kind: 'complete', frame: 5, constraints: [{ kind: 'reach', bone: 'hand', at: [300, 200] }] });
  check('complete with a tip target reports its residual', rc.body.ok && typeof rc.body.suggestions.candidates[0].constraint_error === 'number', rc.body);
  const nx = await postJ('/api/suggest', { kind: 'next', frame: 10, top: 3 });
  check('next returns candidates keyed after the current frame', nx.body.ok && nx.body.suggestions.candidates.length >= 1 && nx.body.suggestions.candidates[0].frame > 10, nx.body);
  const again = await postJ('/api/suggest', { kind: 'between', a: 0, b: 12, t: 0.5, top: 4 });
  check('same request, same answer (deterministic)', JSON.stringify(again.body) === JSON.stringify(b.body));
  for (const [name, body] of [['unknown kind', { kind: 'dance' }], ['missing frame', { kind: 'next' }], ['negative frame', { kind: 'next', frame: -1 }], ['t out of range', { kind: 'between', a: 0, b: 6, t: 3 }], ['no constraints', { kind: 'complete', frame: 1, constraints: [] }], ['unknown bone', { kind: 'complete', frame: 1, constraints: [{ kind: 'angle', bone: 'zzz', value: 1 }] }], ['bad json shape', { kind: 'between', a: 'x' }]]) {
    const r = await postJ('/api/suggest', body);
    check('rejects ' + name, r.status === 400 && r.body.ok === false && r.body.errors.length > 0, r);
  }
  const lib = { takes: [{ name: 'wave', length: 8, tracks: [{ target: 'arm_up', prop: 'bone_angle', keys: [{ frame: 0, value: -20, ease: { type: 'linear' } }, { frame: 4, value: 40, ease: { type: 'linear' } }, { frame: 8, value: -20, ease: { type: 'linear' } }] }] }] };
  const withLib = await postJ('/api/suggest', { kind: 'between', a: 0, b: 12, library: lib });
  check('an inline motion library adds poses', withLib.body.ok && withLib.body.suggestions.poses_in_library === b.body.suggestions.poses_in_library + 3, [withLib.body.suggestions && withLib.body.suggestions.poses_in_library, b.body.suggestions.poses_in_library]);

  // ---------- apply, undo, locks ----------
  const before = await track('arm_low');
  const rev0 = (await getJ('/api/state')).rev;
  const ap = await postJ('/api/suggest', { kind: 'complete', frame: 30, constraints: [{ kind: 'angle', bone: 'arm_low', value: 33 }], apply: 0 });
  const after = await track('arm_low');
  check('apply keys the candidate as one edit', ap.body.ok && ap.body.state && ap.body.state.rev > rev0 && after.some(k => k[0] === 30), [ap.body.ok, after]);
  const hasKey = after.find(k => k[0] === 30);
  check('the applied value matches the constraint', hasKey && Math.abs(hasKey[1] - 33) < 0.6, hasKey);
  await postJ('/api/undo', {});
  const undone = await track('arm_low');
  check('one undo reverts the whole suggestion', JSON.stringify(undone) === JSON.stringify(before), [before, undone]);
  const bad = await postJ('/api/suggest', { kind: 'next', frame: 10, apply: 99 });
  check('applying a missing candidate is an error', bad.body.ok === false, bad.body);
  // lock arm_low and ask again: it is skipped and reported
  const st = await getJ('/api/project');
  const lockOp = { op: 'set', path: '/channel_meta', value: [{ target: 'arm_low', prop: 'bone_angle', lock: true }] };
  const lk = await postJ('/api/op', lockOp);
  check('lock op accepted', lk.body.ok !== false, lk.body);
  const lockedR = await postJ('/api/suggest', { kind: 'between', a: 0, b: 12 });
  check('locked bones are left out and reported', lockedR.body.ok && lockedR.body.suggestions.locked_skipped.includes('arm_low') && lockedR.body.suggestions.candidates.every(c => c.values.every(v => v.bone !== 'arm_low')), lockedR.body);
  const lockedApply = await postJ('/api/suggest', { kind: 'between', a: 0, b: 12, apply: 0 });
  const t2 = await track('arm_low');
  check('applying with a lock never writes the locked channel', lockedApply.body.ok && JSON.stringify(t2) === JSON.stringify(before), t2);
  await postJ('/api/undo', {}); await postJ('/api/undo', {});

  // ---------- browser ----------
  const browser = await chromium.launch({ executablePath: CHROME, args: ['--no-sandbox'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', e => errs.push('PAGEERR ' + e.message));
  page.on('console', m => { if (m.type() === 'error') errs.push('CONSOLE ' + m.text()); });
  await page.goto(`http://127.0.0.1:${PORT}/`);
  await page.waitForFunction(() => typeof UI !== 'undefined' && UI.ready && typeof S !== 'undefined' && S && $('img').naturalWidth > 0, null, { timeout: 20000 }); await sleep(400);
  check('suggest command is in the registry with a handler', await page.evaluate(() => !!UI.byId['suggest.panel'] && UI.byId['suggest.panel'].bindings.includes('global:Alt+8') && typeof handlerFor('suggest.panel') === 'function'));
  check('panel is closed at start', await page.evaluate(() => getComputedStyle($('sgp')).display === 'none'));
  await page.click('#sgbtn');
  check('the Suggest button opens the panel', await page.evaluate(() => getComputedStyle($('sgp')).display === 'block'));
  await page.click('#sg-go'); await page.waitForSelector('#sg-out .cand', { timeout: 10000 });
  const n = await page.locator('#sg-out .cand').count();
  check('between candidates are listed with Apply buttons', n >= 3 && (await page.locator('#sg-out .sg-apply').count()) === n, n);
  const info = await page.textContent('#sg-info');
  check('library size is shown', /\d+ poses, \d+ axes/.test(info), info);
  await page.selectOption('#sg-kind', 'complete');
  check('complete shows the constraint box and hides the frame pair', await page.evaluate(() => $('sg-cons').style.display !== 'none' && $('sg-between').style.display === 'none'));
  await page.fill('#sg-ta', 'arm_low=30\nhand@300,200'); await page.fill('#sg-f', '5');
  await page.click('#sg-go'); await page.waitForFunction(() => $('sg-out').textContent.includes('constraint error'), null, { timeout: 10000 });
  check('complete lists candidates with constraint errors', (await page.textContent('#sg-out')).includes('constraint error'));
  await page.fill('#sg-ta', 'this is not a constraint'); await page.click('#sg-go'); await page.waitForSelector('#sg-err');
  check('a malformed constraint shows an error and sends nothing', (await page.textContent('#sg-err')).includes('cannot read constraint'));
  await page.fill('#sg-ta', 'arm_low=41'); await page.fill('#sg-f', '31'); await page.click('#sg-go'); await page.waitForSelector('#sg-out .cand');
  const rev1 = (await getJ('/api/state')).rev;
  await page.click('#sg-out .cand:first-of-type .sg-apply');
  await page.waitForFunction(r => S && S.rev > r, rev1, { timeout: 10000 });
  const t3 = await track('arm_low');
  const k31 = t3.find(k => k[0] === 31);
  check('Apply in the page edits the project (key at the chosen frame)', k31 && Math.abs(k31[1] - 41) < 0.8, t3);
  await page.click('#undo'); await sleep(300);
  check('Undo button removes it', !(await track('arm_low')).some(k => k[0] === 31));
  await page.keyboard.press('Escape'); await page.mouse.click(40, 600); await sleep(100);
  check('clicking outside closes the panel', await page.evaluate(() => getComputedStyle($('sgp')).display === 'none'));
  await page.keyboard.press('Alt+8'); await sleep(150);
  check('Alt+8 opens it again', await page.evaluate(() => getComputedStyle($('sgp')).display === 'block'));
  const real = errs.filter(e => !/net::ERR_FAILED|Failed to load resource|ERR_ABORTED/.test(e));
  check('no page errors or console errors', real.length === 0, real.slice(0, 5));
  await browser.close(); await stopServer();
  console.log(`\n${pass} passed, ${fail} failed`);
  fs.rmSync(tmp, { recursive: true, force: true });
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error('TEST CRASHED', e); try { server.kill(); } catch (x) { } process.exit(2); });
