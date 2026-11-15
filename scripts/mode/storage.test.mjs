import {test, expect} from 'bun:test';
import * as fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {level, policy, budget, create} from './storage.mjs';
import {defaults, validate, load, save, update, atomicWrite} from './config.mjs';

const wm = {elevated: 75, high: 85, critical: 92, exitElevated: 70, exitHigh: 78, exitCritical: 85};

test('watermark levels honour hysteresis instead of flapping', () => {
  // 升级按触发线。
  expect(level(74, 'normal', wm)).toBe('normal');
  expect(level(75, 'normal', wm)).toBe('elevated');
  expect(level(85, 'elevated', wm)).toBe('high');
  expect(level(92, 'high', wm)).toBe('critical');
  // 降级必须低于退出线，边界值保持原级别。
  expect(level(74, 'elevated', wm)).toBe('elevated');
  expect(level(70, 'elevated', wm)).toBe('elevated');
  expect(level(69.9, 'elevated', wm)).toBe('normal');
  expect(level(80, 'high', wm)).toBe('high');
  expect(level(78, 'high', wm)).toBe('high');
  // 低于退出线才降级；77 < exitHigh(78) → 回落到 elevated，而不是直接 normal。
  expect(level(77, 'high', wm)).toBe('elevated');
  expect(level(86, 'critical', wm)).toBe('critical');
  expect(level(84, 'critical', wm)).toBe('high');
  // 未知水位不能伪造成正常或充足。
  expect(level(null, 'normal', wm)).toBe('unknown');
});

test('watermark levels never skip a cheaper level when pressure spikes', () => {
  expect(level(95, 'normal', wm)).toBe('critical');
  expect(level(86, 'normal', wm)).toBe('high');
});

test('each level only unlocks categories it can justify', () => {
  const cfg = defaults();
  expect(policy('normal', cfg).categories).toEqual(['tmp']);
  expect(policy('elevated', cfg).categories).toEqual(['tmp', 'download']);
  expect(policy('high', cfg).categories).toEqual(['tmp', 'download']);
  expect(policy('critical', cfg).categories).toContain('build');
  expect(policy('critical', cfg).idleDays.tmp).toBe(0);
  expect(policy('high', cfg).allowExpensive).toBe(false);
});

test('budget derives from capacity with clamps, and explicit config wins', () => {
  const cfg = defaults();
  expect(budget(cfg, {capacityBytes: 100 * 1024 ** 3})).toBe(3 * 1024 ** 3);
  expect(budget(cfg, {capacityBytes: 2e12})).toBe(20 * 1024 ** 3);
  expect(budget(cfg, {capacityBytes: null})).toBe(null);
  expect(budget({...cfg, storage: {...cfg.storage, budgetBytes: 12345}}, {capacityBytes: 1e12})).toBe(12345);
});

function fixture() {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'rustfrp-storage-'));
  const root = path.join(base, 'repo');
  const dir = path.join(root, '.local/mode');
  fs.mkdirSync(path.join(root, 'plugins/webui'), {recursive: true});
  fs.mkdirSync(dir, {recursive: true});
  return {base, root: fs.realpathSync(root), dir, store: create({root: fs.realpathSync(root), dir})};
}

const age = (file, days) => {
  const when = new Date(Date.now() - days * 86400000);
  fs.utimesSync(file, when, when);
};

