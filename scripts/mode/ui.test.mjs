import {test, expect} from 'bun:test';
import {renderer, commands, displayWidth} from './ui.mjs';

const base = {
  running: false,
  healthy: false,
  message: '服务未启动',
  root: '/workspace/中文目录/' + 'long-path/'.repeat(9),
  repoKey: 'sample',
  cert: '/tmp/cert.pem',
  log: '/tmp/service.log',
  configFile: '/workspace/repo/.local/mode.json',
  storage: {
    usedPercent: 78.4,
    capacityBytes: 1e12,
    availBytes: 216e9,
    cacheBytes: 12 * 1024 ** 3,
    budgetBytes: 20 * 1024 ** 3,
    level: 'elevated',
    levelLabel: '水位偏高',
    policyText: '回收过期临时与下载缓存',
    logMaxBytes: 10 * 1024 ** 2,
    logKeep: 3,
    lastRun: {at: '2026-11-01T02:03:00.000Z', removedBytes: 3 * 1024 ** 3, before: 91, after: 84},
    suggestClean: false,
  },
};

const stopped = {...base};
const running = {...base, running: true, healthy: true, message: 'HTTPS 与 daemon 已就绪', url: 'https://127.0.0.1:23428', lanUrls: ['https://192.168.1.9:23428']};
const broken = {...running, healthy: false, message: '证书主机名不匹配'};
const unverified = {...base, running: true, unverified: true, message: '无法核验进程归属（可能权限不足）；未探测旧端口'};

function render(method, options = {}, state = stopped) {
  const lines = [];
  renderer({...options, write: line => lines.push(line)})[method](state);
  return lines.join('\n');
}

const plan = {
  level: 'high',
  usedPercent: 86,
  capacityBytes: 1e12,
  availBytes: 140e9,
  budgetBytes: 20 * 1024 ** 3,
  policy: {categories: ['tmp', 'download'], idleDays: {tmp: 5, download: 21}, allowExpensive: false},
  logMaxBytes: 10 * 1024 ** 2,
  logKeep: 3,
  candidates: [
    {id: 'tmp', bytes: 2 * 1024 ** 3, expensive: false, idleDays: 9, source: '管理器记录'},
    {id: 'webui-modules', bytes: 300 * 1024 ** 2, expensive: false, idleDays: 30, source: '首次观察（宽限）'},
  ],
  skipped: [{id: 'build', reason: '类别 build 不在高水位级别范围'}],
};

test('human panels survive redirected and NO_COLOR output', () => {
  for (const options of [{tty: false}, {tty: true, env: {NO_COLOR: '1', FORCE_COLOR: '1'}}]) {
    expect(render('status', options)).toContain('○ 已停止');
    expect(render('status', options)).not.toContain('\x1b[');
    expect(render('help', options)).toContain('服务管理');
    // 重定向仍然保留分组、分隔线与对齐，不压成对象或一行。
    expect(render('status', options).split('\n').length).toBeGreaterThan(15);
    expect(render('status', options)).toContain('磁盘与缓存');
    expect(render('status', options)).toContain('诊断');
  }
});

test('semantic TTY colors, forced colors, and explicit JSON', () => {
  expect(render('status', {tty: true, env: {TERM: 'xterm'}})).toContain('\x1b[33m○ 已停止\x1b[0m');
  expect(render('status', {tty: true, env: {TERM: 'xterm'}}, running)).toContain('\x1b[32m● 运行正常\x1b[0m');
  expect(render('help', {tty: false, env: {FORCE_COLOR: '1'}})).toContain('\x1b[36m');
  expect(render('status', {tty: true, env: {FORCE_COLOR: '0'}})).not.toContain('\x1b[');
  expect(JSON.parse(render('status', {json: true, env: {FORCE_COLOR: '1'}}))).toEqual(stopped);
  expect(JSON.parse(render('help', {json: true}))).toMatchObject({mode: 'local'});
});

test('status distinguishes stopped, healthy, failed, and unverified states in words', () => {
  expect(render('status', {}, stopped)).toContain('○ 已停止');
  expect(render('status', {}, running)).toContain('● 运行正常');
  expect(render('status', {}, broken)).toContain('⚠ 健康异常');
  expect(render('status', {}, broken)).toContain('证书主机名不匹配');
  expect(render('status', {}, unverified)).toContain('⚠ 归属未验证');
  // 停止时不把旧地址当作可访问地址。
  expect(render('status', {}, stopped)).not.toContain('https://');
});

