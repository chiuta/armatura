// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Real-browser check of the release wasm module (WASM.md checklist steps 4, 6, 8, 9, 11, 13).
//   node osatura-wasm/js/browser-check.js <package-dir> <project.json> [native-frame-12.png.rgba]
// <package-dir> is the unpacked osatura-web release package (osatura_wasm.wasm + osatura.js + index.html) or the
// osatura-wasm folder after build-wasm.sh. Runs Chromium through Playwright (PLAYWRIGHT_CORE, default
// /opt/npm-tools/node_modules/playwright-core). Prints one JSON object; exits 1 when a hard check fails.
const http = require('http'), fs = require('fs'), path = require('path'), zlib = require('zlib');
const { chromium } = require(process.env.PLAYWRIGHT_CORE || '/opt/npm-tools/node_modules/playwright-core');
const [dir, projectPath, nativeRgba] = process.argv.slice(2);
if (!dir || !projectPath) { console.error('usage: node browser-check.js <package-dir> <project.json> [native.rgba]'); process.exit(2); }
const types = { '.js': 'text/javascript', '.wasm': 'application/wasm', '.html': 'text/html', '.json': 'application/json' };
const srv = http.createServer((q, r) => {
  const f = path.join(dir, decodeURIComponent(q.url.split('?')[0]).replace(/^\/+/, '') || 'index.html');
  if (!f.startsWith(path.resolve(dir)) || !fs.existsSync(f) || fs.statSync(f).isDirectory()) { r.writeHead(404); return r.end('no'); }
  r.writeHead(200, { 'Content-Type': types[path.extname(f)] || 'application/octet-stream' }); r.end(fs.readFileSync(f));
});
(async () => {
  await new Promise(ok => srv.listen(0, '127.0.0.1', ok));
  const base = `http://127.0.0.1:${srv.address().port}/`;
  const wasm = fs.readFileSync(path.join(dir, 'osatura_wasm.wasm'));
  const out = { wasm_bytes: wasm.length, wasm_gzip_bytes: zlib.gzipSync(wasm).length, wasm_brotli_bytes: zlib.brotliCompressSync(wasm).length };
  const browser = await chromium.launch();
  const page = await browser.newPage();
  const errors = []; page.on('pageerror', e => errors.push(String(e)));
  await page.route('**/blank.html', r => r.fulfill({ contentType: 'text/html', body: '<!doctype html><title>x</title>' }));
  await page.goto(base + 'blank.html');
  const project = fs.readFileSync(projectPath, 'utf8');
  const res = await page.evaluate(async ({ base, project }) => {
    let mod;
    try { mod = await import(base + 'osatura.js'); } catch (e) { mod = await import(base + 'js/osatura.js'); }
    const { Osatura } = mod;
    const x = await Osatura.instantiate(base + 'osatura_wasm.wasm');
    const r = {};
    const a = await Osatura.create(x, project);
    r.size = [a.width, a.height, a.frames, a.fps];
    // 13: performance
    const t = [];
    for (let rep = 0; rep < 3; rep++) for (let f = 0; f < a.frames; f++) { const t0 = performance.now(); a.rgba(f, 1); t.push(performance.now() - t0); }
    t.sort((p, q) => p - q);
    r.rgba_ms = { median: +t[t.length >> 1].toFixed(2), p95: +t[Math.floor(t.length * 0.95)].toFixed(2), max: +t[t.length - 1].toFixed(2) };
    // 6: pixels of frame 12 (for the native comparison done by the caller)
    const px = a.rgba(12, 1);
    r.frame12 = { width: px.width, height: px.height };
    r.frame12_data = Array.from(px.data);
    // 8: hostile inputs must throw Error, not trap
    const rnd = new Uint8Array(5 * 1024 * 1024); for (let i = 0; i < rnd.length; i += 65536) crypto.getRandomValues(rnd.subarray(i, Math.min(rnd.length, i + 65536)));
    const bad = { empty: '', junk: 'not json', array: '[]', random5mb: new TextDecoder('latin1').decode(rnd), noframes: '{"format":1,"name":"x","width":10,"height":10,"fps":24,"frames":0}' };
    r.hostile = {};
    for (const [k, v] of Object.entries(bad)) {
      try { await Osatura.create(x, v); r.hostile[k] = 'accepted'; } catch (e) { r.hostile[k] = (e && e.constructor && e.constructor.name) + ': ' + String(e.message || e).slice(0, 80); }
    }
    // 9: large renders
    r.large = {};
    try { const big = a.rgba(0, 4); r.large.scale4 = [big.width, big.height]; } catch (e) { r.large.scale4 = 'error: ' + e.message; }
    try {
      const p = JSON.parse(project); p.width = 12000; p.height = 12000;
      const h = await Osatura.create(x, JSON.stringify(p)); h.rgba(0, 1); r.large.stage12000 = 'rendered (not refused)'; h.close();
    } catch (e) { r.large.stage12000 = (e.constructor.name) + ': ' + String(e.message).slice(0, 90); }
    // after the large render the wrapper must still work
    try { a.rgba(1, 1); r.large.after = 'ok'; } catch (e) { r.large.after = 'broken: ' + e.message; }
    // 11: open / close leak check
    const mem0 = x.memory.buffer.byteLength;
    for (let i = 0; i < 60; i++) { const h = await Osatura.create(x, project); h.rgba(i % a.frames, 1); h.close(); }
    r.leak = { memory_before: mem0, memory_after_60_open_close: x.memory.buffer.byteLength };
    a.close();
    return r;
  }, { base, project });
  out.browser = res;
  out.page_errors = errors;
  out.chromium = browser.version();
  if (nativeRgba && fs.existsSync(nativeRgba)) {
    const nat = fs.readFileSync(nativeRgba), got = Buffer.from(res.frame12_data);
    let sum = 0, max = 0, n = Math.min(nat.length, got.length);
    for (let i = 0; i < n; i++) { const d = Math.abs(nat[i] - got[i]); sum += d; if (d > max) max = d; }
    out.native_compare = { bytes_native: nat.length, bytes_wasm: got.length, mean_abs_diff: +(sum / n).toFixed(4), max_abs_diff: max };
  }
  delete res.frame12_data;
  console.log(JSON.stringify(out, null, 2));
  await browser.close(); srv.close();
  const hostileBad = Object.values(res.hostile).some(v => v === 'accepted' || /RuntimeError/.test(v));
  process.exit(errors.length || hostileBad ? 1 : 0);
})().catch(e => { console.error(e); process.exit(1); });
