// 控制台渲染：status 状态面板、help 分组帮助、health 摘要、步骤输出。
// 颜色只影响 ANSI；排版、对齐、分组在无色与重定向下完全保留。
// 机器输出只由显式 --json 触发。

import {configFile} from './config.mjs';

export const commands = {
  start: ['服务管理', '构建并后台启动本机 HTTPS 预览'],
  stop: ['服务管理', '停止本仓库实例，保留数据'],
  reload: ['服务管理', '重新构建并重启，应用最新代码'],
  restart: ['服务管理', '重启已有产物，不构建'],
  build: ['服务管理', '安装锁定依赖，构建 WebUI 和 daemon'],
  status: ['状态与排障', '查看状态、访问地址与磁盘水位'],
  health: ['状态与排障', '检查 HTTPS 与后端身份，异常返回非零'],
  logs: ['状态与排障', '显示最后 80 行；--follow 持续跟踪'],
  clean: ['维护', '预览可回收缓存；--execute 实际清理'],
  help: ['状态与排障', '显示分组帮助'],
  test: ['项目任务', '运行 cargo test --lib，透传参数'],
};

const GROUPS = ['服务管理', '状态与排障', '维护', '项目任务'];

const gib = bytes => {
  if (bytes == null) return '未获取';
  const GiB = 1024 ** 3;
  const MiB = 1024 ** 2;
  if (bytes >= GiB) return `${(bytes / GiB).toFixed(1)} GiB`;
  if (bytes >= MiB) return `${(bytes / MiB).toFixed(0)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KiB`;
  return `${bytes} B`;
};

