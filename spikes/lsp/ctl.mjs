import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';
const root = '/home/rtx/crust/spikes/ctl';
const f = `${root}/src/main.rs`;
const srv = startServer();
await initialize(srv, root);
let text = readFileSync(f, 'utf8');
srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text } });
text = text.replace('s.turbo();', 's.turbo();\n    s.');
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${f}`, version: 2 }, contentChanges: [{ text }] });
for (const wait of [0, 1000, 3000]) {
  await new Promise(r => setTimeout(r, wait));
  const comp = await srv.request('textDocument/completion', { textDocument: { uri: `file://${f}` }, position: posOf(text, '\n    s.') });
  console.log(wait, (comp?.items ?? comp).map(i => i.label).slice(0, 8));
}
srv.proc.kill();
