// S3 : overlays didOpen sur des fichiers générés dans OUT_DIR (ext = rsc ou rs)
import { readFileSync, readdirSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { startServer, initialize, posOf } from './lsp.mjs';

const root = process.argv[2];
const ext = process.argv[3];
// OUT_DIR tel que le voit `cargo check` (celui qu'utilise rust-analyzer)
const outDir = process.argv[4] ? `${root}/.crust` : execSync(`cargo check -q --message-format=json --all-targets`, { cwd: root }).toString().match(/"out_dir":"([^"]*)"/)[1];
const gen = process.argv[4] ? `${root}/.crust/gen/src` : `${outDir}/crust/src`;
const srv = startServer();
await initialize(srv, root);

const files = [`${gen}/main.${ext}`, `${gen}/robot/Moteur.${ext}`, `${gen}/robot/Servomoteur.${ext}`];
for (const f of files)
  srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text: readFileSync(f, 'utf8') } });

// overlay : on ajoute une méthode `turbo` à Moteur et on tape `s.parent.` dans main
const mot = readFileSync(files[1], 'utf8').replace('    pub fn accelerer', '    pub fn turbo(&mut self) {}\n    pub fn accelerer').replace(/\n\n(?=    pub fn turbo)/, '\n');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${files[1]}`, version: 2 }, contentChanges: [{ text: mot }] });
const main = readFileSync(files[0], 'utf8').replace('s.parent.accelerer(1200);', 's.parent.accelerer(1200);\n    s.parent.');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${files[0]}`, version: 2 }, contentChanges: [{ text: main }] });
await new Promise(r => setTimeout(r, 1500));

const t = Date.now();
const comp = await srv.request('textDocument/completion', { textDocument: { uri: `file://${files[0]}` }, position: posOf(main, '\n    s.parent.') });
const labels = (comp?.items ?? comp ?? []).map(i => i.label);
console.log(`completion (${Date.now() - t} ms):`, labels.length, labels.filter(l => /turbo|accelerer|vitesse/.test(l)), JSON.stringify(comp).slice(0,200));

const hover = await srv.request('textDocument/hover', { textDocument: { uri: `file://${files[0]}` }, position: posOf(main, 's.parent.vitesse', 3) });
console.log('hover parent:', JSON.stringify(hover?.contents)?.slice(0, 160));

// diagnostics natifs sur un overlay avec une erreur de type
const bad = mot.replace('self.vitesse += delta;', 'self.vitess += delta;');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${files[1]}`, version: 3 }, contentChanges: [{ text: bad }] });
const diag = await srv.waitFor(m => m.method === 'textDocument/publishDiagnostics' && m.params.uri.endsWith(`Moteur.${ext}`) && m.params.diagnostics.length > 0, 30000).catch(() => null);
console.log('diagnostics:', diag?.params.diagnostics.map(d => `${d.range.start.line + 1}:${d.range.start.character + 1} ${d.code} ${d.message.split('\n')[0]}`));
srv.proc.kill();