test('narrow and long-path panels retain full values and failed health detail', () => {
  expect(render('status', {columns: 40}, broken)).toContain('⚠ 健康异常');
  expect(render('status', {columns: 40}, broken)).toContain(stopped.root);
  expect(render('health', {}, broken)).toContain('证书主机名不匹配');
  // 极窄终端下命令与说明分两行，不连成一坨。
  const narrow = render('help', {columns: 40});
  expect(narrow).toContain('mode:reload');
  expect(narrow).toContain('重新构建并重启');
});

test('status shows storage water line, cache budget and last maintenance without inventing numbers', () => {
  const text = render('status', {}, running);
  expect(text).toContain('78.4% 已用');
  expect(text).toContain('水位偏高');
  expect(text).toContain('2026-11-01 02:03');
  expect(text).toContain('91% → 84%');
  const unknown = render('status', {}, {...running, storage: {usedPercent: null, levelLabel: '未获取', policyText: '查询失败', lastRun: null}});
  expect(unknown).toContain('未获取（文件系统查询失败）');
  expect(unknown).toContain('上次清理  未执行');
});

test('clean panel lists candidates, protected items and preview guidance', () => {
  const preview = render('clean', {}, plan);
  expect(preview).toContain('可回收候选');
  expect(preview).toContain('2.0 GiB');
  expect(preview).toContain('跳过与保护');
  expect(preview).toContain('--execute');
  const lines = [];
  const result = {dryRun: false, results: [{id: 'tmp', action: '已清理'}], removedBytes: 2 * 1024 ** 3, before: 86, after: 74, batches: 1, goalMet: true};
  renderer({write: line => lines.push(line)}).clean(plan, result);
  const executed = lines.join('\n');
  expect(executed).toContain('已清理');
  expect(executed).toContain('86% → 74%');
});

test('health renders a short summary and JSON stays parseable', () => {
  expect(render('health', {}, running)).toContain('✓ HTTPS 与 daemon 已就绪');
  expect(JSON.parse(render('health', {json: true}, broken)).healthy).toBe(false);
});

test('every advertised command has a description and a group', () => {
  const groups = new Set(['服务管理', '状态与排障', '维护', '项目任务']);
  for (const [name, [group, description]] of Object.entries(commands)) {
    expect(groups.has(group)).toBe(true);
    expect(description.length).toBeGreaterThan(3);
    expect(name).not.toContain('mode:');
  }
  for (const name of ['start', 'stop', 'reload', 'restart', 'build', 'status', 'health', 'logs', 'clean', 'help', 'test']) {
    expect(commands[name]).toBeDefined();
  }
});

test('display width counts ANSI as zero and CJK as two columns', () => {
  expect(displayWidth('abc')).toBe(3);
  expect(displayWidth('\x1b[36mmode:clean\x1b[0m')).toBe(10);
  expect(displayWidth('运行状态')).toBe(8);
  expect(displayWidth('mode:start')).toBe(10);
});

test('wrapped rows never exceed the terminal width, and long paths stay complete', async () => {
  const {wrapText} = await import('./ui.mjs');
  const longPath = '/home/someone/Projects/' + 'very-long-segment/'.repeat(6);
  for (const columns of [40, 55, 72, 80]) {
    const lines = [];
    renderer({columns, write: line => lines.push(line)}).status({
      ...running,
      root: longPath,
      storage: {...base.storage, usedPercent: 99.9, availBytes: 1, capacityBytes: 1e12},
    });
    // 无空格的超长路径无法折行，允许独占一行超出宽度（契约要求保留完整可复制值）。
    for (const line of lines) {
      const plain = line.replace(/\x1b\[[0-9;]*m/g, '');
      const hasSpace = plain.trim().includes(' ');
      if (hasSpace) expect(displayWidth(line)).toBeLessThanOrEqual(columns);
    }
    // 完整路径仍可复制：内容没有被截断成省略号。
    expect(lines.join('\n').replace(/\s+/g, '')).toContain(longPath.replace(/\s+/g, ''));
  }
  expect(wrapText('short', 76)).toEqual(['short']);
  expect(wrapText('a b c', 3)).toEqual(['a b', 'c']);
  expect(wrapText('工程 项目 目录', 9)).toEqual(['工程 项目', '目录']);
});
