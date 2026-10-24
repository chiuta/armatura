#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/* demo/build-demo.mjs — construiește omulețul „Buddy” cu binarul REAL `armatura` și strânge rezultatele pentru demo.
 *
 *   node demo/build-demo.mjs [--bin ../target/release/armatura] [--out demo/out]
 *
 * Scrie:
 *   out/*.json            proiectul după fiecare pas (se pot deschide în studio: `armatura studio out/07-mouth.json`)
 *   out/demo-assets.json  pașii pentru pagina de prezentare și pentru turul din Studio: comenzi, ieșirea lor reală,
 *                         scripturile, SVG-urile randate de motor și proiectele (JSON).
 * Nimic din ce se arată în demo nu e scris de mână: comenzile rulează aici, iar ieșirea lor e cea reală.
 */
import {spawnSync} from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import {fileURLToPath} from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const BIN = path.resolve(arg('--bin', path.join(HERE, '..', 'target', 'release', 'armatura')));
const OUT = path.resolve(arg('--out', path.join(HERE, 'out')));
const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'buddy-'));
fs.mkdirSync(OUT, {recursive: true});

const read = (f) => fs.readFileSync(f, 'utf8');
const rhai = (n) => read(path.join(HERE, n));
for (const f of fs.readdirSync(HERE).filter((x) => x.endsWith('.rhai') || x === '00-buddy.json')) fs.copyFileSync(path.join(HERE, f), path.join(WORK, f));

/** Rulează o comandă `armatura …` în WORK; întoarce ieșirea (stdout+stderr) fără banner-ul de licență. */
function run(args) {
  const r = spawnSync(BIN, args, {cwd: WORK, encoding: 'utf8', env: {...process.env, ARMATURA_NO_BANNER: '1'}});
  const text = ((r.stdout || '') + (r.stderr || '')).split('\n').filter((l) => l && !/^ARMATURA studio \(armatura-cli\) is free software/.test(l)).join('\n');
  if (r.status !== 0) throw new Error(`armatura ${args.join(' ')} → cod ${r.status}\n${text}`);
  return text;
}
const frames = (proj, dir, extra = []) => {
  fs.rmSync(path.join(WORK, dir), {recursive: true, force: true});
  run(['frames', proj, dir, ...extra]);
  return fs.readdirSync(path.join(WORK, dir)).filter((f) => f.endsWith('.svg')).sort().map((f) => read(path.join(WORK, dir, f)));
};
const still = (proj, frame, extra = []) => { run(['render', proj, String(frame), 'still.svg', ...extra]); return read(path.join(WORK, 'still.svg')); };
const proj = (f) => JSON.stringify(JSON.parse(read(path.join(WORK, f))));
const save = (f) => fs.copyFileSync(path.join(WORK, f), path.join(OUT, f));

const steps = [];
const cmd = (c, out) => ({cmd: c, out});

// 1 — proiect gol
fs.writeFileSync(path.join(WORK, 'buddy.json'), read(path.join(HERE, '00-buddy.json')));
steps.push({id: 'empty', file: 'buddy.json', project: proj('buddy.json'),
  files: [{name: 'buddy.json', text: read(path.join(HERE, '00-buddy.json')).trim()}],
  commands: [cmd('armatura check buddy.json', run(['check', 'buddy.json']))],
  still: still('buddy.json', 0)});

// 2 — forme
const out1 = run(['script', 'buddy.json', '01-shapes.json', '01-shapes.rhai']);
steps.push({id: 'shapes', file: '01-shapes.json', project: proj('01-shapes.json'),
  files: [{name: 'shapes.rhai', text: rhai('01-shapes.rhai').trim()}],
  commands: [cmd('armatura script buddy.json 01-shapes.json shapes.rhai', out1)],
  still: still('01-shapes.json', 0)});

// 3 — față
const out2 = run(['script', '01-shapes.json', '02-face.json', '02-face.rhai']);
steps.push({id: 'face', file: '02-face.json', project: proj('02-face.json'),
  files: [{name: 'face.rhai', text: rhai('02-face.rhai').trim()}],
  commands: [cmd('armatura script 01-shapes.json 02-face.json face.rhai', out2)],
  still: still('02-face.json', 0)});

// 4 — oase
const out3 = run(['script', '02-face.json', '03-bones.json', '03-bones.rhai']);
const chk3 = run(['check', '03-bones.json']);
run(['render', '03-bones.json', '0', 'rig.png', '--bones', '--scale', '2']);
steps.push({id: 'bones', file: '03-bones.json', project: proj('03-bones.json'),
  files: [{name: 'bones.rhai', text: rhai('03-bones.rhai').trim()}],
  commands: [cmd('armatura script 02-face.json 03-bones.json bones.rhai', out3), cmd('armatura check 03-bones.json', chk3),
    cmd('armatura render 03-bones.json 0 rig.png --bones --scale 2', 'wrote rig.png')],
  still: still('03-bones.json', 0, ['--bones'])});

// 5 — mers (generator procedural)
const walkArgs = ['walk', '03-bones.json', '04-walk.json', '--thigh-l', 'thigh_l', '--thigh-r', 'thigh_r', '--shin-l', 'shin_l', '--shin-r', 'shin_r',
  '--arm-l', 'arm_upper_l', '--hips', 'hips', '--period', '24', '--stride', '28', '--knee', '40', '--bob', '4', '--swing', '16'];
