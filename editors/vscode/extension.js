// Extension VS Code de Crust (section 20.4 du cahier des charges).
//
// rust-analyzer fait l'essentiel : cette extension associe les .rsc au langage
// Rust, propose de faire passer rust-analyzer par crust-lsp, ajoute la commande
// « afficher le Rust généré » et la correspondance des sources pour le débogueur.

const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');

const EXPAND_SCHEME = 'crust-expand';

/** Binaire `name` : réglage, puis ~/.cargo/bin, puis le PATH. */
function tool(setting, name) {
  const configured = vscode.workspace.getConfiguration('crust').get(setting);
  if (configured) return configured;
  const local = path.join(os.homedir(), '.cargo', 'bin', process.platform === 'win32' ? `${name}.exe` : name);
  return fs.existsSync(local) ? local : name;
}

/** Un Cargo.toml du workspace a-t-il crust-build en dépendance de build ? */
async function isCrustWorkspace() {
  const manifests = await vscode.workspace.findFiles('**/Cargo.toml', '{**/target/**,**/tests/**,**/node_modules/**}', 200);
  for (const uri of manifests) {
    const text = (await vscode.workspace.fs.readFile(uri)).toString();
    const build = text.split(/^\[/m).find(section => section.startsWith('build-dependencies]'));
    if (build && /^\s*crust-build\s*=/m.test(build)) return true;
  }
  return false;
}

async function setup(context, ask) {
  const ra = vscode.workspace.getConfiguration('rust-analyzer');
  const lsp = tool('lspPath', 'crust-lsp');
  if (ra.inspect('server.path')?.workspaceValue === lsp) return;
  if (ask) {
    if (context.workspaceState.get('crust.setupDeclined')) return;
    const choice = await vscode.window.showInformationMessage(
      'Projet Crust détecté : faire passer rust-analyzer par crust-lsp pour comprendre les fichiers .rsc ?',
      'Activer', 'Plus tard', 'Jamais pour ce workspace');
    if (choice === 'Jamais pour ce workspace') await context.workspaceState.update('crust.setupDeclined', true);
    if (choice !== 'Activer') return;
  }
  await ra.update('server.path', lsp, vscode.ConfigurationTarget.Workspace);
  await vscode.commands.executeCommand('rust-analyzer.restartServer').then(undefined, () => {});
  vscode.window.showInformationMessage('Crust : rust-analyzer utilise maintenant crust-lsp dans ce workspace.');
}

/** Rust généré, en lecture seule, actualisé à l'enregistrement du .rsc. */
class ExpandProvider {
  constructor() {
    this.emitter = new vscode.EventEmitter();
    this.onDidChange = this.emitter.event;
  }
  provideTextDocumentContent(uri) {
    const file = uri.query;
    return new Promise(resolve => {
      cp.execFile(tool('cargoCrustPath', 'cargo-crust'), ['crust', 'expand', file],
        { cwd: path.dirname(file), maxBuffer: 64 * 1024 * 1024 },
        (err, stdout, stderr) => resolve(stdout || `// cargo crust expand a échoué\n// ${(stderr || String(err)).replace(/\n/g, '\n// ')}`));
    });
  }
}

/** sourceMap pour CodeLLDB : <OUT_DIR>/crust/src → <paquet>/src (fichiers de même nom, mêmes lignes). */
function crustSourceMap() {
  const map = {};
  for (const folder of vscode.workspace.workspaceFolders || []) {
    const targets = [path.join(folder.uri.fsPath, 'target')];
    if (process.env.CARGO_TARGET_DIR) targets.push(process.env.CARGO_TARGET_DIR);
    for (const target of targets) {
      const profiles = safeList(target).flatMap(p => [path.join(target, p), ...safeList(path.join(target, p)).map(q => path.join(target, p, q))]);
      for (const profile of profiles) {
        const build = path.join(profile, 'build');
        for (const pkg of safeList(build)) {
          const crust = path.join(build, pkg, 'out', 'crust');
          try {
            const src = fs.readFileSync(path.join(crust, 'source_dir'), 'utf8').trim();
            map[path.join(crust, 'src')] = src;
          } catch { /* pas un paquet Crust */ }
        }
      }
    }
  }
  return map;
}

function safeList(dir) {
  try { return fs.readdirSync(dir); } catch { return []; }
}

function activate(context) {
  const provider = new ExpandProvider();
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(EXPAND_SCHEME, provider),
    vscode.commands.registerCommand('crust.expand', async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !editor.document.fileName.endsWith('.rsc')) return;
      const file = editor.document.fileName;
      const uri = vscode.Uri.from({ scheme: EXPAND_SCHEME, path: `/${path.basename(file, '.rsc')}.rs`, query: file });
      const doc = await vscode.workspace.openTextDocument(uri);
      await vscode.languages.setTextDocumentLanguage(doc, 'rust');
      await vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside, preview: true, preserveFocus: true });
    }),
    vscode.workspace.onDidSaveTextDocument(doc => {
      if (!doc.fileName.endsWith('.rsc')) return;
      for (const d of vscode.workspace.textDocuments) {
        if (d.uri.scheme === EXPAND_SCHEME && d.uri.query === doc.fileName) provider.emitter.fire(d.uri);
      }
    }),
    vscode.commands.registerCommand('crust.setup', () => setup(context, false)),
    vscode.debug.registerDebugConfigurationProvider('lldb', {
      resolveDebugConfiguration(_folder, config) {
        config.sourceMap = { ...crustSourceMap(), ...(config.sourceMap || {}) };
        return config;
      },
    }),
  );
  isCrustWorkspace().then(yes => yes && setup(context, true));
}

function deactivate() {}

module.exports = { activate, deactivate };
