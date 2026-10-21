// Diagnostics natifs (pull) sur un overlay de fichier généré.
import { readFileSync } from 'node:fs';
import { startServer, initialize } from './lsp.mjs';
const [root, f, from, to] = process.argv.slice(2);
const srv = startServer();
await initialize(srv, root);
let text = readFileSync(f, 'utf8');
srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text } });
text = text.replace(from, to);
srv.notify('textDocument/didChange', { textDocument: { uri: `file://${f}`, version: 2 }, contentChanges: [{ text }] });
let r; for (let i = 0; i < 10; i++) { r = await srv.request('textDocument/diagnostic', { textDocument: { uri: `file://${f}` } }).catch(e => ({ items: [e.message] })); if (!String(r.items[0]).includes('cancelled')) break; await new Promise(x => setTimeout(x, 500)); }
console.log(f.replace('/home/rtx/crust/spikes/', ''), '->', r.items.map(d => d.range ? `${d.range.start.line + 1}:${d.range.start.character + 1} ${d.code} ${d.message.split('\n')[0]}` : d));
srv.proc.kill();
