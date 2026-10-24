#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/* demo/inject-studio.mjs — pune turul ghidat „Demo” în armatura-cli/src/studio.html.
 *
 *   node demo/inject-studio.mjs [--studio armatura-cli/src/studio.html] [--data demo/out/demo-studio.json]
 *
 * Idempotent (marcaje /*demo:…*​/). Turul încarcă, la fiecare pas, proiectul produs de comanda reală, prin POST /api/op
 * (Set pe calea "" = înlocuiește tot proiectul, un singur pas de Undo); nu scrie nimic pe disc, iar Save e dezactivat cât timp demo-ul e încărcat.
 */
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const FILE = path.resolve(arg('--studio', path.join(HERE, '..', 'armatura-cli', 'src', 'studio.html')));
const DATA = path.resolve(arg('--data', path.join(HERE, 'out', 'demo-studio.json')));
const rd = (f) => fs.readFileSync(path.join(HERE, 'studio', f), 'utf8');
let s = fs.readFileSync(FILE, 'utf8');
const must = (c, w) => { if (!c) throw new Error('nu găsesc în studio.html: ' + w); };
const strip = (open, close) => { const a = s.indexOf(open); if (a < 0) return; const b = s.indexOf(close, a); must(b > 0, close); s = s.slice(0, a) + s.slice(b + close.length).replace(/^\n/, ''); };

strip('/*demo:css*/', '/*/demo:css*/');
strip('<!--demo:panel-->', '<!--/demo:panel-->');
strip('<!--demo:script-->', '<!--/demo:script-->');

const cssAt = s.indexOf('</style>'); must(cssAt > 0, '</style>');
s = s.slice(0, cssAt) + '/*demo:css*/\n' + rd('tour.css') + '/*/demo:css*/\n' + s.slice(cssAt);

const panelAt = s.indexOf('<div id="sgp"'); must(panelAt > 0, '<div id="sgp"');
s = s.slice(0, panelAt) + '<!--demo:panel-->\n' + rd('tour.html') + '<!--/demo:panel-->\n' + s.slice(panelAt);

const data = JSON.stringify(JSON.parse(fs.readFileSync(DATA, 'utf8'))).replace(/</g, '\\u003c');
const bodyEnd = s.lastIndexOf('</body>'); must(bodyEnd > 0, '</body>');
s = s.slice(0, bodyEnd) + '<!--demo:script-->\n<script type="application/json" id="demo-tour">' + data + '</script>\n<script>\n' + rd('tour.js').replace(/`/g, '') + '</script>\n<!--/demo:script-->\n' + s.slice(bodyEnd);

const btn = '<button id="sgbtn"';
if (!s.includes('id="demobtn"')) { must(s.includes(btn), 'butonul Suggest'); s = s.replace(btn, '<button id="demobtn" title="guided step-by-step demo: build a whole character with the real commands">Demo</button>' + btn); }

fs.writeFileSync(FILE, s);
console.log(`ok: turul ghidat injectat în ${FILE} (${(s.length / 1024).toFixed(0)} KiB)`);
