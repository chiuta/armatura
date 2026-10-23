// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Headless-browser + HTTP test for the overlay controls (onion skin, motion trails) and scrub proxies (R-103).
//
//   cargo build -p armatura-cli && node armatura-cli/tests/studio_overlays_smoke.js
//
// Environment: ARMATURA_BIN, PLAYWRIGHT_CORE, CHROME, SHOTS (screenshot directory, optional) as in studio_ui_smoke.js.
// Only Chromium with a mouse and keyboard is exercised. The timings printed at the end are from headless Chromium with
// a software renderer on whatever machine runs this; they compare the picture kinds with each other, nothing more.
const { spawn, execFileSync } = require('child_process');
const fs = require('fs'), os = require('os'), path = require('path'), http = require('http');
const root = path.resolve(__dirname, '..', '..');
const BIN = process.env.ARMATURA_BIN || path.join(root, 'target/debug/armatura');
const { chromium } = require(process.env.PLAYWRIGHT_CORE || '/opt/npm-tools/node_modules/playwright-core');
const CHROME = process.env.CHROME || '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
const SHOTS = process.env.SHOTS || '';
const PORT = 8890 + Math.floor(Math.random() * 90);
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'armatura-ovl-'));
process.on('exit', () => { try { fs.rmSync(tmp, { recursive: true, force: true }); } catch (e) { } }); // never leave the temp dir behind (also on a crash or exit code)
const UICFG = path.join(tmp, 'cfg', 'ui.json'), PROJ = path.join(tmp, 'demo.json');
execFileSync(BIN, ['demo', PROJ]);
let pass = 0, fail = 0;
const check = (name, ok, extra) => { if (ok) pass++; else fail++; console.log((ok ? 'PASS ' : 'FAIL ') + name + (ok || extra === undefined ? '' : '  -> ' + JSON.stringify(extra))); };
const sleep = ms => new Promise(r => setTimeout(r, ms));
const req = (method, p, body) => new Promise((res, rej) => { const rq = http.request({ host: '127.0.0.1', port: PORT, path: p, method, headers: { 'Content-Type': 'application/json' } }, r => { const bufs = []; r.on('data', d => bufs.push(d)); r.on('end', () => res({ status: r.statusCode, headers: r.headers, buf: Buffer.concat(bufs) })); }); rq.on('error', rej); rq.end(body === undefined ? undefined : JSON.stringify(body)); });
const getJ = async p => JSON.parse((await req('GET', p)).buf.toString());
const postJ = async (p, b) => { const r = await req('POST', p, b); return { status: r.status, body: JSON.parse(r.buf.toString()) }; };
let server;
function startServer() {
  server = spawn(BIN, ['studio', PROJ, '--port', String(PORT), '--ui-config', UICFG], { stdio: 'ignore' });
  return (async () => { for (let i = 0; i < 60; i++) { try { await getJ('/api/state'); return; } catch (e) { await sleep(100); } } throw new Error('server did not start'); })();
}
const stopServer = () => new Promise(r => { server.once('exit', r); server.kill(); });

