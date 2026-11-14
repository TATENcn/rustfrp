import * as fs from 'node:fs';
import {spawn} from 'node:child_process';
import {randomUUID, randomBytes} from 'node:crypto';
import {commands, renderer} from './ui.mjs';
import {root, repoKey, dir, log, cli, supervisor, prepare, atomic, readState, owned, identity, run, ensureCertificate, status, stop, sleep, filesystemBytes} from './runtime.mjs';
import {load as loadConfig} from './config.mjs';
import {statusPayload, storage} from './storage.mjs';

const raw = process.argv.slice(2);
const locked = raw[0] === '--locked';
if (locked) raw.shift();
const command = raw.shift() || 'help';
if (raw[0] === '--') raw.shift();
const json = raw.includes('--json');
const execute = raw.includes('--execute');
const includeBuild = raw.includes('--build');
const ui = renderer({json});

/** 每个命令允许的参数；null 表示全部透传。 */
const ARGS = {
  start: [], stop: [], reload: [], restart: [], build: [],
  logs: ['--follow'], status: ['--json'], health: ['--json'], help: ['--json'],
  clean: ['--json', '--execute', '--build'], test: null,
};

try {
  if (!commands[command]) throw new Error(`未知命令：${command}；运行 bun run mode:help`);
  const allowed = ARGS[command];
  if (allowed && raw.some(arg => !allowed.includes(arg))) throw new Error(`不支持的参数：${raw.join(' ')}`);

  if (command === 'help') {
    ui.help();
  } else if (command === 'status' || command === 'health') {
    const payload = await statusPayload();
    ui[command](payload);
    if (command === 'health' && !payload.healthy) process.exitCode = 1;
  } else if (command === 'test') {
    await run(['cargo', 'test', '--lib', ...raw]);
  } else if (command === 'clean') {
    if (process.platform !== 'linux') throw new Error('本机缓存管理依赖 Linux /proc 与 flock；其他平台请使用 cargo clean 与手动清理');
    prepare();
    const config = loadConfig();
    const plan = storage.inspect(config);
    if (!execute) {
      ui.clean(plan);
    } else if (!locked) {
      await run(['flock', '--exclusive', '--nonblock', '--close', `${dir}/operation.lock`, process.execPath, cli, '--locked', 'clean', ...raw]);
    } else {
      const result = storage.collect(config, plan, {execute: true, includeBuild});
      ui.clean(plan, result);
      if (result.goalMet === false) ui.warn('仍有空间压力；检查是否有其他进程占用磁盘');
    }
  } else {
    if (process.platform !== 'linux') throw new Error('本机服务管理依赖 Linux /proc 与 flock；其他平台请使用已有 just 命令');
    prepare();
    if (command === 'logs') {
      // 轮转是同步、只读探测 + 原子重命名；失败不能阻断日志查看。
      try {
        storage.rotateLog(loadConfig());
      } catch {}
      await run(['tail', '-n', '80', ...(raw.includes('--follow') ? ['-F'] : []), log]);
    } else if (!locked) {
      // --close 确保 detached 监督进程不会继承 checkout 锁。
      await run(['flock', '--exclusive', '--nonblock', '--close', `${dir}/operation.lock`, process.execPath, cli, '--locked', command, ...raw]);
    } else {
      const config = loadConfig();
      // 成功更新后先轮转日志，避免持续增长；轮转失败不影响部署结果。
      try {
        storage.rotateLog(config);
      } catch (e) {
        ui.warn(`日志轮转失败（不影响本次操作）：${e.message}`);
      }

      async function build() {
        const admission = storage.admitBuild(config);
        if (!admission.allowed) throw new Error(admission.reason);
        const before = filesystemBytes();
        const bunEnv = {...process.env, BUN_TMPDIR: `${dir}/tmp`, BUN_INSTALL_CACHE_DIR: `${dir}/bun-cache`};
        fs.mkdirSync(bunEnv.BUN_TMPDIR, {recursive: true});
        ui.step('[1/3] 安装锁定的 WebUI 依赖');
        await run([process.execPath, 'install', '--frozen-lockfile'], {cwd: `${root}/plugins/webui`, env: bunEnv});
        storage.touchUsage(['nodeModules', 'bunCache']);
        ui.step('[2/3] 构建 WebUI');
        await run([process.execPath, 'run', 'build'], {cwd: `${root}/plugins/webui`, env: bunEnv});
        ui.step('[3/3] 增量构建 daemon');
        await run(['cargo', 'build', '--locked', '-p', 'rustfrp-daemon', '--features', 'rust-embed/debug-embed'], {env: {...process.env, CARGO_TARGET_DIR: `${root}/target`}});
        storage.touchUsage(['target']);
        fs.copyFileSync(`${root}/target/debug/rustfrp-daemon`, `${dir}/daemon.tmp`);
        fs.chmodSync(`${dir}/daemon.tmp`, 0o700);
        fs.renameSync(`${dir}/daemon.tmp`, `${dir}/daemon`);
        storage.recordBuildSize(before, filesystemBytes());
      }

      async function start() {
        if (!fs.existsSync(`${dir}/daemon`)) throw new Error('尚无构建产物；请运行 bun run mode:start');
        await ensureCertificate();
        if (!fs.existsSync(`${dir}/api-token`)) atomic(`${dir}/api-token`, randomBytes(32).toString('hex'));
        const instance = randomUUID();
        const fd = fs.openSync(log, 'a', 0o600);
        const child = spawn(process.execPath, [supervisor, instance], {cwd: root, detached: true, stdio: ['ignore', fd, fd]});
        fs.closeSync(fd);
        let spawnError;
        child.on('error', e => { spawnError = e; });
        child.unref();
        let success = false;
        try {
          for (let i = 0; i < 150; i++) {
            if (spawnError) throw spawnError;
            if (child.exitCode !== null) throw new Error('启动失败，请运行 bun run mode:logs');
            const s = readState();
            if (s?.instanceId === instance && (await status()).healthy) {
              success = true;
              return;
            }
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

      if (command === 'build') {
        await build();
        ui.step('✓ 构建完成；bun run mode:restart 使用新产物');
      } else if (command === 'stop') {
        ui.step('停止本仓库服务');
        await stop();
        ui.status(await statusPayload());
      } else if (command === 'start' && owned(readState())) {
        ui.status(await statusPayload());
      } else {
        if (command === 'start' || command === 'reload') await build();
        if (command === 'restart' && !fs.existsSync(`${dir}/daemon`)) throw new Error('尚无构建产物；请运行 bun run mode:start');
        await stop();
        ui.step('启动 HTTPS 预览');
        await start();
        ui.status(await statusPayload());
      }
    }
  }
} catch (e) {
  if (json) console.log(JSON.stringify({error: e.message}));
  console.error(`错误：${e.message}\n下一步：bun run mode:help · bun run mode:logs`);
  process.exitCode = e.exitCode || 1;
}
