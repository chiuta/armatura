#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/* demo/inject-page.mjs — pune demo-ul „pas cu pas” în pagina de prezentare (index.html de pe ramura landing-page).
 *
 *   node demo/inject-page.mjs --page ../armatura-lp/index.html [--data demo/out/demo-page.json]
 *
 * Idempotent: rulat din nou, înlocuiește blocurile dintre marcaje. Sursele sunt în demo/page/ (section.html, demo.css,
 * demo.js, i18n.json); datele (SVG-urile randate de motor și ieșirea comenzilor) vin din `node demo/build-demo.mjs`.
 */
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const arg = (n, d) => { const i = process.argv.indexOf(n); return i >= 0 ? process.argv[i + 1] : d; };
const PAGE = path.resolve(arg('--page', ''));
const DATA = path.resolve(arg('--data', path.join(HERE, 'out', 'demo-page.json')));
if (!process.argv.includes('--page') || !fs.existsSync(PAGE)) { console.error('lipsește --page <index.html>'); process.exit(2); }
const rd = (f) => fs.readFileSync(path.join(HERE, 'page', f), 'utf8');
let s = fs.readFileSync(PAGE, 'utf8');

const strip = (open, close) => { const a = s.indexOf(open); if (a < 0) return; const b = s.indexOf(close, a); if (b < 0) throw new Error('marcaj neînchis: ' + open); s = s.slice(0, a) + s.slice(b + close.length).replace(/^\n/, ''); };
const must = (cond, what) => { if (!cond) throw new Error('nu găsesc în pagină: ' + what); };

// 1. curățare
strip('/*demo:css*/', '/*/demo:css*/');
strip('<!--demo:section-->', '<!--/demo:section-->');
strip('/*demo:i18n*/', '/*/demo:i18n*/');
strip('/*demo:js*/', '/*/demo:js*/');

// 2. CSS (înaintea ultimului </style> al blocului principal)
const cssAt = s.indexOf('</style>'); must(cssAt > 0, '</style>');
s = s.slice(0, cssAt) + '/*demo:css*/\n' + rd('demo.css') + '/*/demo:css*/\n' + s.slice(cssAt);

// 3. secțiunea + datele (JSON inert; `<` scăpat ca să nu închidă blocul)
const data = JSON.stringify(JSON.parse(fs.readFileSync(DATA, 'utf8'))).replace(/</g, '\\u003c');
const secAt = s.indexOf('<section id="features">'); must(secAt > 0, '<section id="features">');
s = s.slice(0, secAt) + '<!--demo:section-->\n' + rd('section.html') + '<script type="application/json" id="demo-data">' + data + '</script>\n<!--/demo:section-->\n\n' + s.slice(secAt);

// 4. traduceri (după definiția LANGS; T e deja definit)
const i18n = JSON.stringify(JSON.parse(rd('i18n.json')));
const langAt = s.indexOf('\nconst LANGS='); must(langAt > 0, 'const LANGS');
const langEnd = s.indexOf('\n', langAt + 1);
s = s.slice(0, langEnd + 1) + '/*demo:i18n*/Object.entries(' + i18n + ').forEach(([l,o])=>Object.assign(T[l],o));/*/demo:i18n*/\n' + s.slice(langEnd + 1);

// 5. JS (înaintea apelului final applyLang();)
const callAt = s.lastIndexOf('\napplyLang();'); must(callAt > 0, 'applyLang();');
s = s.slice(0, callAt) + '\n/*demo:js*/\n' + rd('demo.js') + '/*/demo:js*/' + s.slice(callAt);

// 6. navigație și paleta de comenzi (o singură dată)
const nav = '<li><a href="#play" data-i="n1"></a></li>';
if (!s.includes('data-i="nDemo"')) { must(s.includes(nav), 'link n1 din nav'); s = s.replace(nav, nav + '<li><a href="#demo" data-i="nDemo"></a></li>'); }
if (!s.includes("go('nDemo'")) { must(s.includes("go('n1','#play'),"), "go('n1','#play')"); s = s.replace("go('n1','#play'),", "go('n1','#play'),go('nDemo','#demo'),"); }

fs.writeFileSync(PAGE, s);
console.log(`ok: demo injectat în ${PAGE} (${(s.length / 1024).toFixed(0)} KiB)`);
