import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';
const [root, server] = process.argv.slice(2);
const srv = startServer(server ?? '/home/rtx/crust/target/debug/crust-lsp');
await initialize(srv, root);
if (!server) await srv.waitFor(m => m.method === 'crust/status', 300000);
const f = `${root}/src/Capteur.rs`, t = readFileSync(f, 'utf8');
srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text: t } });
for (let i = 0; i < 10; i++) {
  await new Promise(r => setTimeout(r, 2000));
  const h = await srv.request('textDocument/hover', { textDocument: { uri: `file://${f}` }, position: posOf(t, 'servo.position', 1) });
  console.log(i, 'hover Capteur.rs', JSON.stringify(h).slice(0, 120));
  if (h) break;
}
srv.proc.kill();
