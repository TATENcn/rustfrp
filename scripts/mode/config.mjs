// 仓库内唯一的开发者配置（被 Git 忽略）。
// 只保存人的选择；PID、日志、证书、缓存清单、锁和生成文件都在 .local/mode/ 下。
import * as fs from 'node:fs';
import path from 'node:path';
import {root} from './runtime.mjs';

const GiB = 1024 ** 3;
export const configFile = path.join(root, '.local/mode.json');

const DEFAULTS = {
  version: 1,
  storage: {
    // 触发线必须递增，退出线必须低于对应触发线（滞回，避免抖动）。
    watermarks: {elevated: 75, high: 85, critical: 92, exitElevated: 70, exitHigh: 78, exitCritical: 85},
    // 闲置天数：临时产物 / 构建产物 / 下载缓存。
    idleDays: {tmp: 5, build: 10, download: 21},
    // 0 = 自动：min(20 GiB, max(3 GiB, 容量 2%))。
    budgetBytes: 0,
    // 构建准入：已有产物时的额外预留，与冷启动（无产物）时的预留。
    buildHeadroomBytes: 2 * GiB,
    coldHeadroomBytes: 4 * GiB,
    log: {maxBytes: 10 * 1024 * 1024, keep: 3},
    maintenance: {maxBatches: 8, maxPerBatch: 3, maxMs: 5000},
  },
};

const isObject = v => v !== null && typeof v === 'object' && !Array.isArray(v);
const fail = (file, message) => new Error(`配置文件无效（${file}）：${message}`);

function merge(base, extra) {
  if (!isObject(base) || !isObject(extra)) return extra === undefined ? base : extra;
  const out = {...base};
  for (const [key, value] of Object.entries(extra)) out[key] = key in base ? merge(base[key], value) : value;
  return out;
}

function number(config, file, label, min, max) {
  const value = label.split('.').reduce((node, key) => node?.[key], config);
  if (typeof value !== 'number' || !Number.isFinite(value)) throw fail(file, `${label} 必须是数字`);
  if (value < min || value > max) throw fail(file, `${label} 必须在 ${min}..${max} 之间，当前 ${value}`);
}

const integer = (...args) => {
  number(...args);
  const value = args[2].split('.').reduce((node, key) => node?.[key], args[0]);
  if (!Number.isInteger(value)) throw fail(args[1], `${args[2]} 必须是整数`);
};

/** 校验合并后的配置。未知字段保留（前向兼容），已知字段必须合法。 */
export function validate(config, file = configFile) {
  if (!isObject(config)) throw fail(file, '顶层必须是对象');
  if (config.version !== 1) throw fail(file, `不支持的版本 ${config.version}，期望 1`);
  const wm = config.storage?.watermarks;
  if (!isObject(wm)) throw fail(file, 'storage.watermarks 必须是对象');
  for (const [key, min, max] of [
    ['exitElevated', 1, 99], ['elevated', 2, 99],
    ['exitHigh', 3, 99], ['high', 4, 99],
    ['exitCritical', 5, 99], ['critical', 6, 99.9],
  ]) number(config, file, `storage.watermarks.${key}`, min, max);
  if (!(wm.exitElevated < wm.elevated)) throw fail(file, 'storage.watermarks.exitElevated 必须小于 elevated');
  if (!(wm.elevated < wm.exitHigh)) throw fail(file, 'storage.watermarks.elevated 必须小于 exitHigh');
  if (!(wm.exitHigh < wm.high)) throw fail(file, 'storage.watermarks.exitHigh 必须小于 high');
  if (!(wm.high <= wm.exitCritical)) throw fail(file, 'storage.watermarks.high 必须不大于 exitCritical');
  if (!(wm.exitCritical < wm.critical)) throw fail(file, 'storage.watermarks.exitCritical 必须小于 critical');
  for (const key of ['tmp', 'build', 'download']) integer(config, file, `storage.idleDays.${key}`, 0, 3650);
  number(config, file, 'storage.budgetBytes', 0, 1e15);
  number(config, file, 'storage.buildHeadroomBytes', 0, 1e15);
  number(config, file, 'storage.coldHeadroomBytes', 0, 1e15);
  integer(config, file, 'storage.log.maxBytes', 1024, 1e12);
  integer(config, file, 'storage.log.keep', 1, 20);
  integer(config, file, 'storage.maintenance.maxBatches', 1, 100);
  integer(config, file, 'storage.maintenance.maxPerBatch', 1, 100);
  integer(config, file, 'storage.maintenance.maxMs', 100, 600000);
  return config;
}

/** 读取配置；不存在时返回默认值。损坏或不支持的版本必须报错，不能静默重置。 */
export function load({file = configFile} = {}) {
  let raw;
  try {
    raw = fs.readFileSync(file, 'utf8');
  } catch (e) {
    if (e.code === 'ENOENT') return structuredClone(DEFAULTS);
    throw new Error(`无法读取配置文件 ${file}：${e.message}`);
  }
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    throw new Error(`配置文件不是合法 JSON（${file}）：${e.message}；请修正或删除后重试，管理器不会自动重置`);
  }
  return validate(merge(structuredClone(DEFAULTS), parsed), file);
}

/** 原子写入整个配置（调用方必须已持有仓库锁）。 */
export function save(config, {file = configFile} = {}) {
  validate(config, file);
  atomicWrite(file, `${JSON.stringify(config, null, 2)}\n`);
  return config;
}

/** 局部更新：保留其它字段，包括管理器不认识的字段。 */
export function update(mutate, options = {}) {
  return save(mutate(load(options)), options);
}

export function atomicWrite(file, value) {
  fs.mkdirSync(path.dirname(file), {recursive: true});
  const temp = `${file}.tmp`;
  fs.writeFileSync(temp, value, {mode: 0o600});
  fs.renameSync(temp, file);
}

export function defaults() {
  return structuredClone(DEFAULTS);
}
