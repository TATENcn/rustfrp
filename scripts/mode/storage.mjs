// 磁盘与缓存生命周期：水位分级 + 滞回、最后使用时间过期、按重建成本渐进回收。
//
// 边界：只处理本管理器能核验归属的可重建产物（都在仓库内、被 Git 忽略）。
// 不碰源码、配置、数据库、证书私钥与任何非本仓库目录；不调用无范围 prune。
//
// 用工厂暴露实例，便于测试在隔离目录中验证策略而无需真实磁盘压力。
import * as fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import * as runtime from './runtime.mjs';
import * as config from './config.mjs';

const require = createRequire(import.meta.url);
const {spawnSync} = require('node:child_process');

const GiB = 1024 ** 3;
const DAY = 86400000;
const LEVELS = ['normal', 'elevated', 'high', 'critical'];

const LEVEL_LABEL = {normal: '正常', elevated: '偏高', high: '高', critical: '临界', unknown: '未知'};

const POLICY_TEXT = {
  normal: '仅回收过期临时产物',
  elevated: '回收过期临时与下载缓存',
  high: '放宽到已过期下载缓存',
  critical: '优先回收，含构建产物',
  unknown: '水位未知，保守跳过',
};

const exists = p => {
  try {
    fs.lstatSync(p);
    return true;
  } catch {
    return false;
  }
};

/** 可重建产物登记表。真实路径在采样时逐项核验，不信任配置。 */
const resourceList = (root, dir) => [
  {id: 'tmp', label: '临时工作目录', path: path.join(dir, 'tmp'), category: 'tmp', usageKey: 'tmp'},
  {id: 'bun-cache', label: 'Bun 安装缓存', path: path.join(dir, 'bun-cache'), category: 'download', usageKey: 'bunCache'},
  {id: 'webui-modules', label: 'WebUI 依赖', path: path.join(root, 'plugins/webui/node_modules'), category: 'download', usageKey: 'nodeModules'},
  // 运行中的实例使用 .local/mode/daemon-<uuid> 副本，因此 target 可回收。
  {id: 'build', label: 'Rust 构建产物', path: path.join(root, 'target'), category: 'build', usageKey: 'target', expensive: true},
];

function readJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return fallback;
  }
}

/** 字节数；未知返回 null（不能当作 0 或充足）。 */
function bytes(p) {
  let stat;
  try {
    stat = fs.lstatSync(p);
  } catch (e) {
    return e.code === 'ENOENT' ? 0 : null;
  }
  if (!stat.isDirectory()) return stat.size;
  try {
    const result = spawnSync('du', ['-s', '--block-size=1', p], {encoding: 'utf8', timeout: 20000});
    if (result.status !== 0) return null;
    return Number(result.stdout.trim().split(/\s+/)[0]) || 0;
  } catch {
    return null;
  }
}

/** 水位分级，带滞回：升级看触发线，降级看更低的退出线。 */
export function level(usedPercent, previous, wm) {
  if (usedPercent === null || usedPercent === undefined) return 'unknown';
  let rank = Math.max(0, LEVELS.indexOf(previous));
  const exits = [wm.exitElevated, wm.exitHigh, wm.exitCritical];
  const triggers = [wm.elevated, wm.high, wm.critical];
  while (rank > 0 && usedPercent < exits[rank - 1]) rank -= 1;
  while (rank < 3 && usedPercent >= triggers[rank]) rank += 1;
  return LEVELS[rank];
}

/** 各级别允许回收的类别与闲置期限。 */
export function policy(levelName, config) {
  const {idleDays} = config.storage;
  switch (levelName) {
    case 'elevated':
    case 'high':
      return {categories: ['tmp', 'download'], idleDays: {tmp: idleDays.tmp, download: idleDays.download}, allowExpensive: false};
    case 'critical':
      return {categories: ['tmp', 'download', 'build'], idleDays: {tmp: 0, download: 0, build: 0}, allowExpensive: true};
    case 'normal':
    default:
      return {categories: ['tmp'], idleDays: {tmp: idleDays.tmp}, allowExpensive: false};
  }
}