/** 终端显示宽度：ANSI 序列计 0，宽字符（中文）计 2。 */
export function displayWidth(text) {
  return [...String(text).replace(/\x1b\[[0-9;]*m/g, '')].reduce((n, ch) => {
    const code = ch.codePointAt(0);
    return n + (code > 0x2e7f || (code >= 0x1100 && code <= 0x115f) ? 2 : 1);
  }, 0);
}

/** 按空格折行；单个超长词（路径、URL）保留完整，不强行截断。 */
export function wrapText(text, limit) {
  if (displayWidth(text) <= limit) return [text];
  const lines = [];
  let line = '';
  for (const word of String(text).split(' ')) {
    if (!line) {
      line = word;
    } else if (displayWidth(`${line} ${word}`) <= limit) {
      line = `${line} ${word}`;
    } else {
      lines.push(line);
      line = word;
    }
  }
  if (line) lines.push(line);
  return lines;
}

export function createRenderUtils({columns = 80, write = console.log, paint = text => text} = {}) {
  const indent = '  ';
  const labelWidth = 10;
  const width = Math.max(24, Math.min(76, columns));
  const rule = () => write(paint('─'.repeat(width), 2));

  /** 标签/值两列；超宽时值另起一行，保留完整可复制内容。 */
  const row = (label, value, {size = labelWidth} = {}) => {
    const prefix = `${indent}${label}${' '.repeat(Math.max(2, size - displayWidth(label)))}`;
    if (displayWidth(`${prefix}${value}`) <= width) {
      write(`${paint(prefix, 2)}${value}`);
      return;
    }
    write(paint(`${indent}${label}`, 2));
    for (const line of wrapText(value, width - 4)) write(`${indent}${indent}${line}`);
  };
  const section = title => {
    write('');
    write(`${paint('▍', 36)}${paint(` ${title}`, 1)}`);
  };
  const title = (text, status) => {
    rule();
    if (!status) return write(paint(`${indent}${text}`, 1));
    const left = `${indent}${text}`;
    const gap = width - displayWidth(left) - displayWidth(status.value);
    if (gap >= 2) {
      write(`${paint(left, 1)}${' '.repeat(gap)}${paint(status.value, status.code)}`);
    } else {
      write(paint(left, 1));
      write(`${indent}${paint(status.value, status.code)}`);
    }
    rule();
  };
  return {rule, row, section, title, width, labelWidth, write, paint};
}

export function renderer({
  json = false,
  env = process.env,
  tty = process.stdout.isTTY,
  columns = process.stdout.columns || 80,
  write = console.log,
} = {}) {
  const color = !json && !('NO_COLOR' in env) && env.FORCE_COLOR !== '0' && (env.FORCE_COLOR ? true : tty && env.TERM !== 'dumb');
  const paint = (text, code) => (color ? `\x1b[${code}m${text}\x1b[0m` : text);
  const {rule, row, section, title, width} = createRenderUtils({columns, write, paint});

  const green = text => paint(text, 32);
  const yellow = text => paint(text, 33);
  const red = text => paint(text, 31);
  const cyan = text => paint(text, 36);

  /** 操作指引：按 · 分组折行，绝不把单个命令拆断。 */
  const guide = segments => {
    let line = '';
    for (const segment of segments.split(' · ')) {
      if (!line) {
        line = segment;
      } else if (displayWidth(`  ${line} · ${segment}`) <= width) {
        line = `${line} · ${segment}`;
      } else {
        for (const part of wrapText(line, width - 2)) write(cyan(`  ${part}`));
        line = segment;
      }
    }
    if (line) for (const part of wrapText(line, width - 2)) write(cyan(`  ${part}`));
  };

  const stateBadge = s => {
    if (s.unverified) return {value: '⚠ 归属未验证', code: 33};
    if (s.healthy) return {value: '● 运行正常', code: 32};
    if (s.running) return {value: '⚠ 健康异常', code: 31};
    return {value: '○ 已停止', code: 33};
  };

  const storageRows = s => {
    const storage = s.storage;
    if (!storage) {
      row('磁盘水位', '未获取');
      return;
    }
    if (storage.usedPercent == null) {
      row('磁盘水位', yellow('未获取（文件系统查询失败）'));
    } else {
      const tone = storage.level === 'critical' ? red : storage.level === 'high' || storage.level === 'elevated' ? yellow : green;
      row('磁盘水位', `${tone(`${storage.usedPercent}% 已用`)} · 可用 ${gib(storage.availBytes)} / ${gib(storage.capacityBytes)}`);
    }
    const parts = [];
    if (storage.cacheBytes != null) {
      const over = storage.budgetBytes != null && storage.cacheBytes > storage.budgetBytes;
      const text = `缓存 ${gib(storage.cacheBytes)}`;
      parts.push(over ? yellow(text) : text);
    }
    if (storage.budgetBytes != null) parts.push(`预算 ${gib(storage.budgetBytes)}`);
    if (parts.length) row('缓存占用', parts.join(' · '));
    const levelTone = storage.level === 'critical' ? red : storage.level === 'high' || storage.level === 'elevated' ? yellow : green;
    row('回收策略', `${levelTone(storage.levelLabel)} · ${storage.policyText}`);
    if (storage.lastRun) {
      const when = new Date(storage.lastRun.at).toISOString().replace('T', ' ').slice(0, 16);
      const before = storage.lastRun.before == null ? '未获取' : `${storage.lastRun.before}%`;
      const after = storage.lastRun.after == null ? '未获取' : `${storage.lastRun.after}%`;
      row('上次清理', `${when} · ${cyan(`回收 ${gib(storage.lastRun.removedBytes)}`)} · ${before} → ${after}`);
    } else {
      row('上次清理', '未执行');
    }
  };

  const status = s => {
    if (json) return write(JSON.stringify(s));
    title('RustFRP · 开发环境', stateBadge(s));
    row('当前模式', `${cyan('local')} · 本机构建预览`);
    row('访问地址', s.running && s.url ? cyan(s.url) : s.unverified ? '未获取' : '未启动');
    if (s.running) for (const url of s.lanUrls || []) row('局域网', cyan(url));
    row('监听范围', 'HTTPS 0.0.0.0 · 后端仅回环');

    section('运行检查');
    row('健康检查', s.healthy ? green(`✓ ${s.message}`) : s.running ? red(`⚠ ${s.message}`) : yellow(`○ ${s.message}`));
    row('TLS 证书', s.healthy || s.running ? yellow('⚠ 自签证书 · 浏览器信任未验证') : '自签证书 · 浏览器信任未验证');

    section('磁盘与缓存');
    storageRows(s);

    section('诊断');
    row('项目目录', s.root);
    row('仓库标识', s.repoKey ? cyan(s.repoKey) : '未获取');
    row('配置文件', s.configFile || configFile);
    row('证书文件', s.cert);
    row('日志文件', s.log);
    rule();
    guide(s.running ? 'mode:logs 查看日志 · mode:reload 更新 · mode:status --json 机器输出' : 'bun run mode:start 启动 · bun run mode:help 查看帮助');
    if (s.storage?.suggestClean) {
      guide('bun run mode:clean 预览可回收缓存 · bun run mode:clean -- --execute 实际清理');
    }
  };

  const help = () => {
    if (json) return write(JSON.stringify({mode: 'local', commands}));
    title('RustFRP · 开发命令');
    row('用法', `${cyan('bun run mode:<命令>')} [参数]`);
    row('当前模式', 'local · 单一本机构建预览，无需选择');
    row('配置文件', configFile);
    for (const group of GROUPS) {
      const entries = Object.entries(commands).filter(([, [category]]) => category === group);
      if (!entries.length) continue;
      section(group);
      for (const [name, [, description]] of entries) {
        const command = `mode:${name}`;
        if (width < 60) {
          write(`${cyan(`    ${command}`)}`);
          write(`      ${description}`);
        } else {
          const padded = `${command}${' '.repeat(Math.max(2, 20 - displayWidth(command)))}`;
          write(`    ${cyan(padded)}${description}`);
        }
      }
    }
    section('常用操作');
    for (const example of ['bun run mode:start', 'bun run mode:reload', 'bun run --silent mode:status --json', 'bun run mode:clean', 'bun run mode:test -- --nocapture']) {
      write(cyan(`    ${example}`));
    }
    section('说明');
    write('    Linux + Bun + Cargo + OpenSSL + flock。首次构建需要网络。');
    write('    reload 重新构建后重启；restart 使用上次产物；两者都不提供 HMR。');
    write('    clean 默认只预览；--execute 才删除，构建产物需再显式指定 --build。');
    write('    默认不下载 frpc；先在界面安装版本，再启动隧道。');
    write('    API Token 位于 .local/mode/api-token，首次访问时填入界面。');
    rule();
  };

  const health = s => {
    if (json) return write(JSON.stringify(s));
    title('RustFRP · 健康检查', stateBadge(s));
    row('检查结果', s.healthy ? green(`✓ ${s.message}`) : red(`⚠ ${s.message}`));
    if (s.running && s.url) row('访问地址', cyan(s.url));
    rule();
    guide(s.healthy ? 'bun run mode:status 查看完整状态' : 'bun run mode:logs 查看日志 · bun run mode:restart 重试');
  };

  const clean = (plan, result) => {
    if (json) return write(JSON.stringify(result ? {plan, result} : plan));
    title('RustFRP · 缓存维护', {value: result ? (result.dryRun ? '预览' : '已执行') : '预览', code: 36});
    const tone = plan.level === 'critical' ? red : plan.level === 'high' || plan.level === 'elevated' ? yellow : green;
    row('磁盘水位', plan.usedPercent == null ? yellow('未获取') : `${tone(`${plan.usedPercent}% 已用`)} · 可用 ${gib(plan.availBytes)} / ${gib(plan.capacityBytes)}`);
    row('水位级别', tone(plan.level));
    row('分级策略', `回收类别 ${cyan(plan.policy.categories.join(' / '))} · 闲置期限 ${Object.entries(plan.policy.idleDays).map(([key, days]) => `${key} ${days} 天`).join(' · ')}`);
    row('缓存预算', plan.budgetBytes == null ? '未获取' : gib(plan.budgetBytes));
    row('日志上限', `${gib(plan.logMaxBytes)} × ${plan.logKeep} 个文件`);

    section('可回收候选');
    if (!plan.candidates.length) write('    无候选（未到期限或不在当前级别范围）');
    for (const candidate of plan.candidates) {
      const flag = candidate.expensive ? yellow('昂贵') : green('便宜');
      write(`    ${cyan(candidate.id.padEnd(14))}${gib(candidate.bytes).padStart(10)}  ${flag}  闲置 ${candidate.idleDays} 天 · ${candidate.source}`);
    }

    section('跳过与保护');
    if (!plan.skipped.length) write('    无');
    for (const item of plan.skipped) write(`    ${item.id.padEnd(14)}${item.reason}`);

    if (result && !result.dryRun) {
      section('执行结果');
      for (const item of result.results) {
        const ok = item.action.startsWith('已清理');
        write(`    ${cyan(item.id.padEnd(14))}${ok ? green(item.action) : red(item.action)}`);
      }
      row('实际回收', green(gib(result.removedBytes)));
      row('执行前后', `${result.before == null ? '未获取' : `${result.before}%`} → ${result.after == null ? '未获取' : `${result.after}%`}`);
      row('批次', `${result.batches} 批`);
      if (result.goalMet === false) write(yellow('    未达到退出水位；可提高维护上限或手动处理其他占用'));
    }
    section('下一步');
    guide(!result || result.dryRun ? 'bun run mode:clean -- --execute 执行上面的候选回收' : 'bun run mode:status 查看更新后的水位');
    rule();
  };

  const step = text => write(cyan(`  → ${text}`));
  const warn = text => write(yellow(`  ⚠ ${text}`));
  const fail = text => write(red(`  ✗ ${text}`));

  return {status, help, health, clean, step, warn, fail};
}
