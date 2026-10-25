import * as fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash, X509Certificate} from 'node:crypto';
import os from 'node:os';
import https from 'node:https';
import {spawn} from 'node:child_process';
export const root = fs.realpathSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..'));
export const repoKey = createHash('sha256').update(`mode:v1\0${root}`).digest('hex').slice(0, 12);
export const dir = path.join(root, '.local/mode');
export const cert = path.join(dir, 'cert.pem');
export const key = path.join(dir, 'key.pem');
export const log = path.join(dir, 'service.log');
export const stateFile = path.join(dir, 'state.json');
export const cli = path.join(root, 'scripts/mode/cli.mjs');
export const supervisor = path.join(root, 'scripts/mode/supervisor.mjs');
export const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
export function portFor(service) { return 18000 + createHash('sha256').update(`${repoKey}\0${service}`).digest().readUInt32BE(0) % 12000; }
export function lanIPs() { return Object.values(os.networkInterfaces()).flat().filter(i => i && i.family === 'IPv4' && !i.internal).map(i => i.address).sort(); }
export function prepare() { fs.mkdirSync(dir, {recursive: true, mode: 0o700}); }
export function atomic(file, value) { fs.writeFileSync(`${file}.tmp`, value, {mode: 0o600}); fs.renameSync(`${file}.tmp`, file); }
export function readState() { try { return JSON.parse(fs.readFileSync(stateFile, 'utf8')); } catch (e) { if (e.code === 'ENOENT') return null; throw e; } }
export function processExists(pid) { return Number.isInteger(pid) && pid > 1 && fs.existsSync(`/proc/${pid}`); }
export function identity(pid) {
  try {
    const stat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8').split(') ').slice(1).join(') ').split(' ');
    return {pid, ticks: stat[19], group: Number(stat[2]), state: stat[0], cwd: fs.realpathSync(`/proc/${pid}/cwd`), argv: fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8').split('\0')};
  } catch { return null; }
}
export function owned(s) {
  if (!s || s.root !== root || s.repoKey !== repoKey || !Number.isInteger(s.pid) || s.pid <= 1) return false;
  const p = identity(s.pid);
  return !!p && p.state !== 'Z' && p.cwd === root && p.ticks === s.ticks && p.group === s.pid && p.argv.includes(supervisor) && p.argv.includes(s.instanceId);
}
export function backendOwned(s) {
  const p = identity(s.backend?.pid);
  if (!p || p.state === 'Z' || p.ticks !== s.backend.ticks || p.group !== s.pid || p.cwd !== root || !p.argv.includes(s.binary)) return false;
  // Verify the listening socket belongs to this daemon, not a process that won a port race.
  try {
    const inodes = new Set(fs.readdirSync(`/proc/${p.pid}/fd`).flatMap(fd => {
      try { return [fs.readlinkSync(`/proc/${p.pid}/fd/${fd}`).match(/^socket:\[(\d+)\]$/)?.[1]]; } catch { return []; }
    }));
    const address = `0100007F:${s.backendPort.toString(16).toUpperCase().padStart(4, '0')}`;
    return fs.readFileSync('/proc/net/tcp', 'utf8').split('\n').some(line => { const v = line.trim().split(/\s+/); return v[1] === address && v[3] === '0A' && inodes.has(v[9]); });
  } catch { return false; }
}
export function run(argv, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(argv[0], argv.slice(1), {cwd: root, stdio: 'inherit', ...options});
    child.on('error', reject); child.on('exit', (code, signal) => code === 0 ? resolve() : reject(Object.assign(new Error(`${argv[0]} 失败（${signal || code}）`), {exitCode: code || 1})));
  });
}
export async function ensureCertificate() {
  const ips = lanIPs();
  try {
    const x = new X509Certificate(fs.readFileSync(cert));
    if (Date.parse(x.validTo) > Date.now() + 86400000 && x.checkHost('localhost') && ['127.0.0.1', ...ips].every(ip => x.checkIP(ip)) && fs.existsSync(key)) return;
  } catch {}
  await run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '30', '-subj', '/CN=localhost', '-addext', `subjectAltName=DNS:localhost,IP:127.0.0.1${ips.map(ip => `,IP:${ip}`).join('')}`, '-keyout', `${key}.tmp`, '-out', `${cert}.tmp`], {stdio: 'ignore'});
  fs.chmodSync(`${key}.tmp`, 0o600); fs.renameSync(`${key}.tmp`, key); fs.renameSync(`${cert}.tmp`, cert);
}
export function probe(s) {
  return new Promise((resolve, reject) => {
    const request = https.get(`${s.url}/api/v1/health`, {ca: fs.readFileSync(cert), timeout: 2500}, response => {
      let body = ''; response.on('data', chunk => { body += chunk; if (body.length > 65536) request.destroy(new Error('健康响应过大')); });
      response.on('end', () => {
        try { if (response.statusCode !== 200 || response.headers['x-rustfrp-instance'] !== s.instanceId || JSON.parse(body).data?.status !== 'ok') throw new Error('健康响应或实例标识不匹配'); resolve(); } catch (e) { reject(e); }
      });
    });
    request.on('timeout', () => request.destroy(new Error('HTTPS 健康检查超时'))); request.on('error', reject);
  });
}
export async function status() {
  const s = readState(); const running = owned(s);
  const result = {root, repoKey, cert, log, running, healthy: false, message: '服务未启动', url: null, lanUrls: []};
  if (!running) {
    result.unverified = !!s && processExists(s.pid);
    if (s) result.message = result.unverified ? '无法核验进程归属（可能权限不足）；未探测旧端口' : '实例已退出；未探测旧端口';
    return result;
  }
  Object.assign(result, {url: s.url, lanUrls: s.lanUrls});
  try { if (!backendOwned(s)) throw new Error('后端进程或监听归属未通过校验'); await probe(s); result.healthy = true; result.message = 'HTTPS 与 daemon 已就绪'; }
  catch (e) { result.message = e.message; }
  return result;
}
export async function stop() {
  const s = readState();
  if (!s) return;
  if (!owned(s)) {
    if (processExists(s.pid)) throw new Error('实例身份不匹配或权限不足，拒绝停止；请检查 state.json');
    fs.unlinkSync(stateFile); return;
  }
  process.kill(-s.pid, 'SIGTERM');
  for (let i = 0; i < 50 && owned(s); i++) await sleep(100);
  if (owned(s)) { process.kill(-s.pid, 'SIGKILL'); await sleep(200); }
  if (owned(s)) throw new Error('服务停止超时');
  fs.rmSync(stateFile, {force: true});
}
