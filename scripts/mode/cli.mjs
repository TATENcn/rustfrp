import * as fs from 'node:fs';
import {spawn} from 'node:child_process';
import {randomUUID, randomBytes} from 'node:crypto';
import {commands, renderer} from './ui.mjs';
import {root, repoKey, dir, log, cli, supervisor, prepare, atomic, readState, owned, identity, run, ensureCertificate, status, stop, sleep} from './runtime.mjs';
const raw = process.argv.slice(2);
const locked = raw[0] === '--locked'; if (locked) raw.shift();
const command = raw.shift() || 'help'; if (raw[0] === '--') raw.shift();
const json = raw.includes('--json');
const ui = renderer({json});
try {
  if (!commands[command]) throw new Error(`未知命令：${command}；运行 bun run mode:help`);
  const allowed = ['status', 'health', 'help'].includes(command) ? ['--json'] : command === 'logs' ? ['--follow'] : [];
  if (command !== 'test' && raw.some(arg => !allowed.includes(arg))) throw new Error(`不支持的参数：${raw.join(' ')}`);
  if (command === 'help') ui.help();
  else if (command === 'status' || command === 'health') {
    const s = await status(); ui[command](s); if (command === 'health' && !s.healthy) process.exitCode = 1;
  } else if (command === 'test') await run(['cargo', 'test', '--lib', ...raw]);
  else {
    if (process.platform !== 'linux') throw new Error('本机服务管理依赖 Linux /proc 与 flock；其他平台请使用已有 just 命令');
    prepare();
    if (command === 'logs') await run(['tail', '-n', '80', ...(raw.includes('--follow') ? ['-F'] : []), log]);
    else if (!locked) {
      // --close ensures the detached supervisor cannot inherit the checkout lock.
      await run(['flock', '--exclusive', '--nonblock', '--close', `${dir}/operation.lock`, process.execPath, cli, '--locked', command, ...raw]);
    } else {
      async function build() {
        const bunEnv = {...process.env, BUN_TMPDIR: `${dir}/tmp`, BUN_INSTALL_CACHE_DIR: `${dir}/bun-cache`};
        fs.mkdirSync(bunEnv.BUN_TMPDIR, {recursive: true});
        ui.step('[1/3] 安装锁定的 WebUI 依赖');
        await run([process.execPath, 'install', '--frozen-lockfile'], {cwd: `${root}/plugins/webui`, env: bunEnv});
        ui.step('[2/3] 构建 WebUI');
        await run([process.execPath, 'run', 'build'], {cwd: `${root}/plugins/webui`, env: bunEnv});
        ui.step('[3/3] 增量构建 daemon');
        await run(['cargo', 'build', '--locked', '-p', 'rustfrp-daemon', '--features', 'rust-embed/debug-embed'], {env: {...process.env, CARGO_TARGET_DIR: `${root}/target`}});
        fs.copyFileSync(`${root}/target/debug/rustfrp-daemon`, `${dir}/daemon.tmp`);
        fs.chmodSync(`${dir}/daemon.tmp`, 0o700); fs.renameSync(`${dir}/daemon.tmp`, `${dir}/daemon`);
      }
      async function start() {
        if (!fs.existsSync(`${dir}/daemon`)) throw new Error('尚无构建产物；请运行 bun run mode:start');
        await ensureCertificate();
        if (!fs.existsSync(`${dir}/api-token`)) atomic(`${dir}/api-token`, randomBytes(32).toString('hex'));
        const instance = randomUUID(); const fd = fs.openSync(log, 'a', 0o600);
        const child = spawn(process.execPath, [supervisor, instance], {cwd: root, detached: true, stdio: ['ignore', fd, fd]});
        fs.closeSync(fd);
        let spawnError; child.on('error', e => { spawnError = e; }); child.unref();
        let success = false;
        try {
          for (let i = 0; i < 150; i++) {
            if (spawnError) throw spawnError;
            if (child.exitCode !== null) throw new Error('启动失败，请运行 bun run mode:logs');
            const s = readState();
            if (s?.instanceId === instance && (await status()).healthy) { success = true; return; }
            await sleep(100);
          }
          throw new Error('启动超时，请运行 bun run mode:logs');
        } finally {
          if (!success) {
            const p = identity(child.pid);
            const s = {root, repoKey, instanceId: instance, pid: child.pid, ticks: p?.ticks};
            if (owned(s)) {
              process.kill(-child.pid, 'SIGTERM');
              for (let i = 0; i < 50 && owned(s); i++) await sleep(100);
              if (owned(s)) process.kill(-child.pid, 'SIGKILL');
            }
          }
        }
      }
      if (command === 'build') { await build(); ui.step('✓ 构建完成；bun run mode:restart 使用新产物'); }
      else if (command === 'stop') { ui.step('停止本仓库服务'); await stop(); ui.status(await status()); }
      else if (command === 'start' && owned(readState())) ui.status(await status());
      else {
        if (command === 'start' || command === 'reload') await build();
        if (command === 'restart' && !fs.existsSync(`${dir}/daemon`)) throw new Error('尚无构建产物；请运行 bun run mode:start');
        await stop(); ui.step('启动 HTTPS 预览'); await start(); ui.status(await status());
      }
    }
  }
} catch (e) {
  if (json) console.log(JSON.stringify({error: e.message}));
  console.error(`错误：${e.message}\n下一步：bun run mode:help · bun run mode:logs`);
  process.exitCode = e.exitCode || 1;
}
