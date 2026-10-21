// S2 : rust-analyzer résout-il include!(OUT_DIR) + #[path] absolu vers des fichiers .rsc ?
import { readFileSync } from 'node:fs';
import { startServer, initialize, posOf } from './lsp.mjs';

const root = process.argv[2] ?? '/home/rtx/crust/spikes/s1';
const ext = process.argv[3] ?? 'rsc';
const srv = startServer();
const t0 = Date.now();
await initialize(srv, root);
console.log(`indexation: ${Date.now() - t0} ms`);

const syms = await srv.request('workspace/symbol', { query: 'Servomoteur' });
console.log('symboles:', syms.map(s => `${s.name} @ ${(s.location.uri ?? '').replace(root, '')}`));
const servoUri = syms.find(s => s.name === 'Servomoteur')?.location.uri;
if (!servoUri) { console.log('S2 ÉCHEC : type non trouvé'); process.exit(1); }
const genDir = servoUri.replace('file://', '').replace(new RegExp(`/src/robot/Servomoteur\\.${ext}$`), '');
const mainPath = `${genDir}/src/main.${ext}`;
const mainUri = `file://${mainPath}`;
const mainText = readFileSync(mainPath, 'utf8');

const def = await srv.request('textDocument/definition', {
  textDocument: { uri: mainUri }, position: posOf(mainText, 'Servomoteur::new', 14) });
console.log('definition new ->', JSON.stringify(def).replace(new RegExp(genDir, 'g'), '<gen>'));

const hover = await srv.request('textDocument/hover', {
  textDocument: { uri: mainUri }, position: posOf(mainText, 's.parent.vitesse', 4) });
console.log('hover parent ->', hover?.contents?.value?.replace(/\n+/g, ' | '));

const defAcc = await srv.request('textDocument/definition', {
  textDocument: { uri: mainUri }, position: posOf(mainText, 'accelerer', 2) });
console.log('definition accelerer ->', JSON.stringify(defAcc).replace(new RegExp(genDir, 'g'), '<gen>'));

srv.proc.kill();
