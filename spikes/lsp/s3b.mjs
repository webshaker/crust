// Complétion / diagnostics dans un module atteint par #[path] (sans include!) et extension .rsc
import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';
const root = process.argv[2];
const gen = process.argv[3] ?? `${root}/.crust/gen/src`;
const srv = startServer();
await initialize(srv, root);
const files = [`${gen}/main.rsc`, `${gen}/robot/Moteur.rsc`, `${gen}/robot/Servomoteur.rsc`];
for (const f of files)
  srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text: readFileSync(f, 'utf8') } });
let servo = readFileSync(files[2], 'utf8').replace('Self { parent', 'let m = Moteur::default();\n        m.\n        Self { parent');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${files[2]}`, version: 2 }, contentChanges: [{ text: servo }] });
const comp = await srv.request('textDocument/completion', { textDocument: { uri: `file://${files[2]}` }, position: posOf(servo, '\n        m.') });
console.log('completion Servomoteur:', (comp?.items ?? comp ?? []).map(i => i.label));
const bad = readFileSync(files[1], 'utf8').replace('self.vitesse += delta;', 'self.vitess += delta;');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${files[1]}`, version: 2 }, contentChanges: [{ text: bad }] });
let diag; for (let i = 0; i < 10; i++) { const r = await srv.request('textDocument/diagnostic', { textDocument: { uri: `file://${files[1]}` } }).catch(e => null); if (r) { diag = { params: { diagnostics: r.items } }; break; } await new Promise(x => setTimeout(x, 500)); }
console.log('diagnostics Moteur:', diag?.params.diagnostics.map(d => `${d.range.start.line + 1}:${d.range.start.character + 1} ${d.code} ${d.message.split('\n')[0]}`));
srv.proc.kill();
