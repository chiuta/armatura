#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/* scripts/build-person-demo.mjs — demo-ul „pas cu pas” al persoanei care merge, din randările REALE ale motorului.
 *
 *   node scripts/build-person-demo.mjs --bin /cale/osatura [--template site/index.template.html]
 *   python3 scripts/build-site.py            # apoi regenerează index.html
 *
 * `osatura demo-person` scrie proiectul terminat. Etapele sunt ACELAȘI proiect, tăiat după fiecare pas (straturi, oase și canale
 * adăugate treptat); fiecare etapă e verificată (`check`) și randată (`render` / `frames`) cu binarul real. Nimic nu e desenat de mână.
 * Textele pașilor vin din scripts/demo-person-i18n.json și înlocuiesc cheile dS și dD din blocul demo:i18n al șablonului.
 */
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import {fileURLToPath} from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..');
const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const BIN = path.resolve(arg('--bin', 'osatura'));
const TPL = path.resolve(arg('--template', path.join(ROOT, 'site', 'index.template.html')));
const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'person-'));

function run(args, ok = [0]) {
  const r = spawnSync(BIN, args, {cwd: WORK, encoding: 'utf8', maxBuffer: 1 << 28});
  const text = ((r.stdout || '') + (r.stderr || '')).split('\n').filter((l) => l && !/free software|This repository moved|^remote:/.test(l)).join('\n');
  if (!ok.includes(r.status)) throw new Error(`osatura ${args.join(' ')} → cod ${r.status}\n${text}`);
  return text;
}
const rd = (f) => JSON.parse(fs.readFileSync(path.join(WORK, f), 'utf8'));
const wr = (f, o) => fs.writeFileSync(path.join(WORK, f), JSON.stringify(o));
const read = (f) => fs.readFileSync(path.join(WORK, f), 'utf8');

run(['demo-person', 'person.json']);
const full = rd('person.json');

// ---- grupuri de straturi (după nume) și de canale (după țintă) ----
const base = (n) => n.replace(/_far$/, '');
const SCENERY = new Set(['sky', 'sun', 'hills', 'meadow', 'fence', 'ground ticks']);
const BODY = new Set(['shadow', 'leg', 'shoe', 'sole', 'pelvis', 'torso', 'neck']);
const HEAD = new Set(['ponytail', 'head', 'blush', 'ear', 'hair', 'brow', 'eye', 'nostril', 'mouth']);
const ARMS = new Set(['arm', 'sleeve', 'palm', 'pinky', 'ring', 'middle', 'index', 'thumb']);
const groupOf = (name) => SCENERY.has(name) ? 0 : BODY.has(base(name)) ? 1 : HEAD.has(base(name)) ? 2 : ARMS.has(base(name)) ? 3 : -1;
for (const l of full.layers) if (groupOf(l.name) < 0) throw new Error('strat necunoscut: ' + l.name);

const sfx = (n) => n.replace(/_far$/, '');
const LEGS = new Set(['hips', 'thigh', 'shin', 'foot', 'toes']);
const ARMCH = /^(arm_up|arm_low|hand|index[12]|middle[12]|ring[12]|pinky[12]|thumb[12])$/;
const TRUNK = new Set(['spine', 'chest', 'neck', 'head', 'ear', 'hair1', 'hair2']);
const FACE = new Set(['eye', 'mouth']);
const GROUND = new Set(['ground ticks', 'fence', 'shadow']);
const chGroup = (t) => LEGS.has(sfx(t)) ? 0 : ARMCH.test(sfx(t)) ? 1 : TRUNK.has(sfx(t)) ? 2 : FACE.has(t) ? 3 : GROUND.has(t) ? 4 : -1;
for (const c of full.channels) if (chGroup(c.target) < 0) throw new Error('canal necunoscut: ' + c.target);

const clone = (o) => JSON.parse(JSON.stringify(o));
function stripWeights(o) { if (Array.isArray(o)) o.forEach(stripWeights); else if (o && typeof o === 'object') { delete o.weights; for (const k of Object.keys(o)) stripWeights(o[k]); } }
const noBone = (l) => { l.bone = null; if (l.children) l.children.forEach(noBone); if (l.layers) l.layers.forEach(noBone); };

