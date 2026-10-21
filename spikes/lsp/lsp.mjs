// Client LSP minimal pour piloter rust-analyzer dans les spikes.
import { spawn } from 'node:child_process';

export const RA = process.env.RA ??
  `${process.env.HOME}/.vscode-server/extensions/rust-lang.rust-analyzer-0.3.3089-linux-x64/server/rust-analyzer`;

export function startServer(cmd = RA, args = [], opts = {}) {
  const proc = spawn(cmd, args, { stdio: ['pipe', 'pipe', 'pipe'], ...opts });
  let buf = Buffer.alloc(0);
  let nextId = 1;
  const pending = new Map();
  const listeners = [];
  const notifications = [];
  proc.stderr.on('data', d => { if (process.env.LSP_STDERR) process.stderr.write(d); });
  proc.stdout.on('data', chunk => {
    buf = Buffer.concat([buf, chunk]);
    for (;;) {
      const sep = buf.indexOf('\r\n\r\n');
      if (sep < 0) return;
      const len = Number(/Content-Length: (\d+)/i.exec(buf.subarray(0, sep).toString())[1]);
      if (buf.length < sep + 4 + len) return;
      const msg = JSON.parse(buf.subarray(sep + 4, sep + 4 + len).toString());
      buf = buf.subarray(sep + 4 + len);
      if (msg.id !== undefined && msg.method === undefined) {
        const p = pending.get(msg.id);
        pending.delete(msg.id);
        msg.error ? p.reject(new Error(JSON.stringify(msg.error))) : p.resolve(msg.result);
      } else if (msg.id !== undefined) {
        // requête serveur -> client : on répond null
        send({ jsonrpc: '2.0', id: msg.id, result: null });
      } else {
        notifications.push(msg);
        for (const l of listeners) l(msg);
      }
    }
  });
  function send(obj) {
    const s = JSON.stringify(obj);
    proc.stdin.write(`Content-Length: ${Buffer.byteLength(s)}\r\n\r\n${s}`);
  }
  return {
    proc, notifications,
    request(method, params) {
      const id = nextId++;
      send({ jsonrpc: '2.0', id, method, params });
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    notify(method, params) { send({ jsonrpc: '2.0', method, params }); },
    waitFor(pred, timeoutMs = 120000) {
      const hit = notifications.find(pred);
      if (hit) return Promise.resolve(hit);
      return new Promise((resolve, reject) => {
        const t = setTimeout(() => reject(new Error('timeout')), timeoutMs);
        listeners.push(m => { if (pred(m)) { clearTimeout(t); resolve(m); } });
      });
    },
  };
}

export async function initialize(srv, rootPath, initializationOptions = {}) {
  await srv.request('initialize', {
    processId: process.pid,
    rootUri: `file://${rootPath}`,
    workspaceFolders: [{ uri: `file://${rootPath}`, name: 'ws' }],
    capabilities: {
      experimental: { serverStatusNotification: true },
      textDocument: { completion: { completionItem: { snippetSupport: false } },
                      publishDiagnostics: {}, hover: { contentFormat: ['markdown'] } },
      workspace: { workspaceFolders: true },
    },
    initializationOptions,
  });
  srv.notify('initialized', {});
  await srv.waitFor(m => m.method === 'experimental/serverStatus' && m.params.quiescent);
}

export function posOf(text, needle, offsetInNeedle = needle.length) {
  const i = text.indexOf(needle);
  if (i < 0) throw new Error(`introuvable: ${needle}`);
  const before = text.slice(0, i + offsetInNeedle).split('\n');
  return { line: before.length - 1, character: before.at(-1).length };
}
