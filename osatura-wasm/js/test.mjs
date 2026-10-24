// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
// Node test. Part 1: wrapper against a JS mock of the exports (always runs).
// Part 2: against the real osatura_wasm.wasm if it exists (it was never built in the authoring environment).
import { Osatura } from './osatura.js';
import { existsSync, readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

function mockExports() {
  const memory = new WebAssembly.Memory({ initial: 4 });
  let next = 4096, err = '', out = new Uint8Array(0), ow = 0, oh = 0, open = new Set();
  const enc = new TextEncoder();
  const setErr = (s) => { err = s; const b = enc.encode(s); new Uint8Array(memory.buffer).set(b, 60000); };
  return {
    memory,
    arm_alloc: (n) => { if (!n) { setErr('bad length'); return 0; } const p = next; next += n + 8; return p; },
    arm_free: () => {},
    arm_open: (p, n) => {
      const t = new TextDecoder().decode(new Uint8Array(memory.buffer, p, n));
      try { JSON.parse(t); } catch (e) { setErr('invalid json'); return 0; }
      open.add(1); return 1;
    },
    arm_close: (h) => open.delete(h),
    arm_frame_count: () => 24, arm_fps: () => 12, arm_width: () => 8, arm_height: () => 4,
    arm_error_ptr: () => 60000, arm_error_len: () => err.length,
    arm_render_svg: (h, f) => { if (Number.isNaN(f)) { setErr('frame must be finite'); return -1; } out = enc.encode('<svg>' + f + '</svg>'); const p = 20000; new Uint8Array(memory.buffer).set(out, p); return out.length; },
    arm_render_rgba: (h, f, s) => { ow = Math.round(8 * s); oh = Math.round(4 * s); out = new Uint8Array(ow * oh * 4).fill(7); new Uint8Array(memory.buffer).set(out, 30000); return out.length; },
    arm_out_ptr: () => (out.length && out[0] === 7 ? 30000 : 20000), arm_out_len: () => out.length,
    arm_out_width: () => ow, arm_out_height: () => oh,
  };
}

const a = await Osatura.create(mockExports(), { name: 'x' });
assert.equal(a.frames, 24); assert.equal(a.fps, 12);
assert.equal(a.svg(3), '<svg>3</svg>');
const r = a.rgba(0, 2);
assert.equal(r.width, 16); assert.equal(r.height, 8); assert.equal(r.data.length, 16 * 8 * 4); assert.equal(r.data[0], 7);
assert.throws(() => a.svg(NaN), /finite/);
assert.throws(() => a.load('{ nope'), /invalid json/);
assert.equal(a.svg(1), '<svg>1</svg>', 'old project still usable after failed load');
a.close();
{
  // the wrapper feeds the module clock when the module has one (and works with modules that do not)
  const calls = [];
  const x = mockExports();
  x.arm_set_clock = (mono, unix) => calls.push([mono, unix]);
  const c = await Osatura.create(x, { name: 'x' });
  c.svg(1); c.rgba(0, 1);
  assert.ok(calls.length >= 3, 'load, svg and rgba each set the clock');
  assert.ok(calls.every(([m, u]) => Number.isFinite(m) && m >= 0 && u > 1.6e12), 'performance.now() and Date.now()');
  c.close();
}
// play(): an engine error in a frame stops the loop and goes to onError (it used to die silently inside requestAnimationFrame).
{
  const cbs = [];
  const g = globalThis, old = { raf: g.requestAnimationFrame, caf: g.cancelAnimationFrame, id: g.ImageData };
  g.requestAnimationFrame = (f) => cbs.push(f); g.cancelAnimationFrame = () => {}; g.ImageData = class { constructor(d, w, h) { this.data = d; this.width = w; this.height = h; } };
  const cv = { width: 0, height: 0, getContext: () => ({ putImageData() {} }) };
  const b = await Osatura.create(mockExports(), { name: 'x' });
  const errs = [];
  b.play(cv, { onError: (e) => errs.push(e.message) });
  cbs.shift()(0);
  assert.equal(errs.length, 0); assert.equal(cbs.length, 1);
  b.rgba = () => { throw new Error('boom'); };
  cbs.shift()(16);
  assert.deepEqual(errs, ['boom']); assert.equal(cbs.length, 0); assert.equal(b._raf, null);
  Object.assign(g, { requestAnimationFrame: old.raf, cancelAnimationFrame: old.caf, ImageData: old.id });
}
console.log('mock tests: ok');

// Mock of the machine exports: one machine "hero", layer "body", states a/b, bool input "go".
function mockMachine(x) {
  const dec = new TextDecoder(), enc = new TextEncoder();
  const str = (p, n) => dec.decode(new Uint8Array(x.memory.buffer, p, n));
  let started = false, state = 'a', go = false, pending = [], err = '';
  const emit = (s) => { x.__out = enc.encode(s); new Uint8Array(x.memory.buffer).set(x.__out, 40000); return x.__out.length; };
  const fail = (m) => { err = m; new Uint8Array(x.memory.buffer).set(enc.encode(m), 60000); x.arm_error_len = () => m.length; return -1; };
  x.arm_machine_count = () => 1;
  x.arm_machine_name = (h, i) => (i === 0 ? emit('hero') : fail('machine index out of range'));
  x.arm_player_start = (h, p, n) => (str(p, n) === 'hero' ? ((started = true), (state = 'a'), 0) : fail('unknown machine'));
  x.arm_player_stop = () => { started = false; };
  x.arm_player_set_bool = (h, p, n, v) => (started ? (str(p, n) === 'go' ? ((go = !!v), 0) : fail('no input')) : fail('no player started'));
  x.arm_player_set_number = (h, p, n, v) => fail('input is not a number');
  x.arm_player_fire = () => fail('input is not a trigger');
  x.arm_player_set_value = (h, p, n, v) => (str(p, n).includes(' ') ? fail('invalid value path') : 0);
  x.arm_player_advance = (h, dt) => {
    if (!(dt >= 0)) return fail('dt out of range');
    if (go && state === 'a') { state = 'b'; pending.push({ time: 0, layer: 'body', kind: 'entered', state: 'b' }); }
    return 0;
  };
  x.arm_player_state = (h, p, n) => (str(p, n) === 'body' ? emit(state) : fail('unknown layer'));
  x.arm_player_events = () => { const j = JSON.stringify(pending); pending = []; return emit(j); };
  x.arm_player_render_svg = () => emit('<svg>' + state + '</svg>');
  x.arm_player_render_rgba = (h, s) => { x.__out = new Uint8Array(4 * 2 * 2).fill(9); new Uint8Array(x.memory.buffer).set(x.__out, 40000); x.__w = 2; x.__h = 2; return 16; };
  const baseOut = x.arm_out_ptr, baseLen = x.arm_out_len, baseW = x.arm_out_width, baseH = x.arm_out_height;
  x.arm_out_ptr = () => (x.__out ? 40000 : baseOut());
  x.arm_out_len = () => (x.__out ? x.__out.length : baseLen());
  x.arm_out_width = () => (x.__w ?? baseW());
  x.arm_out_height = () => (x.__h ?? baseH());
  return x;
}
const m = await Osatura.create(mockMachine(mockExports()), { name: 'x' });
assert.deepEqual(m.machines(), ['hero']);
assert.throws(() => m.setBool('go', true), /no player started/);
assert.throws(() => m.startPlayer('nope'), /unknown machine/);
m.startPlayer('hero');
assert.equal(m.state('body'), 'a');
m.setBool('go', true).advance(0.1);
assert.equal(m.state('body'), 'b');
assert.equal(m.playerSvg(), '<svg>b</svg>');
assert.deepEqual(m.events().map((e) => e.state), ['b']);
assert.deepEqual(m.events(), []);
assert.equal(m.playerRgba(0.5).data[0], 9);
assert.throws(() => m.setNumber('go', 1), /not a number/);
assert.throws(() => m.fire('go'), /not a trigger/);
assert.throws(() => m.setValue('bad path', 1), /invalid value path/);
m.setValue('ok.path', 1);
assert.throws(() => m.advance(-1), /dt out of range/);
assert.throws(() => m.state('zz'), /unknown layer/);
m.stopPlayer();
m.close();
console.log('machine mock tests: ok');

// Mock of the layout / scroll / LOD exports.
function mockLayout(x) {
  const dec = new TextDecoder(), enc = new TextEncoder();
  const str = (p, n) => dec.decode(new Uint8Array(x.memory.buffer, p, n));
  let canvas = [8, 4], off = [0, 0], settled = true, lod = 0;
  const emit = (s) => { x.__out = enc.encode(s); new Uint8Array(x.memory.buffer).set(x.__out, 40000); return x.__out.length; };
  const fail = (m) => { new Uint8Array(x.memory.buffer).set(enc.encode(m), 60000); x.arm_error_len = () => m.length; return -1; };
  x.arm_player_set_canvas = (h, w, hh) => (w > 0 && hh > 0 ? ((canvas = [w, hh]), 0) : fail('canvas size must be finite and > 0'));
  x.arm_player_layout = () => emit(JSON.stringify({ canvas, scrolls: ['list'], lod }));
  x.arm_player_scroll_drag = (h, p, n, a, b) => (str(p, n) === 'list' ? ((off = [a, b]), (settled = true), 0) : fail('unknown scroll'));
  x.arm_player_scroll_release = (h, p, n) => (str(p, n) === 'list' ? ((settled = false), 0) : fail('unknown scroll'));
  x.arm_player_scroll_info = (h, p, n) => (str(p, n) === 'list' ? emit(JSON.stringify({ offset: off, max: [0, 80], settled })) : fail('unknown scroll'));
  x.arm_player_set_screen = (h, w, hh, e) => (w > 0 && e >= 0 ? ((lod = e > 1 ? 2 : 0), 0) : fail('screen size must be finite and > 0'));
  const baseOut = x.arm_out_ptr, baseLen = x.arm_out_len;
  x.arm_out_ptr = () => (x.__out ? 40000 : baseOut());
  x.arm_out_len = () => (x.__out ? x.__out.length : baseLen());
  return x;
}
const l = await Osatura.create(mockLayout(mockExports()), { name: 'x' });
assert.deepEqual(l.layout().scrolls, ['list']);
l.setCanvas(100, 120);
assert.deepEqual(l.layout().canvas, [100, 120]);
assert.throws(() => l.setCanvas(0, 5), /canvas size/);
l.scrollDrag('list', 0, 50);
assert.deepEqual(l.scrollInfo('list'), { offset: [0, 50], max: [0, 80], settled: true });
l.scrollRelease('list', 0, 100);
assert.equal(l.scrollInfo('list').settled, false);
assert.throws(() => l.scrollDrag('nope', 0, 1), /unknown scroll/);
assert.throws(() => l.scrollInfo('nope'), /unknown scroll/);
l.setScreen(100, 120, 4);
assert.equal(l.layout().lod, 2);
assert.throws(() => l.setScreen(0, 120, 1), /screen size/);
l.close();
console.log('layout mock tests: ok');

// instantiate(url): a failing HTTP answer is a clear error, not a WebAssembly CompileError on an HTML error page
{
  const realFetch = globalThis.fetch;
  globalThis.fetch = async () => ({ ok: false, status: 404, statusText: 'Not Found', arrayBuffer: async () => new TextEncoder().encode('<html>404').buffer });
  await assert.rejects(() => Osatura.instantiate('http://x/osatura_wasm.wasm'), /HTTP 404[\s\S]*build-wasm\.sh/);
  globalThis.fetch = async () => { throw new TypeError('fetch failed'); };
  await assert.rejects(() => Osatura.instantiate('http://x/a.wasm'), /cannot load the wasm module[\s\S]*fetch failed/);
  globalThis.fetch = realFetch;
  if (typeof Response !== 'undefined') await assert.rejects(() => Osatura.instantiate(new Response('nope', { status: 500 })), /HTTP 500/);
  console.log('instantiate error tests: ok');
}

const wasm = join(dirname(fileURLToPath(import.meta.url)), '..', 'osatura_wasm.wasm');
if (!existsSync(wasm)) {
  console.log('real wasm tests: SKIPPED (osatura-wasm/osatura_wasm.wasm not built; run build-wasm.sh)');
} else {
  const x = await Osatura.instantiate(readFileSync(wasm));
  assert.equal(x.arm_abi_version(), 1);
  const demoPath = process.argv[2];
  assert.ok(demoPath, 'pass a project JSON path as argv[2]');
  const p = await Osatura.create(x, readFileSync(demoPath, 'utf8'));
  assert.ok(p.svg(0).includes('<svg'));
  const img = p.rgba(0, 0.5);
  assert.equal(img.data.length, img.width * img.height * 4);
  assert.throws(() => p.load('{ nope'));
  p.close();
  console.log('real wasm tests: ok');
}