const out4 = run(walkArgs);
const tl4 = run(['timeline', '04-walk.json']);
steps.push({id: 'walk', file: '04-walk.json', project: proj('04-walk.json'), files: [],
  commands: [cmd('armatura walk 03-bones.json 04-walk.json --thigh-l thigh_l --thigh-r thigh_r \\\n    --shin-l shin_l --shin-r shin_r --arm-l arm_upper_l --hips hips \\\n    --period 24 --stride 28 --knee 40 --bob 4 --swing 16', out4),
    cmd('armatura timeline 04-walk.json', tl4)],
  frames: frames('04-walk.json', 'fr-walk', ['--bones'])});

// 6 — IK: salut
const out5 = run(['script', '04-walk.json', '05-wave.json', '05-wave.rhai']);
steps.push({id: 'wave', file: '05-wave.json', project: proj('05-wave.json'),
  files: [{name: 'wave.rhai', text: rhai('05-wave.rhai').trim()}],
  commands: [cmd('armatura script 04-walk.json 05-wave.json wave.rhai', out5)],
  frames: frames('05-wave.json', 'fr-wave', ['--bones'])});

// 7 — buclă: mersul și salutul se închid fără cusătură (înainte de gură: cheile „switch” nu se amestecă)
const loop = run(['loop-solve', '05-wave.json', '06-loop.json']);
steps.push({id: 'loop', file: '06-loop.json', project: proj('06-loop.json'), files: [],
  commands: [cmd('armatura loop-solve 05-wave.json 06-loop.json', loop)],
  frames: frames('06-loop.json', 'fr-loop', ['--bones'])});

// 8 — gura
const out6 = run(['script', '06-loop.json', '07-mouth.json', '06-mouth.rhai']);
steps.push({id: 'mouth', file: '07-mouth.json', project: proj('07-mouth.json'),
  files: [{name: 'mouth.rhai', text: rhai('06-mouth.rhai').trim()}],
  commands: [cmd('armatura script 06-loop.json 07-mouth.json mouth.rhai', out6)],
  frames: frames('07-mouth.json', 'fr-mouth')});

// 9 — verificări
const lintR = spawnSync(BIN, ['lint', '07-mouth.json'], {cwd: WORK, encoding: 'utf8'});
const lint = (lintR.stdout + lintR.stderr).split('\n').filter((l) => l && !/free software/.test(l)).join('\n');
if (lintR.status !== 0) throw new Error('lint a găsit erori:\n' + lint);
const statsFull = run(['stats', '07-mouth.json']);
const statsLines = statsFull.split('\n');
steps.push({id: 'check', file: '07-mouth.json', project: proj('07-mouth.json'), files: [],
  commands: [cmd('armatura lint 07-mouth.json', lint), cmd('armatura stats 07-mouth.json', statsLines.slice(0, 12).join('\n') + (statsLines.length > 12 ? '\n  …' : ''))],
  frames: frames('07-mouth.json', 'fr-final')});

// 10 — export
const sizes = {};
const exp = [];
for (const ext of ['gif', 'html', 'json']) {
  const t = run(['export', '07-mouth.json', `buddy.${ext}`]);
  sizes[ext] = fs.statSync(path.join(WORK, `buddy.${ext}`)).size;
  exp.push(cmd(`armatura export 07-mouth.json buddy.${ext}`, t + `   (${(sizes[ext] / 1024).toFixed(0)} KiB)`));
}
steps.push({id: 'export', file: '07-mouth.json', project: proj('07-mouth.json'), files: [], commands: exp, frames: null, sizes});

for (const f of fs.readdirSync(WORK).filter((x) => /^\d\d-.*\.json$/.test(x))) save(f);
const bundle = {version: 1, generator: 'demo/build-demo.mjs', engine: run(['--version']).split('\n')[0], width: 400, height: 360, fps: 24, frames: 24, steps};
fs.writeFileSync(path.join(OUT, 'demo-assets.json'), JSON.stringify(bundle));
console.log(`ok: ${steps.length} pași → ${path.join(OUT, 'demo-assets.json')} (${(fs.statSync(path.join(OUT, 'demo-assets.json')).size / 1024).toFixed(0)} KiB)`);
// Pachet subțire pentru pagina de prezentare: fără proiectele JSON (pagina arată doar comenzi, scripturi și randări).
const slim = {...bundle, steps: steps.map(({project, file, ...s}) => s)};
fs.writeFileSync(path.join(OUT, 'demo-page.json'), JSON.stringify(slim));
// Pachet pentru turul din Studio: proiectele JSON (încărcate prin /api/op), comenzile și scripturile; fără SVG-uri (Studio randează singur).
const tour = {...bundle, steps: steps.map(({still, frames, sizes, ...s}) => s)};
fs.writeFileSync(path.join(OUT, 'demo-studio.json'), JSON.stringify(tour));
console.log(`studio: ${path.join(OUT, 'demo-studio.json')} (${(fs.statSync(path.join(OUT, 'demo-studio.json')).size / 1024).toFixed(0)} KiB)`);
console.log(`pagina: ${path.join(OUT, 'demo-page.json')} (${(fs.statSync(path.join(OUT, 'demo-page.json')).size / 1024).toFixed(0)} KiB)`);
console.log(`motor: ${bundle.engine}`);
fs.rmSync(WORK, {recursive: true, force: true});
