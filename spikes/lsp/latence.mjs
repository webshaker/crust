// Latence ajoutée par crust-lsp : 100 survols sur un .rs (relais) et sur un .rsc (converti).
import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';
const [root, server, file, needle] = process.argv.slice(2);
const srv = startServer(server);
await initialize(srv, root);
if (server.includes('crust-lsp')) await srv.waitFor(m => m.method === 'crust/status', 300000);
const f = `${root}/${file}`, t = readFileSync(f, 'utf8');
srv.notify('textDocument/didOpen', { textDocument: { uri: `file://${f}`, languageId: 'rust', version: 1, text: t } });
const req = () => srv.request('textDocument/hover', { textDocument: { uri: `file://${f}` }, position: posOf(t, needle, 1) });
for (let i = 0; i < 40 && !(await req()); i++) await new Promise(r => setTimeout(r, 500));
const times = [];
for (let i = 0; i < 100; i++) { const s = process.hrtime.bigint(); await req(); times.push(Number(process.hrtime.bigint() - s) / 1e6); }
times.sort((a, b) => a - b);
console.log(`${server.split('/').pop()} ${file} : médiane ${times[50].toFixed(2)} ms, 99e centile ${times[98].toFixed(2)} ms`);
srv.proc.kill();
