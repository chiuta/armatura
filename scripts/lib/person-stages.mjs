// SPDX-License-Identifier: MPL-2.0
// Copyright (c) 2026 Alexandru-Ionuț Chiuță (Alexio)
/* scripts/lib/person-stages.mjs — etapele demo-ului „persoana care merge”, folosite de site (scripts/build-person-demo.mjs)
 * și de turul din Studio (scripts/build-studio-tour.mjs).
 *
 * `osatura demo-person` scrie proiectul terminat; etapele sunt ACELAȘI proiect, tăiat după fiecare pas (straturi, oase și canale
 * adăugate treptat). Fiecare etapă e scrisă într-un fișier stage-NN.json din directorul de lucru și verificată cu binarul real
 * (`check`, `timeline`, `lint`, `stats`, `export`); ieșirile comenzilor sunt cele reale.
 */
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

/** Afirmațiile din texte (numărul de oase, dimensiunea, cadrele) trebuie să rămână adevărate: build-ul pică dacă proiectul s-a schimbat. */
export const EXPECT = {bones: 42, width: 640, height: 480, fps: 24, frames: 32};

export function personStages({bin, work}) {
  const WORK = work;
  const run = (args, ok = [0]) => {
    const r = spawnSync(bin, args, {cwd: WORK, encoding: 'utf8', maxBuffer: 1 << 28});
    const text = ((r.stdout || '') + (r.stderr || '')).split('\n').filter((l) => l && !/free software|This repository moved|^remote:/.test(l)).join('\n');
    if (!ok.includes(r.status)) throw new Error(`osatura ${args.join(' ')} → cod ${r.status}\n${text}`);
    return text;
  };
  const rd = (f) => JSON.parse(fs.readFileSync(path.join(WORK, f), 'utf8'));
  const wr = (f, o) => fs.writeFileSync(path.join(WORK, f), JSON.stringify(o));
  const read = (f) => fs.readFileSync(path.join(WORK, f), 'utf8');
run(['demo-person', 'person.json']);
const full = rd('person.json');
for (const [k, v] of Object.entries({width: EXPECT.width, height: EXPECT.height, fps: EXPECT.fps, frames: EXPECT.frames})) if (full[k] !== v) throw new Error(`demo-person: ${k} = ${full[k]}, textele spun ${v}; actualizează textele și EXPECT`);
if (full.bones.length !== EXPECT.bones) throw new Error(`demo-person: ${full.bones.length} oase, textele spun ${EXPECT.bones}; actualizează textele și EXPECT`);

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

const tail = (s, n) => { const a = s.split('\n'); return a.length > n ? a.slice(0, n).join('\n') + '\n  …' : s; };
const cmd = (c, out) => ({cmd: c, out});

const steps = [];

// 1 — proiect gol
{
  const e = {format: full.format, name: full.name, width: full.width, height: full.height, fps: full.fps, frames: full.frames, background: full.background};
  fs.writeFileSync(path.join(WORK, 'stage-01.json'), JSON.stringify(e));
  const pretty = JSON.stringify(e, null, 2);
  steps.push({id: 'empty', file: 'stage-01.json', files: [{name: 'stage-01.json', text: pretty}], commands: [cmd('osatura check stage-01.json', run(['check', 'stage-01.json']))], project: read('stage-01.json')});
}

// 2–5 — straturi, pe grupe (fără oase)
const stills = [
  {id: 'scenery', groups: [0]},
  {id: 'body', groups: [0, 1]},
  {id: 'head', groups: [0, 1, 2]},
  {id: 'arms', groups: [0, 1, 2, 3]},
];
stills.forEach((s, i) => {
  const f = `stage-${String(i + 2).padStart(2, '0')}.json`;
  const p = cut({layerGroups: s.groups, rig: false, chGroups: []});
  wr(f, p);
  const c = run(['check', f]);
  steps.push({id: s.id, file: f, commands: [cmd(`osatura check ${f}`, c)], files: [], project: read(f)});
});

// 6 — scheletul (oase + greutăți), cu oasele desenate
{
  const f = 'stage-06.json';
  wr(f, cut({layerGroups: [0, 1, 2, 3], rig: true, chGroups: []}));
  const c = run(['check', f]);
  steps.push({id: 'skeleton', file: f, bones: true, commands: [cmd(`osatura check ${f}`, c), cmd(`osatura render ${f} 0 rig.svg --bones`, run(['render', f, '0', 'rig.svg', '--bones']) || 'wrote rig.svg')], files: [], project: read(f)});
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
  steps.push({id: s.id, file: f, anim: true, commands: [cmd(`osatura check ${f}`, c), cmd(`osatura timeline ${f}`, t)], files: [], project: read(f)});
  lastFile = f;
});

// o verificare: etapa finală trebuie să fie proiectul terminat (aceleași straturi, oase și canale)
{
  const a = rd(lastFile);
  if (a.layers.length !== full.layers.length || a.bones.length !== full.bones.length || a.channels.length !== full.channels.length) throw new Error('etapa finală diferă de proiectul terminat');
}

// 12 — verificare
{
  const lintR = spawnSync(bin, ['lint', lastFile], {cwd: WORK, encoding: 'utf8'});
  const lint = ((lintR.stdout || '') + (lintR.stderr || '')).split('\n').filter((l) => l && !/free software|repository moved|^remote:/.test(l)).join('\n');
  if (lintR.status !== 0) throw new Error('lint a găsit erori:\n' + lint);
  const st = run(['stats', lastFile]).split('\n');
  steps.push({id: 'check', file: lastFile, commands: [cmd(`osatura lint ${lastFile}`, lint), cmd(`osatura stats ${lastFile}`, st.slice(0, 12).join('\n') + (st.length > 12 ? '\n  …' : ''))], files: [], same: 'ground'});
}

// 13 — export
{
  const sizes = {}; const ex = [];
  for (const ext of ['gif', 'html', 'json']) {
    const t = run(['export', lastFile, `person.${ext}`]);
    sizes[ext] = fs.statSync(path.join(WORK, `person.${ext}`)).size;
    ex.push(cmd(`osatura export ${lastFile} person.${ext}`, t + `   (${(sizes[ext] / 1024).toFixed(0)} KiB)`));
  }
  steps.push({id: 'export', file: lastFile, commands: ex, files: [], same: 'ground'});
}


  const engine = run(['--version']).split('\n')[0];
  return {run, read, full, steps, lastFile, engine};
}