/** 默认预算：容量 2%，夹在 3–20 GiB 之间。 */
export function budget(config, fsInfo) {
  if (config.storage.budgetBytes > 0) return config.storage.budgetBytes;
  if (!fsInfo?.capacityBytes) return null;
  return Math.min(20 * GiB, Math.max(3 * GiB, Math.round(fsInfo.capacityBytes * 0.02)));
}

/**
 * 为一个仓库目录创建管理器实例。
 * @param {string} baseRoot 仓库根（必须已 realpath）
 * @param {string} baseDir  运行状态目录（.local/mode）
 */
export function create({root, dir}) {
  const usageFile = path.join(dir, 'usage.json');
  const stateFile = path.join(dir, 'storage.json');
  const logFile = path.join(dir, 'service.log');
  const resources = resourceList(root, dir);

  const writeJson = (file, value) => {
    fs.mkdirSync(dir, {recursive: true, mode: 0o700});
    config.atomicWrite(file, `${JSON.stringify(value, null, 2)}\n`);
  };

  const readUsage = () => readJson(usageFile, {});
  const readState = () => readJson(stateFile, {});
  const writeState = state => writeJson(stateFile, state);

  /** 记录真实使用时间（依赖安装、构建成功时调用），不使用 atime。 */
  function touchUsage(keys) {
    if (!keys.length) return;
    const usage = readUsage();
    const now = new Date().toISOString();
    for (const key of keys) usage[key] = now;
    writeJson(usageFile, usage);
  }

  /** 首次观察时间：无使用记录的既有缓存从观察到起给宽限期，不直接判过期。 */
  function observe() {
    const state = readState();
    const firstSeen = {...(state.firstSeen || {})};
    let changed = false;
    for (const resource of resources) {
      const present = exists(resource.path);
      if (present && !firstSeen[resource.id]) {
        firstSeen[resource.id] = new Date().toISOString();
        changed = true;
      } else if (!present && firstSeen[resource.id]) {
        delete firstSeen[resource.id];
        changed = true;
      }
    }
    if (changed) writeState({...state, firstSeen});
    return firstSeen;
  }

  /** 归属核验：真实路径必须落在仓库内，且不经过符号链接逃逸。 */
  function owned(target) {
    if (!exists(target)) return false;
    try {
      const real = fs.realpathSync(target);
      const realRoot = fs.realpathSync(root);
      return real !== realRoot && real.startsWith(`${realRoot}${path.sep}`);
    } catch {
      return false;
    }
  }

  /** 文件系统水位，同一文件系统去重；查询失败标记 unknown。 */
  function filesystems() {
    const seen = new Map();
    const realRoot = exists(root) ? fs.realpathSync(root) : root;
    for (const candidate of [root, dir, path.join(root, 'target'), path.join(root, 'plugins/webui')]) {
      const probe = exists(candidate) ? fs.realpathSync(candidate) : realRoot;
      let stat;
      let statfs;
      try {
        stat = fs.statSync(probe);
        statfs = fs.statfsSync(probe);
      } catch {
        continue;
      }
      if (seen.has(stat.dev)) continue;
      const capacityBytes = statfs.bsize * statfs.blocks;
      const availBytes = statfs.bsize * statfs.bavail;
      seen.set(stat.dev, {
        dev: stat.dev,
        label: probe === realRoot ? '仓库所在文件系统' : path.relative(root, probe) || probe,
        capacityBytes,
        availBytes,
        usedPercent: capacityBytes > 0 ? Number((((capacityBytes - availBytes) / capacityBytes) * 100).toFixed(1)) : null,
      });
    }
    return [...seen.values()];
  }

  /** 只读采样：候选、跳过原因、估算回收量。不修改使用时间。 */
  function inspect(cfg = config.load()) {
    const firstSeen = observe();
    const usage = readUsage();
    const fsList = filesystems();
    const fsInfo = fsList[0] || null;
    const current = level(fsInfo?.usedPercent ?? null, readState().level, cfg.storage.watermarks);
    const rules = policy(current, cfg);
    const now = Date.now();
    const candidates = [];
    const skipped = [];

    for (const resource of resources) {
      if (!exists(resource.path)) continue;
      const size = bytes(resource.path);
      if (!owned(resource.path)) {
        skipped.push({id: resource.id, label: resource.label, reason: '真实路径不在仓库内或经过符号链接逃逸'});
        continue;
      }
      if (size === null) {
        skipped.push({id: resource.id, label: resource.label, reason: '空间统计失败，按未知处理'});
        continue;
      }
      if (!rules.categories.includes(resource.category)) {
        skipped.push({id: resource.id, label: resource.label, reason: `类别 ${resource.category} 不在${LEVEL_LABEL[current]}水位级别范围`});
        continue;
      }
      const recorded = usage[resource.usageKey] ? Date.parse(usage[resource.usageKey]) : null;
      const observed = firstSeen[resource.id] ? Date.parse(firstSeen[resource.id]) : null;
      const lastUse = recorded ?? observed;
      if (lastUse === null) {
        skipped.push({id: resource.id, label: resource.label, reason: '最后使用时间未知，保守跳过'});
        continue;
      }
      const idleMs = Math.max(0, now - lastUse);
      const requiredDays = rules.idleDays[resource.category];
      if (idleMs < requiredDays * DAY) {
        skipped.push({id: resource.id, label: resource.label, reason: `未到占用期限（闲置 ${Math.floor(idleMs / DAY)} 天，需 ${requiredDays} 天）`});
        continue;
      }
      candidates.push({
        id: resource.id,
        label: resource.label,
        path: resource.path,
        category: resource.category,
        expensive: !!resource.expensive,
        bytes: size,
        lastUse: new Date(lastUse).toISOString(),
        idleDays: Math.floor(idleMs / DAY),
        source: recorded ? '管理器记录' : '首次观察（宽限）',
      });
    }

    // 崩溃遗留的运行副本：未被任何进程持有的，任何级别都可回收。
    for (const item of staleInstances()) {
      candidates.push({...item, idleDays: 0, source: '无进程持有'});
    }

    // 便宜且释放多的先回收；昂贵产物排最后。
    candidates.sort((a, b) => Number(a.expensive) - Number(b.expensive) || b.bytes - a.bytes);
    return {
      level: current,
      usedPercent: fsInfo?.usedPercent ?? null,
      capacityBytes: fsInfo?.capacityBytes ?? null,
      availBytes: fsInfo?.availBytes ?? null,
      filesystems: fsList,
      budgetBytes: budget(cfg, fsInfo),
      watermarks: cfg.storage.watermarks,
      policy: rules,
      candidates,
      skipped,
      logMaxBytes: cfg.storage.log.maxBytes,
      logKeep: cfg.storage.log.keep,
    };
  }

  function removeResource(target) {
    fs.rmSync(target, {recursive: true, force: true});
    fs.mkdirSync(target, {recursive: true, mode: 0o700});
    return true;
  }

  /**
   * 崩溃遗留的运行副本（daemon-<uuid>）。只有确认没有进程带有该文件路径时才删除；
   * 这些副本由 .local/mode/daemon 复制而来，重建成本为零。
   */
  function staleInstances() {
    let entries;
    try {
      entries = fs.readdirSync(dir);
    } catch {
      return [];
    }
    const holders = new Set();
    try {
      for (const pid of fs.readdirSync('/proc')) {
        if (!/^\d+$/.test(pid)) continue;
        let cmdline;
        try {
          cmdline = fs.readFileSync(`/proc/${pid}/cmdline`, 'utf8');
        } catch {
          continue;
        }
        for (const match of cmdline.matchAll(/daemon-[a-f0-9-]{36}/g)) holders.add(match[0]);
      }
    } catch {
      return [];
    }
    return entries
      .filter(name => /^daemon-[a-f0-9-]{36}$/.test(name) && !holders.has(name))
      .map(name => {
        const file = path.join(dir, name);
        let size = 0;
        try {
          size = fs.statSync(file).size;
        } catch {}
        return {id: 'instances', label: '遗留运行副本', path: file, category: 'instances', expensive: false, bytes: size};
      });
  }

  function pruneInstances(list = staleInstances()) {
    let removed = 0;
    let count = 0;
    for (const item of list) {
      try {
        fs.rmSync(item.path, {force: true});
        removed += item.bytes;
        count += 1;
      } catch {}
    }
    return {count, removed};
  }

  /** 构建产物走 cargo clean（受支持的清理方式），不手删底层目录。 */
  function cleanBuild() {
    const result = spawnSync('cargo', ['clean', '--manifest-path', path.join(root, 'Cargo.toml')], {cwd: root, encoding: 'utf8', timeout: 900000});
    if (result.status !== 0) return false;
    fs.mkdirSync(path.join(root, 'target'), {recursive: true});
    return true;
  }

  /**
   * 执行回收。调用方必须持有仓库锁（与构建互斥）。
   * 分批 + 上限；每批后重新测量，达到退出线即停止。
   */
  function collect(cfg, plan, {execute = false, includeBuild = false} = {}) {
    if (!execute) return {dryRun: true, results: plan.candidates, removedBytes: 0, goalMet: null, batches: 0};
    const {maxBatches, maxPerBatch, maxMs} = cfg.storage.maintenance;
    const started = Date.now();
    const wm = cfg.storage.watermarks;
    const exitPercent = plan.level === 'critical' ? wm.exitCritical : plan.level === 'high' ? wm.exitHigh : wm.exitElevated;
    // 遗留副本与临时产物始终在范围内；昂贵构建产物需要显式授权。
    const targets = plan.candidates.filter(c => c.category === 'instances' || includeBuild || !c.expensive);
    const results = [];
    let removedBytes = 0;
    let batches = 0;
    let index = 0;

    while (index < targets.length && batches < maxBatches && Date.now() - started < maxMs) {
      const slice = targets.slice(index, index + maxPerBatch);
      index += slice.length;
      batches += 1;
      for (const candidate of slice) {
        if (!owned(candidate.path)) {
          results.push({...candidate, action: '跳过：删除前归属核验失败'});
          continue;
        }
        const before = bytes(candidate.path) ?? 0;
        const ok = candidate.category === 'build' ? cleanBuild() : candidate.category === 'instances' ? (fs.rmSync(candidate.path, {force: true}), true) : removeResource(candidate.path);
        if (!ok) {
          results.push({...candidate, action: '失败：清理工具返回非零'});
          continue;
        }
        removedBytes += before;
        results.push({...candidate, bytes: before, action: '已清理'});
      }
      const current = filesystems()[0];
      if (current?.usedPercent != null && current.usedPercent <= exitPercent) break;
    }

    const after = filesystems()[0];
    writeState({
      ...readState(),
      level: plan.level,
      lastRun: {
        at: new Date().toISOString(),
        level: plan.level,
        removedBytes,
        batches,
        before: plan.usedPercent,
        after: after?.usedPercent ?? null,
        results: results.map(({id, action, bytes: size}) => ({id, action, bytes: size})),
      },
    });
    return {
      dryRun: false,
      results,
      removedBytes,
      before: plan.usedPercent,
      after: after?.usedPercent ?? null,
      goalMet: after?.usedPercent != null ? after.usedPercent <= exitPercent : null,
      batches,
    };
  }

  /** 构建前准入：临界水位且可用空间低于所需时拒绝重型构建。 */
  function admitBuild(cfg) {
    const fsInfo = filesystems()[0] || null;
    const state = readState();
    if (!fsInfo || fsInfo.usedPercent === null) return {allowed: true, reason: null, fsInfo, need: null, unknown: true};
    const need = Math.max(cfg.storage.buildHeadroomBytes, state.peakGrowthBytes ?? 0);
    if (fsInfo.usedPercent >= cfg.storage.watermarks.critical && fsInfo.availBytes < need) {
      return {
        allowed: false,
        reason: `文件系统已用 ${fsInfo.usedPercent}%，可用 ${(fsInfo.availBytes / GiB).toFixed(1)} GiB 低于所需 ${(need / GiB).toFixed(1)} GiB；先运行 bun run mode:clean`,
        fsInfo,
        need,
        peak: state.peakGrowthBytes ?? null,
      };
    }
    return {allowed: true, reason: null, fsInfo, need, peak: state.peakGrowthBytes ?? null};
  }

  /** 构建成功后记录实测增长，供后续准入估算。 */
  function recordBuildSize(sizeBefore, sizeAfter) {
    if (!Number.isFinite(sizeBefore) || !Number.isFinite(sizeAfter)) return;
    const growth = sizeAfter - sizeBefore;
    if (growth <= 0) return;
    const state = readState();
    writeState({...state, peakGrowthBytes: Math.max(state.peakGrowthBytes ?? 0, growth), peakAt: new Date().toISOString()});
  }

  /** 日志轮转：超上限后滚动，保留 keep 个历史文件，并留下空的当前文件。 */
  function rotateLog(cfg, file = logFile) {
    let stat;
    try {
      stat = fs.statSync(file);
    } catch {
      return null;
    }
    const {maxBytes, keep} = cfg.storage.log;
    if (stat.size < maxBytes) return null;
    // 先丢弃最旧的一份，再逐个向高位挪动，最后把当前文件升为 .1。
    fs.rmSync(`${file}.${keep}`, {force: true});
    for (let i = keep - 1; i >= 1; i -= 1) {
      const from = `${file}.${i}`;
      if (exists(from)) fs.renameSync(from, `${file}.${i + 1}`);
    }
    fs.renameSync(file, `${file}.1`);
    fs.writeFileSync(file, '', {mode: 0o600});
    return {bytes: stat.size, kept: keep};
  }

  /** status 面板摘要。重型统计带缓存，避免每次 status 全盘扫描。 */
  function summary(cfg = config.load(), {cacheMs = 30000} = {}) {
    const state = readState();
    const cached = state.summary;
    if (cached?.at && Date.now() - Date.parse(cached.at) < cacheMs) return cached.value;

    const fsList = filesystems();
    const fsInfo = fsList[0] || null;
    const current = level(fsInfo?.usedPercent ?? null, state.level, cfg.storage.watermarks);
    // 不存在的资源计 0；任一统计失败则整体按未知处理。
    let cacheBytes = 0;
    for (const resource of resources) {
      if (!exists(resource.path) || !owned(resource.path)) continue;
      const size = bytes(resource.path);
      if (size === null) {
        cacheBytes = null;
        break;
      }
      cacheBytes += size;
    }
    const value = {
      usedPercent: fsInfo?.usedPercent ?? null,
      capacityBytes: fsInfo?.capacityBytes ?? null,
      availBytes: fsInfo?.availBytes ?? null,
      cacheBytes,
      budgetBytes: budget(cfg, fsInfo),
      level: current,
      levelLabel: `水位${LEVEL_LABEL[current]}`,
      policyText: POLICY_TEXT[current],
      logMaxBytes: cfg.storage.log.maxBytes,
      logKeep: cfg.storage.log.keep,
      lastRun: state.lastRun ? {at: state.lastRun.at, removedBytes: state.lastRun.removedBytes, before: state.lastRun.before, after: state.lastRun.after} : null,
      suggestClean: current === 'high' || current === 'critical',
    };
    writeState({...state, level: current, summary: {at: new Date().toISOString(), value}});
    return value;
  }

  return {root, dir, logFile, inspect, collect, admitBuild, recordBuildSize, rotateLog, summary, filesystems, touchUsage, readUsage, readState, staleInstances, pruneInstances};
}

/** 仓库实际使用的实例。 */
export const storage = create({root: runtime.root, dir: runtime.dir});

/** status 面板数据：运行状态 + 配置路径 + 存储摘要。 */
export async function statusPayload({config: cfg = config.load()} = {}) {
  const running = await runtime.status();
  let summaryValue;
  try {
    summaryValue = storage.summary(cfg);
  } catch (e) {
    summaryValue = {usedPercent: null, cacheBytes: null, budgetBytes: null, levelLabel: '未获取', policyText: e.message, lastRun: null, suggestClean: false};
  }
  return {...running, configFile: config.configFile, storage: summaryValue};
}