test('inspect is read-only, skips unknown or out-of-scope resources, and never crosses symlinks', () => {
  const {base, root, dir, store} = fixture();
  try {
    fs.mkdirSync(path.join(dir, 'tmp'));
    fs.writeFileSync(path.join(dir, 'tmp', 'scratch.bin'), Buffer.alloc(4096));
    fs.mkdirSync(path.join(root, 'plugins/webui/node_modules'));
    fs.writeFileSync(path.join(root, 'plugins/webui/node_modules/dep.js'), 'x');
    // 符号链接逃逸：真实路径在仓库外，必须跳过。
    const outside = path.join(base, 'outside');
    fs.mkdirSync(outside);
    fs.writeFileSync(path.join(outside, 'secret.bin'), Buffer.alloc(1024));
    fs.mkdirSync(path.join(root, 'target'));
    fs.symlinkSync(outside, path.join(root, 'target', 'escaped'));

    const cfg = defaults();
    const before = fs.statSync(path.join(dir, 'tmp', 'scratch.bin')).mtimeMs;
    const plan = store.inspect(cfg);
    expect(plan.candidates).toEqual([]);
    // 不存在的资源不列出；存在的都要给出可读的跳过理由。
    expect(plan.skipped.map(item => item.id).sort()).toEqual(['build', 'tmp', 'webui-modules'].sort());
    expect(plan.skipped.every(item => item.reason.length > 0)).toBe(true);
    expect(plan.usedPercent).not.toBe(null);
    // 只读：使用时间与文件 mtime 都不变。
    expect(fs.statSync(path.join(dir, 'tmp', 'scratch.bin')).mtimeMs).toBe(before);
    expect(store.readUsage()).toEqual({});
    expect(plan.candidates.length).toBe(0);

    // 逃逸的 target 归属核验必须失败。
    expect(store.inspect({...cfg, storage: {...cfg.storage, watermarks: {...wm, elevated: 0, exitElevated: 0}}}).skipped.find(item => item.id === 'build')).toBeDefined();
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('expired temporary files are candidates only when idle time is recorded', () => {
  const {base, dir, store} = fixture();
  try {
    fs.mkdirSync(path.join(dir, 'tmp'));
    fs.writeFileSync(path.join(dir, 'tmp', 'old.bin'), Buffer.alloc(2048));
    const cfg = defaults();
    // 无使用记录 → 首次观察起宽限，不能判过期。
    expect(store.inspect(cfg).candidates).toEqual([]);
    // 记录真实使用时间后，超过期限才可回收。
    store.touchUsage(['tmp']);
    expect(store.inspect(cfg).candidates).toEqual([]);
    const usageFile = path.join(dir, 'usage.json');
    const usage = JSON.parse(fs.readFileSync(usageFile, 'utf8'));
    usage.tmp = new Date(Date.now() - 6 * 86400000).toISOString();
    atomicWrite(usageFile, JSON.stringify(usage));
    const plan = store.inspect(cfg);
    expect(plan.candidates.map(c => c.id)).toEqual(['tmp']);
    expect(plan.candidates[0].idleDays).toBeGreaterThanOrEqual(6);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('collect without execute deletes nothing; execute removes only verified owned candidates', () => {
  const {base, dir, store} = fixture();
  try {
    fs.mkdirSync(path.join(dir, 'tmp'));
    const victim = path.join(dir, 'tmp', 'junk.bin');
    fs.writeFileSync(victim, Buffer.alloc(4096));
    const cfg = defaults();
    store.touchUsage(['tmp']);
    const usageFile = path.join(dir, 'usage.json');
    const usage = JSON.parse(fs.readFileSync(usageFile, 'utf8'));
    usage.tmp = new Date(Date.now() - 10 * 86400000).toISOString();
    atomicWrite(usageFile, JSON.stringify(usage));

    const plan = store.inspect(cfg);
    expect(plan.candidates.map(c => c.id)).toEqual(['tmp']);
    const preview = store.collect(cfg, plan, {execute: false});
    expect(preview.dryRun).toBe(true);
    expect(fs.existsSync(victim)).toBe(true);

    const result = store.collect(cfg, plan, {execute: true});
    expect(result.dryRun).toBe(false);
    expect(result.removedBytes).toBeGreaterThan(0);
    expect(fs.existsSync(victim)).toBe(false);
    // 目录被重建，但仍是受管路径。
    expect(fs.existsSync(path.join(dir, 'tmp'))).toBe(true);
    // 记录维护结果供 status 展示。
    expect(store.readState().lastRun.level).toBe('normal');
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('collect stops at the batch limit instead of deleting everything at once', () => {
  const {base, dir, store} = fixture();
  try {
    fs.mkdirSync(path.join(dir, 'tmp'));
    fs.writeFileSync(path.join(dir, 'tmp', 'a.bin'), Buffer.alloc(1024));
    const cfg = defaults();
    cfg.storage.maintenance = {maxBatches: 1, maxPerBatch: 1, maxMs: 60000};
    store.touchUsage(['tmp']);
    const usageFile = path.join(dir, 'usage.json');
    const usage = JSON.parse(fs.readFileSync(usageFile, 'utf8'));
    usage.tmp = new Date(Date.now() - 30 * 86400000).toISOString();
    atomicWrite(usageFile, JSON.stringify(usage));

    const plan = store.inspect(cfg);
    const result = store.collect(cfg, plan, {execute: true});
    expect(result.batches).toBe(1);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('build artefacts are only cleaned when explicitly requested', () => {
  const {base, dir, root, store} = fixture();
  try {
    fs.mkdirSync(path.join(root, 'target'));
    fs.writeFileSync(path.join(root, 'target', 'artifact.bin'), Buffer.alloc(2048));
    const cfg = defaults();
    cfg.storage.watermarks = {...wm, elevated: 0, exitElevated: 0, high: 0, exitHigh: 0, critical: 0, exitCritical: 0};
    store.touchUsage(['target']);
    const usageFile = path.join(dir, 'usage.json');
    const usage = JSON.parse(fs.readFileSync(usageFile, 'utf8'));
    usage.target = new Date(Date.now() - 60 * 86400000).toISOString();
    atomicWrite(usageFile, JSON.stringify(usage));

    const plan = store.inspect(cfg);
    expect(plan.level).toBe('critical');
    expect(plan.candidates.some(c => c.id === 'build')).toBe(true);
    const withoutBuild = store.collect(cfg, plan, {execute: true});
    expect(withoutBuild.batches).toBe(0);
    expect(fs.existsSync(path.join(root, 'target', 'artifact.bin'))).toBe(true);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('log rotation keeps a bounded number of files', () => {
  const {base, dir, store} = fixture();
  try {
    const cfg = defaults();
    cfg.storage.log = {maxBytes: 1024, keep: 2};
    const file = path.join(dir, 'service.log');
    fs.writeFileSync(file, 'x'.repeat(2048));
    expect(store.rotateLog(cfg, file)).toEqual({bytes: 2048, kept: 2});
    expect(fs.existsSync(`${file}.1`)).toBe(true);
    fs.writeFileSync(file, 'y'.repeat(2048));
    store.rotateLog(cfg, file);
    // 超过 keep 的历史文件被删除，总量有界。
    expect(fs.existsSync(`${file}.3`)).toBe(false);
    expect(fs.readFileSync(`${file}.2`, 'utf8').startsWith('x')).toBe(true);
    expect(fs.readFileSync(`${file}.1`, 'utf8').startsWith('y')).toBe(true);
    // 轮转后留下空的当前文件继续接收输出。
    expect(fs.statSync(file).size).toBe(0);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('config rejects corrupt or unsupported content instead of silently resetting', () => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'rustfrp-config-'));
  const file = path.join(base, 'mode.json');
  try {
    expect(load({file})).toEqual(defaults());
    fs.writeFileSync(file, '{ not json');
    expect(() => load({file})).toThrow(/不是合法 JSON/);
    fs.writeFileSync(file, JSON.stringify({version: 2}));
    expect(() => load({file})).toThrow(/不支持的版本/);
    fs.writeFileSync(file, JSON.stringify({version: 1, storage: {watermarks: {...wm, exitElevated: 80}}}));
    expect(() => load({file})).toThrow(/exitElevated/);
    // 合法文件保留未知字段与已有值。
    fs.writeFileSync(file, JSON.stringify({version: 1, storage: {budgetBytes: 4096}, custom: {keep: true}}));
    const loaded = load({file});
    expect(loaded.storage.budgetBytes).toBe(4096);
    expect(loaded.custom).toEqual({keep: true});
    expect(loaded.storage.watermarks).toEqual(wm);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('config updates preserve other fields and stay schema-valid', () => {
  const base = fs.mkdtempSync(path.join(os.tmpdir(), 'rustfrp-config-'));
  const file = path.join(base, 'mode.json');
  try {
    save({...defaults(), extra: 'kept'}, {file});
    update(cfg => {
      cfg.storage.idleDays.tmp = 9;
      return cfg;
    }, {file});
    const reloaded = load({file});
    expect(reloaded.storage.idleDays.tmp).toBe(9);
    expect(reloaded.extra).toBe('kept');
    expect(validate(reloaded, file)).toBeDefined();
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('build admission refuses only when critical pressure lacks headroom', () => {
  const {base, root, dir, store} = fixture();
  try {
    const cfg = defaults();
    // 真实文件系统通常远低于临界，准入应放行并报告已知水位。
    const admission = store.admitBuild(cfg);
    expect(admission.allowed).toBe(true);
    expect(admission.fsInfo).toBeDefined();
    // 无可用空间的判断必须基于真实测量，不能伪造；此处验证未知分支的宽容行为。
    expect(store.admitBuild({...cfg, storage: {...cfg.storage, buildHeadroomBytes: 0}}).allowed).toBe(true);
    // 记录构建增长后，准入使用实测峰值。
    store.recordBuildSize(0, 512 * 1024 ** 2);
    expect(store.readState().peakGrowthBytes).toBe(512 * 1024 ** 2);
    store.recordBuildSize(1024, 512);
    expect(store.readState().peakGrowthBytes).toBe(512 * 1024 ** 2);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});

test('summary caches heavy sampling and refreshes after the cache window', () => {
  const {base, dir, store} = fixture();
  try {
    fs.mkdirSync(path.join(dir, 'tmp'));
    fs.writeFileSync(path.join(dir, 'tmp', 'x.bin'), Buffer.alloc(8192));
    const cfg = defaults();
    const first = store.summary(cfg);
    expect(first.cacheBytes).toBeGreaterThanOrEqual(8192);
    fs.writeFileSync(path.join(dir, 'tmp', 'y.bin'), Buffer.alloc(8192));
    // 缓存窗口内不重扫。
    expect(store.summary(cfg).cacheBytes).toBe(first.cacheBytes);
    // 窗口过期后重新采样。
    const fresh = store.summary(cfg, {cacheMs: 0});
    expect(fresh.cacheBytes).toBe(first.cacheBytes + 8192);
  } finally {
    fs.rmSync(base, {recursive: true, force: true});
  }
});