/** Proiect parțial. layerGroups: grupurile de straturi vizibile; rig: oase+greutăți; chGroups: grupurile de canale. */
function cut({layerGroups, rig, chGroups}) {
  const p = clone(full);
  p.layers = p.layers.filter((l) => layerGroups.includes(groupOf(l.name)));
  p.channels = p.channels.filter((c) => chGroups.includes(chGroup(c.target)) && (layerGroups.length > 0));
  if (!rig) { p.bones = []; p.ik = []; p.constraints = []; p.springs = []; p.layers.forEach((l) => { noBone(l); stripWeights(l); }); }
  return p;
}
const names = (p) => p.layers.map((l) => l.name);

const tail = (s, n) => { const a = s.split('\n'); return a.length > n ? a.slice(0, n).join('\n') + '\n  …' : s; };
const svgs = (dir) => fs.readdirSync(path.join(WORK, dir)).filter((f) => f.endsWith('.svg')).sort().map((f) => read(path.join(dir, f)));
const still = (file, extra = []) => { run(['render', file, '0', 'still.svg', ...extra]); return read('still.svg'); };
const frames = (file, dir, extra = []) => { fs.rmSync(path.join(WORK, dir), {recursive: true, force: true}); run(['frames', file, dir, ...extra]); return svgs(dir); };
const cmd = (c, out) => ({cmd: c, out});

const steps = [];
const AR = `${full.width} / ${full.height}`;

// 1 — proiect gol
{
  const e = {format: full.format, name: 'Walking person', width: full.width, height: full.height, fps: full.fps, frames: full.frames, background: full.background};
  fs.writeFileSync(path.join(WORK, 'stage-01.json'), JSON.stringify(e));
  const pretty = JSON.stringify(e, null, 2);
  steps.push({id: 'empty', files: [{name: 'stage-01.json', text: pretty}], commands: [cmd('osatura check stage-01.json', run(['check', 'stage-01.json']))], still: still('stage-01.json'), ar: AR});
}

// 2–5 — straturi, pe grupe (fără oase)
const stills = [
  {id: 'scenery', groups: [0]},
  {id: 'body', groups: [0, 1]},
  {id: 'head', groups: [0, 1, 2]},
  {id: 'arms', groups: [0, 1, 2, 3]},
];
let prevNames = new Set();
stills.forEach((s, i) => {
  const f = `stage-${String(i + 2).padStart(2, '0')}.json`;
  const p = cut({layerGroups: s.groups, rig: false, chGroups: []});
  wr(f, p);
  const c = run(['check', f]);
  prevNames = new Set(names(p));
  steps.push({id: s.id, commands: [cmd(`osatura check ${f}`, c)], files: [], still: still(f), ar: AR});
});

// 6 — scheletul (oase + greutăți), cu oasele desenate
{
  const f = 'stage-06.json';
  wr(f, cut({layerGroups: [0, 1, 2, 3], rig: true, chGroups: []}));
  const c = run(['check', f]);
  steps.push({id: 'skeleton', commands: [cmd(`osatura check ${f}`, c), cmd(`osatura render ${f} 0 rig.svg --bones`, 'wrote rig.svg')], files: [], still: still(f, ['--bones']), ar: AR});
}

// 7–11 — canale, cumulativ
const anim = [
  {id: 'legs', ch: [0]},
  {id: 'swing', ch: [0, 1]},
  {id: 'trunk', ch: [0, 1, 2]},
  {id: 'face', ch: [0, 1, 2, 3]},
  {id: 'ground', ch: [0, 1, 2, 3, 4]},
];
let lastFile = '';
anim.forEach((s, i) => {
  const f = `stage-${String(i + 7).padStart(2, '0')}.json`;
  wr(f, cut({layerGroups: [0, 1, 2, 3], rig: true, chGroups: s.ch}));
  const c = run(['check', f]);
  const t = tail(run(['timeline', f]), 14);
  steps.push({id: s.id, commands: [cmd(`osatura check ${f}`, c), cmd(`osatura timeline ${f}`, t)], files: [], frames: frames(f, 'fr-' + s.id), ar: AR});
  lastFile = f;
});