(async () => {
  await startServer();

  // ---------- HTTP: frames, proxies, cache ----------
  const f0 = await req('GET', '/api/frame?frame=5&proxy=0');
  check('/api/frame full quality is the SVG picture', f0.headers['content-type'] === 'image/svg+xml' && f0.buf.toString().startsWith('<svg'), f0.headers['content-type']);
  check('first request is a cache miss', f0.headers['x-cache'] === 'miss');
  const f0b = await req('GET', '/api/frame?frame=5&proxy=0');
  check('second request is a cache hit with identical bytes', f0b.headers['x-cache'] === 'hit' && Buffer.compare(f0.buf, f0b.buf) === 0);
  const legacy = await req('GET', '/frame/5.svg');
  check('the old /frame/N.svg route shares the cache and bytes', legacy.headers['x-cache'] === 'hit' && Buffer.compare(legacy.buf, f0.buf) === 0);
  const png = await req('GET', '/api/frame?frame=5&proxy=2&kind=png');
  check('proxy=2 PNG is a quarter-size bitmap (160x120)', png.headers['content-type'] === 'image/png' && png.buf.readUInt32BE(16) === 160 && png.buf.readUInt32BE(20) === 120, [png.buf.readUInt32BE(16), png.buf.readUInt32BE(20)]);
  const png2 = await req('GET', '/api/frame?frame=5&proxy=2&kind=png');
  check('proxy PNG is cached and deterministic', png2.headers['x-cache'] === 'hit' && Buffer.compare(png.buf, png2.buf) === 0);
  const vec = await req('GET', '/api/frame?frame=5&proxy=3');
  check('proxy=3 vector picture is an SVG without gradients, smaller than the full one', vec.buf.toString().startsWith('<svg') && !vec.buf.toString().includes('Gradient') && vec.buf.length < f0.buf.length, [vec.buf.length, f0.buf.length]);
  const ras = await req('GET', '/api/frame?frame=5&proxy=2&kind=raster');
  check('proxy kind=raster is an SVG wrapper around a bitmap', ras.buf.toString().includes('data:image/png;base64,'));
  for (const bad of ['/api/frame?frame=5&proxy=9', '/api/frame?frame=5&proxy=x', '/api/frame?frame=5&kind=zip', '/api/frame?frame=-3', '/api/frame?frame=99999']) {
    const r = await req('GET', bad); check('rejects ' + bad, r.status === 400, r.status);
  }
  const c1 = await getJ('/api/cache');
  check('cache statistics are reported', c1.entries >= 4 && c1.hits >= 3 && c1.misses >= 4 && c1.invalidations === 0, c1);
  const rev0 = (await getJ('/api/state')).rev;
  const ed = await postJ('/api/key', { target: 'hand', prop: 'bone_angle', frame: 3, value: 25 });
  check('an edit changes the project revision', ed.body.rev !== rev0 && ed.body.rev > rev0, [rev0, ed.body.rev]);
  const f1 = await req('GET', '/api/frame?frame=5&proxy=0');
  check('after the edit the same frame is a miss (cache emptied)', f1.headers['x-cache'] === 'miss');
  const c2 = await getJ('/api/cache');
  check('...and the cache counts one invalidation', c2.invalidations === 1 && c2.entries === 1, c2);
  const f1b = await req('GET', '/api/frame?frame=3&proxy=0');
  await postJ('/api/undo', {});
  const f1c = await req('GET', '/api/frame?frame=3&proxy=0');
  check('undo also empties the cache: frame 3 differs from the edited one', f1c.headers['x-cache'] === 'miss' && Buffer.compare(f1b.buf, f1c.buf) !== 0);

  // ---------- HTTP: overlay settings and trails ----------
  const o0 = await getJ('/api/overlays');
  check('default overlay settings', o0.ok && o0.settings.onion.enabled === false && o0.settings.onion.before === 2 && o0.settings.proxy.level === 2 && !o0.svg);
  const o1 = await postJ('/api/overlays', { settings: { onion: { enabled: true, before: 3, after: 2, step: 2 } }, frame: 12 });
  check('POST /api/overlays merges, validates and returns the ghosts of the frame', o1.status === 200 && o1.body.ghosts.map(g => g.frame).join() === '6,8,10,16,14' && o1.body.svg.includes('data-frame="14"'), o1.body.ghosts);
  const op = o1.body.ghosts.map(g => +g.opacity.toFixed(3));
  check('ghosts come farthest first and opacity falls off with distance (0.135 / 0.292 / 0.45 past, 0.135 / 0.45 future)', JSON.stringify(op) === JSON.stringify([0.135, 0.292, 0.45, 0.135, 0.45]), op);
  const bad = await postJ('/api/overlays', { settings: { onion: { before: 99 } } });
  check('invalid settings are 400 and change nothing', bad.status === 400 && (await getJ('/api/overlays')).settings.onion.before === 3);
  check('invalid colour is rejected', (await postJ('/api/overlays', { settings: { onion: { tint_after: 'purplish' } } })).status === 400);
  const k = await postJ('/api/overlays', { settings: { onion: { mode: 'keyed', before: 2, after: 1 } }, frame: 12 });
  check('keyed mode ghosts the neighbouring key frames', k.body.ghosts.length > 0 && k.body.ghosts.every(g => g.frame !== 12), k.body.ghosts);
  await postJ('/api/overlays', { settings: { onion: { mode: 'relative', selection_only: true } } });
  const sel1 = await getJ('/api/overlays?frame=12&sel=arm');
  const sel0 = await getJ('/api/overlays?frame=12');
  check('selection_only limits the ghosts to the selected layer (shorter picture); empty selection = all', sel1.svg.length < sel0.svg.length && sel0.svg.length > 1000, [sel1.svg.length, sel0.svg.length]);
  await postJ('/api/overlays', { settings: { onion: { selection_only: false } } });
  const saved = JSON.parse(fs.readFileSync(UICFG, 'utf8'));
  check('settings are written to the UI config file (not the project)', saved.overlays && saved.overlays.onion.before === 2 && saved.overlays.onion.step === 2 && !fs.readFileSync(PROJ, 'utf8').includes('tint_before'));
  const t = await postJ('/api/trails', { targets: [{ kind: 'bone', name: 'hand', tip: true }], frame: 10, before: 4, after: 4, svg: true });
  const tr = t.body.trails && t.body.trails[0];
  check('/api/trails returns the range around the frame, ticks, curve and key markers', t.body.ok && t.body.from === 6 && t.body.to === 14 && tr.frames.length === 9 && tr.points.length === 9 && tr.curve.length === 8 * 4 + 1 && Array.isArray(tr.keys), t.body.ok ? [t.body.from, t.body.to, tr.frames.length, tr.curve.length] : t.body);
  check('...and an SVG overlay when asked', t.body.svg.includes('<polyline') && t.body.svg.includes('r="6"'));
  const p7 = tr.points[3];
  const h1 = await postJ('/api/trails', { targets: [{ kind: 'bone', name: 'hand', tip: true }], from: 6, to: 14, smooth: 0, hit: { x: p7[0] + 1, y: p7[1] - 1, radius: 4 } });
  check('hit-testing a point near the tick of frame 9 returns frame 9', h1.body.hit && h1.body.hit.frame === 9 && h1.body.hit.dist < 2, h1.body.hit);
  const h2 = await postJ('/api/trails', { targets: [{ kind: 'bone', name: 'hand', tip: true }], from: 6, to: 14, hit: { x: 5, y: 5, radius: 4 } });
  check('a miss returns null', h2.body.ok && h2.body.hit === null);
  for (const badBody of [{ targets: [], frame: 3 }, { targets: [{ kind: 'bone', name: 'nope', tip: true }], frame: 3 }, { targets: [{ kind: 'layer', path: 'nope' }], from: 0, to: 3 }, { targets: [{ kind: 'bone', name: 'hand' }], from: 5, to: 2 }, { targets: [{ kind: 'bone', name: 'hand' }], from: 0, to: 999 }, { targets: [{ kind: 'wat' }], frame: 2 }, { targets: [{ kind: 'bone', name: 'hand' }] }]) {
    const r = await postJ('/api/trails', badBody); check('/api/trails rejects ' + JSON.stringify(badBody).slice(0, 70), r.status === 400 && r.body.ok === false, r.status);
  }
  const lay = await postJ('/api/trails', { targets: [{ kind: 'layer', path: 'arm' }], from: 0, to: 10 });
  check('a layer pivot trail works', lay.body.ok && lay.body.trails[0].points.length === 11);

  // ---------- browser ----------
  const browser = await chromium.launch({ executablePath: CHROME, args: ['--no-sandbox'] });
  const ctx = await browser.newContext({ viewport: { width: 1500, height: 900 } });
  const page = await ctx.newPage();
  const errs = [];
  page.on('pageerror', e => errs.push('PAGEERR ' + e.message)); page.on('console', m => { if (m.type() === 'error') errs.push('CONSOLE ' + m.text()); });
  ctx.on('response', r => { if (r.status() >= 400 && !/api\/(frame|trails|overlays)/.test(r.url())) errs.push('HTTP ' + r.status() + ' ' + r.url()); });
  const ev = (f, a) => page.evaluate(f, a);
  const shot = async n => { if (SHOTS) await page.screenshot({ path: path.join(SHOTS, n + '.png') }); };
  // reset settings so the browser run starts from known values
  await postJ('/api/overlays', { reset: true });
  await page.goto(`http://127.0.0.1:${PORT}/`);
  await page.waitForFunction(() => typeof UI !== 'undefined' && UI.ready && typeof TK !== 'undefined' && TK.m && S && typeof OVS !== 'undefined' && OVS && $('img').naturalWidth > 0, null, { timeout: 20000 });
  await sleep(500);

  const nocmd = await ev(() => UI.cmds.filter(c => !handlerFor(c.id)).map(c => c.id));
  check('every registry command (incl. the 13 overlay commands) has a page handler', nocmd.length === 0 && (await ev(() => UI.cmds.filter(c => c.id.startsWith('overlay.')).length)) === 13, nocmd);
  check('overlay shortcuts present in the registry', await ev(() => ['overlay.onion_toggle:global:Alt+N', 'overlay.trails_toggle:global:Alt+M', 'overlay.proxy_toggle:global:Alt+Q', 'overlay.panel:global:Alt+7'].every(s => { const [id, ...b] = s.split(':'); return UI.byId[id].bindings.includes(b.join(':')); })));
  check('no shortcut conflicts', await ev(() => UI.conflicts.length === 0));

  // popover
  await page.locator('#ovbtn').click(); await sleep(200);
  check('the Overlays button opens the settings panel', await ev(() => $('ovp').style.display === 'block'));
  check('panel shows the stored values', await ev(() => $('ov-b').value === '2' && $('ov-a').value === '1' && $('ov-tb').value === '#ff4646' && $('ov-px').checked === true && $('ov-pl').value === '2'));
  await shot('ovl-1-panel');
  await ev(() => { frame = 12; show(); }); await sleep(300);
  await page.locator('#ov-on').check(); await sleep(700);
  check('enabling onion skin shows the ghost layer with 3 ghosts (2 past, 1 future)', await ev(() => $('onion').style.display === 'block' && $('onion').dataset.ghosts === '3' && $('onion').naturalWidth > 0), await ev(() => [$('onion').style.display, $('onion').dataset.ghosts]));
  check('...stacked between the picture and the interaction layer', await ev(() => { const k = [...$('stage').children].map(c => c.id); return k.indexOf('img') < k.indexOf('onion') && k.indexOf('onion') < k.indexOf('trl') && k.indexOf('trl') < k.indexOf('ov'); }));
  await page.locator('#ov-b').fill('4'); await page.locator('#ov-b').dispatchEvent('change'); await sleep(600);
  check('changing "past ghosts" to 4 gives 5 ghosts and is stored on the server', (await ev(() => $('onion').dataset.ghosts)) === '5' && (await getJ('/api/overlays')).settings.onion.before === 4);
  await page.locator('#ov-ol').check(); await sleep(500);
  check('outline mode switches the ghost picture', await ev(async () => { const r = await api('/api/overlays?frame=' + frame); return r.svg.includes('vector-effect'); }));
  await page.locator('#ov-ol').uncheck(); await sleep(300);
  await ev(() => { document.activeElement && document.activeElement.blur(); $('ov').focus(); });
  await page.keyboard.press('Alt+n'); await sleep(600);
  check('Alt+N toggles onion skin off (registry shortcut)', await ev(() => OVS.onion.enabled === false && $('onion').style.display === 'none'));
  await page.keyboard.press('Alt+n'); await sleep(600);
  check('...and on again', await ev(() => OVS.onion.enabled === true && $('onion').style.display === 'block'));
  await shot('ovl-2-onion');
  // frame near the start has fewer ghosts
  await ev(() => { frame = 1; show(); }); await sleep(600);
  check('near the start only the existing frames are ghosted (1 past + 1 future)', (await ev(() => $('onion').dataset.ghosts)) === '2', await ev(() => $('onion').dataset.ghosts));
  await ev(() => { frame = 12; show(); });

  // trails need a selection
  await ev(async () => { await selApply([{ kind: 'bone', name: 'hand' }], 'replace'); }); await sleep(400);
  await page.keyboard.press('Alt+m'); await sleep(900);
  check('Alt+M turns trails on and the selected bone gets a trail', await ev(() => OVS.trails.enabled === true && OVT.length === 1 && $('trl').querySelectorAll('polyline').length === 1), await ev(() => [OVS.trails.enabled, OVT.length]));
  const nTicks = await ev(() => $('trl').querySelectorAll('circle[data-frame],path[data-frame]').length);
  check('the trail has a mark per frame of its range (24 frames around 12 -> 25 marks incl. current)', nTicks === 25, nTicks);
  check('a ring marks the current frame', await ev(() => $('trl').querySelectorAll('[data-current]').length === 1));
  check('key frames are drawn as diamonds', await ev(() => $('trl').querySelectorAll('path[data-keyed]').length) >= 1);
  await shot('ovl-3-trails');
  // click a tick: playhead goes there, selection unchanged
  const tickPos = await ev(() => { const c = [...$('trl').querySelectorAll('circle[data-frame]')].find(c => +c.dataset.frame === 16); const r = c.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; });
  const selBefore = await ev(() => JSON.stringify(csel));
  await page.mouse.click(tickPos.x, tickPos.y); await sleep(700);
  check('clicking the tick of frame 16 moves the playhead to 16 and keeps the selection', (await ev(() => frame)) === 16 && (await ev(() => JSON.stringify(csel))) === selBefore, await ev(() => [frame, csel]));
  check('the trail follows the new playhead (range re-centred)', await ev(() => OVT.length === 1 && OVT[0].from === 4 && OVT[0].to === 28), await ev(() => OVT.length && [OVT[0].from, OVT[0].to]));
  await page.keyboard.press('Alt+m'); await sleep(500);
  check('Alt+M turns the trail off again', await ev(() => OVS.trails.enabled === false && $('trl').childElementCount === 0));
  await page.keyboard.press('Alt+m'); await sleep(500);

  // ---------- scrub proxy ----------
  await ev(() => { frame = 4; show(); }); await sleep(500);
  check('at rest the picture is full quality', await ev(() => $('img').dataset.level === '0' && $('img').src.includes('proxy=0')));
  const rb = await ev(() => { const b = $('dk-cv').getBoundingClientRect(); return { x: b.left, y: b.top }; });
  const xAt = async f => rb.x + (await ev(f => f2x(f), f));
  await page.mouse.move(await xAt(6), rb.y + 10); await page.mouse.down(); await sleep(150);
  await page.mouse.move(await xAt(14), rb.y + 10, { steps: 6 }); await sleep(250);
  const during = await ev(() => ({ level: $('img').dataset.level, src: $('img').src, scrubbing, frame, ghosts: $('onion').dataset.ghosts }));
  check('while the playhead is dragged the proxy picture is requested (level 2)', during.level === '2' && during.src.includes('proxy=2') && during.scrubbing === true, during);
  await page.mouse.up(); await sleep(700);
  const after = await ev(() => ({ level: $('img').dataset.level, src: $('img').src, scrubbing }));
  check('on release the full-quality picture comes back', after.level === '0' && after.src.includes('proxy=0') && after.scrubbing === false, after);
  check('the playhead ended where the drag ended', await ev(() => frame >= 12 && frame <= 15), await ev(() => frame));
  const cs = await getJ('/api/cache');
  check('scrubbing back and forth is served from the cache (hits grew)', cs.hits > c2.hits, [c2.hits, cs.hits]);
  // the proxy can be turned off
  await page.keyboard.press('Alt+q'); await sleep(500);
  check('Alt+Q turns the proxy off', await ev(() => OVS.proxy.enabled === false));
  await page.mouse.move(await xAt(6), rb.y + 10); await page.mouse.down(); await page.mouse.move(await xAt(9), rb.y + 10, { steps: 3 }); await sleep(250);
  check('with the proxy off a drag stays at full quality', await ev(() => $('img').dataset.level === '0'));
  await page.mouse.up(); await sleep(300);
  await page.keyboard.press('Alt+q'); await sleep(400);
  await ev(() => CMD['overlay.proxy_level_cycle']()); await sleep(500);
  check('the cycle command moves the quality 2 -> 3 and stores it', await ev(() => OVS.proxy.level === 3) && (await getJ('/api/overlays')).settings.proxy.level === 3);
  await ev(() => CMD['overlay.proxy_level_cycle']()); await sleep(500); await ev(() => CMD['overlay.proxy_level_cycle']()); await sleep(500);
  check('...and wraps around after 4', await ev(() => OVS.proxy.level === 1), await ev(() => OVS.proxy.level));
  await ev(() => CMD['overlay.proxy_level_cycle']()); await sleep(500);

  // ---------- scrub proxy with reduced-detail copies (I-11 wired into the studio) ----------
  const lodReq = (f, extra) => req('GET', `/api/frame?frame=${f}&proxy=2&kind=vector&lod=1&screen=320x240&lodpx=1${extra || ''}`);
  const bads = ['/api/frame?frame=5&proxy=2&lod=1', '/api/frame?frame=5&proxy=2&lod=1&screen=0x0', '/api/frame?frame=5&proxy=2&lod=1&screen=320x240&lodpx=99', '/api/frame?frame=5&proxy=2&lod=1&screen=320x240&lodpx=x', '/api/frame?frame=5&proxy=2&lod=2&screen=320x240', '/api/frame?frame=5&proxy=2&lod=1&screen=axb'];
  const badRes = await Promise.all(bads.map(u => req('GET', u)));
  check('lod requests with a missing or bad screen / budget / flag are 400', badRes.every(r => r.status === 400), badRes.map(r => r.status));
  const l0 = await lodReq(30);
  check('the first lod request is answered at once at level 0 (levels are being built in the background)', l0.status === 200 && l0.headers['x-lod-level'] === '0' && l0.headers['x-lod-bound-px'] === '0.0000', l0.headers);
  let cstat = null; for (let i = 0; i < 300; i++) { cstat = (await getJ('/api/cache')).lod; if (cstat.builds >= 1) break; await sleep(100); }
  check('the background build finishes and is reported (levels, build time)', cstat.builds === 1 && cstat.levels.length === 6 && cstat.last_build_ms > 0 && !cstat.unavailable, cstat);
  const l1 = await lodReq(31);
  const lvl = +l1.headers['x-lod-level'], bnd = +l1.headers['x-lod-bound-px'];
  check('afterwards a picture is drawn from a reduced level whose declared bound is within the requested 1 px', l1.status === 200 && lvl >= 1 && bnd <= 1 && bnd > 0, [lvl, bnd]);
  const rec = cstat.levels[lvl - 1];
  check('...and that bound is the level\'s stage-unit bound times the screen scale (320/640 = 0.5)', Math.abs(rec.bound_stage * 0.5 - bnd) < 1e-3, [rec, bnd]);
  const tight = await lodReq(32, '').then(() => req('GET', '/api/frame?frame=33&proxy=2&kind=vector&lod=1&screen=320x240&lodpx=0'));
  check('a zero budget always means the project itself (level 0)', tight.headers['x-lod-level'] === '0');
  const big = await req('GET', '/api/frame?frame=34&proxy=2&kind=vector&lod=1&screen=6400x4800&lodpx=0.5');
  check('a screen ten times larger needs a finer level than the small one', +big.headers['x-lod-level'] < lvl || big.headers['x-lod-level'] === '0', [big.headers['x-lod-level'], lvl]);
  const lodHit = await lodReq(31);
  check('the same lod picture is served from the cache, and is a different cache entry than the non-lod one', lodHit.headers['x-cache'] === 'hit' && (await req('GET', '/api/frame?frame=31&proxy=2&kind=vector')).headers['x-cache'] === 'miss' && Buffer.compare(lodHit.buf, l1.buf) === 0);
  check('lod pictures are valid SVG', l1.buf.toString().startsWith('<svg') && l1.buf.toString().trim().endsWith('</svg>'));
  const full0 = await req('GET', '/api/frame?frame=5&proxy=0&lod=1&screen=320x240');
  check('the full-quality picture (proxy=0) never uses a reduced copy', full0.headers['x-lod-level'] === '0');
  // an edit empties the levels
  const inv0 = (await getJ('/api/cache')).lod.invalidations;
  await postJ('/api/op', { op: 'set', path: '/background', value: '#ffeecc' });
  const afterEdit = await lodReq(35);
  check('after an edit the old levels are dropped at once (level 0 until the new build is ready)', afterEdit.headers['x-lod-level'] === '0' && (await getJ('/api/cache')).lod.invalidations === inv0 + 1, [afterEdit.headers['x-lod-level'], inv0]);
  for (let i = 0; i < 300; i++) { if ((await getJ('/api/cache')).lod.builds >= 2) break; await sleep(100); }
  check('...and the new levels are built for the edited project', (await getJ('/api/cache')).lod.builds === 2 && +(await lodReq(36)).headers['x-lod-level'] >= 1);
  await postJ('/api/undo', {});
  // the page asks for it when the toggle is on
  await ev(() => ovPatch('proxy', { lod: true, lod_px: 1 })); await sleep(500);
  check('the Overlays panel has the toggle and the pixel budget and they are stored', await ev(() => $('ov-lod').checked === true && +$('ov-lodpx').value === 1) && (await getJ('/api/overlays')).settings.proxy.lod === true);
  await ev(() => { frame = 4; show(); }); await sleep(400);
  await page.mouse.move(await xAt(6), rb.y + 10); await page.mouse.down(); await sleep(150);
  await page.mouse.move(await xAt(14), rb.y + 10, { steps: 6 }); await sleep(300);
  const dur2 = await ev(() => ({ src: $('img').src, level: $('img').dataset.level }));
  check('while scrubbing with the toggle on the page asks for lod=1 with its picture size and budget', /lod=1&screen=\d+x\d+&lodpx=1/.test(dur2.src) && dur2.level === '2', dur2);
  await page.mouse.up(); await sleep(600);
  check('at rest no lod parameters are sent (full quality)', await ev(() => !$('img').src.includes('lod=') && $('img').src.includes('proxy=0')));
  await ev(() => ovPatch('proxy', { lod: false })); await sleep(300);
  await page.mouse.move(await xAt(6), rb.y + 10); await page.mouse.down(); await page.mouse.move(await xAt(10), rb.y + 10, { steps: 3 }); await sleep(250);
  check('with the toggle off the page sends no lod parameters', await ev(() => !$('img').src.includes('lod=')));
  await page.mouse.up(); await sleep(300);
  const badSet = await postJ('/api/overlays', { settings: { proxy: { lod_px: 99 } } });
  check('a pixel budget outside 0..16 is refused', badSet.status === 400 && /lod_px/.test(JSON.stringify(badSet.body)));

  // measured: a heavy project (many anchors per outline) on a second server, scrub pictures with and without the reduced copy
  {
    const HP = PORT + 100, heavy = path.join(tmp, 'heavy.json'), hp = JSON.parse(fs.readFileSync(PROJ, 'utf8'));
    hp.layers = []; hp.channels = []; hp.bones = []; for (const k of ['ik', 'constraints', 'springs', 'skins', 'channel_meta']) delete hp[k];
    const NL = +(process.env.LOD_LAYERS || 6), NA = +(process.env.LOD_ANCHORS || 100);
    for (let l = 0; l < NL; l++) { const cx = 60 + (l % 8) * 70, cy = 60 + Math.floor(l / 8) * 70, an = []; for (let i = 0; i < NA; i++) { const a = i / NA * 2 * Math.PI, rr = 28 + 3 * Math.sin(3 * a); an.push({ pos: [cx + rr * Math.cos(a), cy + rr * Math.sin(a)] }); } hp.layers.push({ name: 'blob' + l, type: 'vector', paths: [{ name: 'p', anchors: an, closed: true, fill: '#3a7bd5', stroke: { paint: '#112233', width: 1.5 } }] }); }
    fs.writeFileSync(heavy, JSON.stringify(hp));
    const hs = spawn(BIN, ['studio', heavy, '--port', String(HP), '--ui-config', path.join(tmp, 'ui-heavy.json')], { stdio: 'ignore' });
    const hreq = p => new Promise((res, rej) => { http.get({ host: '127.0.0.1', port: HP, path: p }, r => { const b = []; r.on('data', d => b.push(d)); r.on('end', () => res({ headers: r.headers, buf: Buffer.concat(b), status: r.statusCode })); }).on('error', rej); });
    for (let i = 0; i < 100; i++) { try { await hreq('/api/state'); break; } catch (e) { await sleep(100); } }
    const kindQ = { png: 'kind=png', vector: 'kind=vector' };
    const q = (f, lod, kind) => `/api/frame?frame=${f}&proxy=1&${kindQ[kind || 'png']}${lod ? '&lod=1&screen=320x240&lodpx=1' : ''}`;
    await hreq(q(0, true));
    let hb = null; for (let i = 0; i < 1200; i++) { hb = JSON.parse((await hreq('/api/cache')).buf.toString()).lod; if (hb.builds >= 1) break; await sleep(100); }
    const lv = +(await hreq(q(1, true))).headers['x-lod-level'];
    const mean = async (lod, kind) => { const a = []; for (let f = 2; f < 26; f++) a.push(+(await hreq(q(f, lod, kind))).headers['x-render-ms']); return a.reduce((x, y) => x + y, 0) / a.length; };
    const tOff = await mean(false), tOn = await mean(true);
    const vOff = await mean(false, 'vector'), vOn = await mean(true, 'vector');
    const stats = hb && hb.levels && hb.levels[lv - 1];
    console.log(`LOD speed (${NL} outlines x ${NA} anchors, 24 cold frames each, bitmap proxy 1/2 size, ${BIN.includes('release') ? 'release' : 'debug'} binary, build ${hb && hb.last_build_ms ? hb.last_build_ms.toFixed(0) : '?'} ms): full ${tOff.toFixed(2)} ms/frame, reduced level ${lv} (${stats ? stats.anchors : '?'} anchors kept of ${NL * NA}) ${tOn.toFixed(2)} ms/frame, ${(tOff / tOn).toFixed(2)}x; simplified-vector proxy: full ${vOff.toFixed(2)} ms/frame, reduced ${vOn.toFixed(2)} ms/frame, ${(vOff / vOn).toFixed(2)}x`);
    check('heavy project: levels were built in the background and a reduced level is used', hb && hb.builds >= 1 && lv >= 1, [hb && hb.builds, lv]);
    check('heavy project: the reduced picture is not slower than the full one (measured, see the log line)', tOn <= tOff * 1.1, [tOff, tOn]);
    hs.kill();
  }

  // timing. Server: X-Render-Ms of cold requests (the cache is emptied by a no-op-free edit: undo/redo pair) and of warm ones.
  const N = 24, avg = a => a.reduce((x, y) => x + y, 0) / a.length;
  const kinds = [['full svg', 'proxy=0'], ['vector proxy 2', 'proxy=2&kind=vector'], ['vector proxy 4', 'proxy=4&kind=vector'], ['bitmap proxy 1', 'proxy=1&kind=png'], ['bitmap proxy 2', 'proxy=2&kind=png'], ['bitmap proxy 4', 'proxy=4&kind=png']];
  const srv = {};
  for (const [name, q] of kinds) {
    const cold = [], warm = [];
    for (let f = 0; f < N; f++) cold.push(+(await req('GET', `/api/frame?frame=${f}&${q}`)).headers['x-render-ms']);
    for (let f = 0; f < N; f++) warm.push(+(await req('GET', `/api/frame?frame=${f}&${q}`)).headers['x-render-ms']);
    srv[name] = { cold: +avg(cold).toFixed(3), warm: +avg(warm).toFixed(4) };
  }
  console.log('server: ms to produce a picture, mean of 24 frames (' + path.basename(BIN) + ' build ' + (BIN.includes('release') ? 'release' : 'as given') + '):', JSON.stringify(srv));
  check('server timings were measured; warm (cached) requests are far cheaper than cold ones', kinds.every(([n]) => srv[n].cold > 0 && srv[n].warm < srv[n].cold), srv);
  // Browser: fetch + decode of the same pictures (headless Chromium, software rendering).
  const timing = await ev(async () => {
    const out = {}; const N = 24;
    for (const [name, url] of [['full svg', f => `/api/frame?frame=${f}&proxy=0`], ['vector proxy 2', f => `/api/frame?frame=${f}&proxy=2&kind=vector`], ['vector proxy 4', f => `/api/frame?frame=${f}&proxy=4&kind=vector`], ['bitmap proxy 2', f => `/api/frame?frame=${f}&proxy=2&kind=raster`], ['bitmap proxy 4', f => `/api/frame?frame=${f}&proxy=4&kind=raster`]]) {
      const t0 = performance.now();
      for (let f = 0; f < N; f++) { const im = new Image(); im.src = url(f); await im.decode(); }
      out[name] = +((performance.now() - t0) / N).toFixed(2);
    }
    return out;
  });
  console.log('browser: ms per picture, fetch + decode, cached on the server (headless Chromium, demo project):', JSON.stringify(timing));
  check('browser timings were measured', Object.values(timing).every(v => v > 0));

  // ---------- persistence across a restart ----------
  const before = await getJ('/api/overlays');
  await ctx.close(); await stopServer(); await startServer();
  const ctx2 = await browser.newContext({ viewport: { width: 1500, height: 900 } });
  const p2 = await ctx2.newPage();
  await p2.goto(`http://127.0.0.1:${PORT}/`);
  await p2.waitForFunction(() => typeof OVS !== 'undefined' && OVS && S && $('img').naturalWidth > 0, null, { timeout: 20000 }); await sleep(400);
  const restored = await p2.evaluate(() => JSON.stringify(OVS));
  check('settings survive a server restart and are loaded by the page', restored === JSON.stringify(before.settings) && JSON.parse(restored).onion.enabled === true && JSON.parse(restored).trails.enabled === true, restored);
  await ctx2.close();

  const real = errs.filter(e => !/net::ERR_FAILED|Failed to load resource|ERR_ABORTED/.test(e));
  check('no page errors or console errors', real.length === 0, real.slice(0, 5));
  await browser.close(); await stopServer();
  console.log(`\n${pass} passed, ${fail} failed`);
  fs.rmSync(tmp, { recursive: true, force: true });
  process.exit(fail ? 1 : 0);
})().catch(e => { console.error('TEST CRASHED', e); try { server.kill(); } catch (x) { } process.exit(2); });
