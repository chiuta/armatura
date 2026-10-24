#!/usr/bin/env node
// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
/* scripts/build-studio-tour.mjs — turul ghidat „Demo” din Studio: persoana care merge, în 13 etape.
 *
 *   node scripts/build-studio-tour.mjs --bin target/release/osatura [--studio osatura-cli/src/studio.html]
 *
 * `osatura demo-person` scrie proiectul terminat; etapele sunt ACELAȘI proiect, tăiat după fiecare pas. Fiecare etapă e verificată
 * cu binarul real (`check`, `timeline`, `lint`, `stats`, `export`), iar turul o încarcă în editor prin POST /api/op (Set pe calea ""
 * = înlocuiește tot proiectul, un singur pas de Undo). Nu scrie nimic pe disc; Save e dezactivat cât timp demo-ul e încărcat.
 * Idempotent: rulat din nou, înlocuiește blocurile dintre marcaje. Sursele: scripts/studio-tour/{tour.html,tour.css,tour.js}.
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
const STUDIO = path.resolve(arg('--studio', path.join(ROOT, 'osatura-cli', 'src', 'studio.html')));
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
const cmd = (c, out) => ({cmd: c, out});

const steps = [];

// 1 — proiect gol
{
  const e = {format: full.format, name: 'Walking person', width: full.width, height: full.height, fps: full.fps, frames: full.frames, background: full.background};
  fs.writeFileSync(path.join(WORK, 'stage-01.json'), JSON.stringify(e));
  const pretty = JSON.stringify(e, null, 2);
  steps.push({id: 'empty', files: [{name: 'stage-01.json', text: pretty}], commands: [cmd('osatura check stage-01.json', run(['check', 'stage-01.json']))], project: read('stage-01.json')});
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
  steps.push({id: s.id, commands: [cmd(`osatura check ${f}`, c)], files: [], project: read(f)});
});

// 6 — scheletul (oase + greutăți), cu oasele desenate
{
  const f = 'stage-06.json';
  wr(f, cut({layerGroups: [0, 1, 2, 3], rig: true, chGroups: []}));
  const c = run(['check', f]);
  steps.push({id: 'skeleton', commands: [cmd(`osatura check ${f}`, c), cmd(`osatura render ${f} 0 rig.svg --bones`, run(['render', f, '0', 'rig.svg', '--bones']) || 'wrote rig.svg')], files: [], project: read(f)});
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
  steps.push({id: s.id, commands: [cmd(`osatura check ${f}`, c), cmd(`osatura timeline ${f}`, t)], files: [], project: read(f)});
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
  steps.push({id: 'check', commands: [cmd(`osatura lint ${lastFile}`, lint), cmd(`osatura stats ${lastFile}`, st.slice(0, 12).join('\n') + (st.length > 12 ? '\n  …' : ''))], files: [], same: 'ground'});
}

// 13 — export
{
  const sizes = {}; const ex = [];
  for (const ext of ['gif', 'html', 'json']) {
    const t = run(['export', lastFile, `person.${ext}`]);
    sizes[ext] = fs.statSync(path.join(WORK, `person.${ext}`)).size;
    ex.push(cmd(`osatura export ${lastFile} person.${ext}`, t + `   (${(sizes[ext] / 1024).toFixed(0)} KiB)`));
  }
  steps.push({id: 'export', commands: ex, files: [], same: 'ground'});
}

const engine = run(['--version']).split('\n')[0];

// ---- textele (engleză, din scripts/demo-person-i18n.json de pe ramura site-ului) ----
const TEXTS = JSON.parse(fs.readFileSync(path.join(HERE, 'studio-tour', 'texts.en.json'), 'utf8'));
const IDS = steps.map((s) => s.id);
const dmText = Object.fromEntries(IDS.map((id, k) => [id, [TEXTS['dS' + (k + 1)], TEXTS['dD' + (k + 1)]]]));

// ---- injectare în studio.html ----
const td = (f) => fs.readFileSync(path.join(HERE, 'studio-tour', f), 'utf8');
let s = fs.readFileSync(STUDIO, 'utf8');
const must = (c, w) => { if (!c) throw new Error('nu găsesc în studio.html: ' + w); };
const strip = (open, close) => { const a = s.indexOf(open); if (a < 0) return; const b = s.indexOf(close, a); must(b > 0, close); s = s.slice(0, a) + s.slice(b + close.length).replace(/^\n/, ''); };
strip('/*demo:css*/', '/*/demo:css*/');
strip('<!--demo:panel-->', '<!--/demo:panel-->');
strip('<!--demo:script-->', '<!--/demo:script-->');
const cssAt = s.indexOf('</style>'); must(cssAt > 0, '</style>');
s = s.slice(0, cssAt) + '/*demo:css*/\n' + td('tour.css') + '/*/demo:css*/\n' + s.slice(cssAt);
const panelAt = s.indexOf('<div id="sgp"'); must(panelAt > 0, '<div id="sgp"');
s = s.slice(0, panelAt) + '<!--demo:panel-->\n' + td('tour.html') + '<!--/demo:panel-->\n' + s.slice(panelAt);
// proiectele se stochează gzip+base64 (decomprimate în browser la „Start the demo”)
const packed = {engine, steps: steps.map((st) => { const {project, ...r} = st; return st.same ? r : {...r, zp: zlib.gzipSync(Buffer.from(project), {level: 9}).toString('base64')}; })};
const data = JSON.stringify(packed).replace(/</g, '\\u003c');
const tourJs = td('tour.js').replace(/^\/\/ SPDX.*\n\/\/ Copyright.*\n/, '').replace('/*DM_TEXT*/{}', JSON.stringify(dmText)).replace(/`/g, '');
const bodyEnd = s.lastIndexOf('</body>'); must(bodyEnd > 0, '</body>');
s = s.slice(0, bodyEnd) + '<!--demo:script-->\n<script type="application/json" id="demo-tour">' + data + '</script>\n<script>\n' + tourJs + '</script>\n<!--/demo:script-->\n' + s.slice(bodyEnd);
const btn = '<button id="sgbtn"';
if (!s.includes('id="demobtn"')) { must(s.includes(btn), 'butonul Suggest'); s = s.replace(btn, '<button id="demobtn" title="guided step-by-step demo: build a walking person with the real commands">Demo</button>' + btn); }
fs.writeFileSync(STUDIO, s);
console.log(`ok: ${steps.length} pași, date ${(data.length / 1024).toFixed(0)} KiB → ${STUDIO} (${(s.length / 1024).toFixed(0)} KiB)`);
fs.rmSync(WORK, {recursive: true, force: true});