// o verificare: etapa finală trebuie să fie proiectul terminat (aceleași straturi, oase și canale)
{
  const a = rd(lastFile);
  if (a.layers.length !== full.layers.length || a.bones.length !== full.bones.length || a.channels.length !== full.channels.length) throw new Error('etapa finală diferă de proiectul terminat');
}

// 12 — verificare
{
  const lintR = spawnSync(BIN, ['lint', lastFile], {cwd: WORK, encoding: 'utf8'});
  const lint = ((lintR.stdout || '') + (lintR.stderr || '')).split('\n').filter((l) => l && !/free software|repository moved|^remote:/.test(l)).join('\n');
  if (lintR.status !== 0) throw new Error('lint a găsit erori:\n' + lint);
  const st = run(['stats', lastFile]).split('\n');
  steps.push({id: 'check', commands: [cmd(`osatura lint ${lastFile}`, lint), cmd(`osatura stats ${lastFile}`, st.slice(0, 12).join('\n') + (st.length > 12 ? '\n  …' : ''))], files: [], same: 'ground', ar: AR});
}

// 13 — export
{
  const sizes = {}; const ex = [];
  for (const ext of ['gif', 'html', 'json']) {
    const t = run(['export', lastFile, `person.${ext}`]);
    sizes[ext] = fs.statSync(path.join(WORK, `person.${ext}`)).size;
    ex.push(cmd(`osatura export ${lastFile} person.${ext}`, t + `   (${(sizes[ext] / 1024).toFixed(0)} KiB)`));
  }
  steps.push({id: 'export', commands: ex, files: [], same: 'ground', sizes, ar: AR});
}

const engine = run(['--version']).split('\n')[0];
const data = {version: 2, generator: 'scripts/build-person-demo.mjs', engine, width: full.width, height: full.height, fps: full.fps, frames: full.frames, steps};

// ---- injectare în șablon ----
let tpl = fs.readFileSync(TPL, 'utf8');
// cadrele (SVG) se stochează comprimate gzip+base64; primul cadru rămâne ca imagine statică de rezervă. Pagina le decomprimă la încărcare.
const packed = {...data, steps: data.steps.map((s) => {
  if (!s.frames) return s;
  const {frames, ...rest} = s;
  return {...rest, still: frames[0], count: frames.length, zf: zlib.gzipSync(Buffer.from(JSON.stringify(frames)), {level: 9}).toString('base64')};
})};
const json = JSON.stringify(packed).replace(/</g, '\\u003c');
const dRe = /(<script type="application\/json" id="demo-data">)[\s\S]*?(<\/script>)/;
if (!dRe.test(tpl)) throw new Error('nu găsesc #demo-data în șablon');
tpl = tpl.replace(dRe, (_, a, b) => a + json + b);

const iRe = /\/\*demo:i18n\*\/Object\.entries\(([\s\S]*?)\)\.forEach/;
const mm = tpl.match(iRe);
if (!mm) throw new Error('nu găsesc blocul demo:i18n');
const tr = JSON.parse(mm[1]);
const add = JSON.parse(fs.readFileSync(path.join(HERE, 'demo-person-i18n.json'), 'utf8'));
for (const l of Object.keys(tr)) {
  for (const k of Object.keys(tr[l])) if (/^d[SD]\d+$/.test(k)) delete tr[l][k];
  Object.assign(tr[l], add[l] || add.en);
}
for (const l of Object.keys(add)) { if (!tr[l]) throw new Error('limbă nouă fără bloc: ' + l); for (let n = 1; n <= steps.length; n++) if (!add[l]['dS' + n] || !add[l]['dD' + n]) throw new Error(`lipsește dS${n}/dD${n} în ${l}`); }
tpl = tpl.replace(iRe, () => '/*demo:i18n*/Object.entries(' + JSON.stringify(tr) + ').forEach');
fs.writeFileSync(TPL, tpl);
const kb = (s) => (Buffer.byteLength(s) / 1024).toFixed(0);
console.log(`ok: ${steps.length} pași, date ${kb(json)} KiB → ${TPL}`);
for (const s of steps) console.log(' ', s.id.padEnd(9), s.frames ? s.frames.length + ' cadre, ' + kb(s.frames.join('')) + ' KiB' : s.still ? 'still ' + kb(s.still) + ' KiB' : '-');
fs.rmSync(WORK, {recursive: true, force: true});
