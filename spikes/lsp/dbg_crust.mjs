import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';
const root = process.argv[2];
const srv = startServer('/home/rtx/crust/target/debug/crust-lsp', [], { env: { ...process.env, CRUST_LSP_TRACE: '1' } });
await initialize(srv, root);
await srv.waitFor(m => m.method === 'crust/status', 300000);
console.log('ready');
const t0=Date.now(); await srv.waitFor(m => m.method === 'experimental/serverStatus' && m.params.quiescent && srv.notifications.indexOf(m) > srv.notifications.findIndex(n => n.method==='crust/status'), 300000).catch(()=>console.log('pas quiescent')); console.log('quiescent après', Date.now()-t0, 'ms');
const f = `${root}/src/robot/Servomoteur.rsc`;
let text = readFileSync(f, 'utf8').replace('        self.position = position;', '        self.position = position;\n        self.parent.');
srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text } });
for (let i = 0; i < 6; i++) {
  await new Promise(r => setTimeout(r, 3000));
  const c = await srv.request('textDocument/completion', { textDocument: { uri: `file://${f}` }, position: posOf(text, 'self.parent.\n', 12) });
  console.log('completion', JSON.stringify(c).slice(0, 200));
  const h = await srv.request('textDocument/hover', { textDocument: { uri: `file://${f}` }, position: posOf(text, 'Moteur::new', 2) });
  const m = `${root}/src/main.rsc`; const mt = readFileSync(m,'utf8'); if (i==0) srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${m}`, languageId: 'rust', version: 1, text: mt } });
  const h2 = await srv.request('textDocument/hover', { textDocument: { uri: `file://${m}` }, position: posOf(mt, 'Servomoteur::new', 2) });
  console.log('hover main', JSON.stringify(h2).slice(0, 150));
  console.log('hover', JSON.stringify(h).slice(0, 200));
}
srv.proc.kill();
