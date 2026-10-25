import {test, expect} from 'bun:test';
import * as fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import https from 'node:https';
import {spawnSync} from 'node:child_process';
import {root, repoKey, portFor, run, identity, owned} from './runtime.mjs';

test('argv and nonzero task exit codes survive subprocess execution', async () => {
  await run([process.execPath, '-e', 'if(process.argv[1] !== "with spaces") process.exit(9)', 'with spaces'], {stdio: 'pipe'});
  try { await run([process.execPath, '-e', 'process.exit(17)'], {stdio: 'pipe'}); throw new Error('Expected failure'); }
  catch (e) { expect(e.exitCode).toBe(17); }
});
test('current unrelated process and stale PID identities are never owned', () => {
  const p = identity(process.pid);
  expect(owned({...p, root, repoKey, instanceId: 'fake'})).toBe(false);
  expect(owned({root, repoKey, pid: 1, ticks: '0'})).toBe(false);
  expect(portFor('https')).not.toBe(portFor('daemon'));
});

const built = fs.existsSync(`${root}/.local/mode/daemon`);
(built ? test : test.skip)('isolated real daemon: TLS, collisions, restart, health forgery, stop and data retention', async () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'rustfrp-mode-'));
  fs.mkdirSync(`${temp}/scripts`, {recursive: true});
  fs.cpSync(`${root}/scripts/mode`, `${temp}/scripts/mode`, {recursive: true});
  fs.mkdirSync(`${temp}/.local/mode`, {recursive: true});
  fs.copyFileSync(`${root}/.local/mode/daemon`, `${temp}/.local/mode/daemon`);
  const exec = (...args) => spawnSync(process.execPath, [`${temp}/scripts/mode/cli.mjs`, ...args], {cwd: temp, encoding: 'utf8', timeout: 25000});
  const state = () => JSON.parse(fs.readFileSync(`${temp}/.local/mode/state.json`, 'utf8'));
  const port = Number(spawnSync(process.execPath, ['-e', `import {portFor} from ${JSON.stringify(`${temp}/scripts/mode/runtime.mjs`)}; console.log(portFor('https'))`], {encoding: 'utf8'}).stdout.trim());
  const blocker = net.createServer();
  let validState;
  const request = (s, options = {}, url = s.url) => new Promise((resolve, reject) => {
    const req = https.get(url, {ca: fs.readFileSync(`${temp}/.local/mode/cert.pem`), timeout: 2000, ...options}, res => { res.resume(); res.on('end', () => resolve(res.statusCode)); });
    req.on('error', reject); req.on('timeout', () => req.destroy(new Error('timeout')));
  });
  try {
    await new Promise((resolve, reject) => { blocker.once('error', reject); blocker.listen(port, '0.0.0.0', resolve); });
    const start = exec('restart'); expect(start.status).toBe(0);
    const first = state(); validState = first; expect(first.actualPort).toBeGreaterThan(port); expect(first.repoKey).not.toBe(repoKey);
    expect(exec('health', '--json').status).toBe(0);
    expect(exec('reload').status).not.toBe(0); // Fixture intentionally has no WebUI build inputs.
    expect(exec('health', '--json').status).toBe(0);
    expect(state().instanceId).toBe(first.instanceId);
    const concurrent = spawnSync('flock', [`${temp}/.local/mode/operation.lock`, process.execPath, `${temp}/scripts/mode/cli.mjs`, 'restart'], {encoding: 'utf8', timeout: 5000});
    expect(concurrent.status).not.toBe(0);
    expect(state().instanceId).toBe(first.instanceId);
    expect(await request(first)).toBe(200);
    if (first.lanUrls.length) expect(await request(first, {}, first.lanUrls[0])).toBe(200);
    await expect(request(first, {ca: []})).rejects.toThrow();
    await expect(request(first, {servername: 'wrong.example'})).rejects.toThrow();
    const wrongHeader = spawnSync(process.execPath, ['-e', `import {probe} from ${JSON.stringify(`${temp}/scripts/mode/runtime.mjs`)}; await probe(${JSON.stringify({...first, instanceId: 'wrong'})})`], {encoding: 'utf8'});
    expect(wrongHeader.status).not.toBe(0);
    const wrongBackend = {...first, backendPort: first.backendPort + 1};
    fs.writeFileSync(`${temp}/.local/mode/state.json`, JSON.stringify(wrongBackend));
    expect(exec('health', '--json').status).toBe(1);
    const forged = {...first, instanceId: 'wrong-instance'};
    fs.writeFileSync(`${temp}/.local/mode/state.json`, JSON.stringify(forged));
    expect(exec('health', '--json').status).toBe(1);
    expect(exec('stop').status).toBe(1);
    fs.writeFileSync(`${temp}/.local/mode/state.json`, JSON.stringify(first));
    expect(exec('restart').status).toBe(0);
    validState = state();
    expect(state().instanceId).not.toBe(first.instanceId);
    expect(exec('stop').status).toBe(0);
    expect(exec('health', '--json').status).toBe(1);
    expect(fs.existsSync(`${temp}/.local/mode/config.db`)).toBe(true);
    expect(blocker.listening).toBe(true);
  } finally {
    if (validState && fs.existsSync(`${temp}/.local/mode/state.json`)) fs.writeFileSync(`${temp}/.local/mode/state.json`, JSON.stringify(validState));
    const stopped = exec('stop'); blocker.close();
    if (stopped.status === 0) fs.rmSync(temp, {recursive: true, force: true});
  }
}, 60000);
