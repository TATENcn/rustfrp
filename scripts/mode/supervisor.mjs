import * as fs from 'node:fs';
import https from 'node:https';
import http from 'node:http';
import {spawn} from 'node:child_process';
import {root, dir, repoKey, cert, key, stateFile, atomic, identity, backendOwned, portFor, lanIPs, sleep} from './runtime.mjs';
const instanceId = process.argv[2];
if (!/^[a-f0-9-]{36}$/.test(instanceId || '')) throw new Error('Invalid instance ID');
const binary = `${dir}/daemon-${instanceId}`;
let child, server, closing = false;
async function shutdown(code = 0) {
  if (closing) return; closing = true;
  server?.close();
  if (child && child.exitCode === null) {
    child.kill('SIGTERM');
    for (let i = 0; i < 30 && child.exitCode === null; i++) await sleep(100);
    if (child.exitCode === null) child.kill('SIGKILL');
  }
  fs.rmSync(binary, {force: true});
  process.exit(code);
}
process.on('SIGTERM', () => shutdown()); process.on('SIGINT', () => shutdown());
try {
  fs.copyFileSync(`${dir}/daemon`, binary); fs.chmodSync(binary, 0o700);
  // A single backend candidate fails safely on collision; the HTTPS listener retries 16 candidates.
  const backendPort = portFor('daemon');
  const env = {...process.env, RUSTFRP_API_TOKEN_FILE: `${dir}/api-token`, RUSTFRP_NO_AUTO_DOWNLOAD: 'true'};
  delete env.RUSTFRP_API_TOKEN; delete env.RUSTFRP_AUTH_POLICY_FILE;
  child = spawn(binary, ['--db-path', `${dir}/config.db`, '--config-dir', `${dir}/runtime`, '--api-listen', `127.0.0.1:${backendPort}`, '--no-auto-download'], {cwd: root, env, stdio: ['ignore', 'inherit', 'inherit']});
  child.on('error', e => { console.error(e.message); shutdown(1); });
  child.on('exit', () => { if (!closing) shutdown(1); });
  const s = {version: 1, mode: 'local', root, repoKey, instanceId, pid: process.pid, ticks: identity(process.pid).ticks, startedAt: new Date().toISOString(), binary, backendPort};
  for (let i = 0; i < 100; i++) {
    s.backend = identity(child.pid);
    if (backendOwned(s)) break;
    await sleep(100);
  }
  if (!backendOwned(s)) throw new Error('daemon 未监听本仓库端口，可能端口冲突；请检查日志');
  server = https.createServer({key: fs.readFileSync(key), cert: fs.readFileSync(cert)}, (req, res) => {
    if (!backendOwned(s)) { res.writeHead(503); res.end('Backend unavailable'); return; }
    const upstream = http.request({hostname: '127.0.0.1', port: backendPort, path: req.url, method: req.method, headers: {...req.headers, host: `127.0.0.1:${backendPort}`, 'x-forwarded-proto': 'https'}}, incoming => {
      res.writeHead(incoming.statusCode, {...incoming.headers, 'x-rustfrp-instance': instanceId}); incoming.pipe(res);
    });
    upstream.on('error', () => { if (!res.headersSent) res.writeHead(502); res.end('Backend unavailable'); });
    res.on('close', () => upstream.destroy()); req.pipe(upstream);
  });
  // The built UI uses HTTP/SSE, not Vite HMR or WebSockets.
  const base = portFor('https');
  for (let offset = 0; offset < 16; offset++) {
    try {
      await new Promise((resolve, reject) => { server.once('error', reject); server.listen(base + offset, '0.0.0.0', () => { server.removeListener('error', reject); resolve(); }); });
      break;
    } catch (e) { if (e.code !== 'EADDRINUSE' || offset === 15) throw e; }
  }
  s.portBase = base; s.actualPort = server.address().port; s.bindHost = '0.0.0.0';
  s.url = `https://127.0.0.1:${s.actualPort}`; s.lanUrls = lanIPs().map(ip => `https://${ip}:${s.actualPort}`);
  atomic(stateFile, JSON.stringify(s));
  console.log(`${new Date().toISOString()} HTTPS ready: ${s.url}`);
} catch (e) { console.error(e.message); await shutdown(1); }
