export const commands = {
  start: ['服务管理', '构建并后台启动本机 HTTPS 预览'],
  stop: ['服务管理', '停止本仓库实例，保留数据'],
  reload: ['服务管理', '重新构建并重启，应用最新代码'],
  restart: ['服务管理', '重启已有产物，不构建'],
  build: ['项目任务', '安装锁定依赖，构建 WebUI 和 daemon'],
  test: ['项目任务', '运行 cargo test --lib，透传参数'],
  status: ['状态与排障', '查看状态面板；支持 --json'],
  health: ['状态与排障', '检查 HTTPS 和后端；支持 --json'],
  logs: ['状态与排障', '显示最后 80 行；--follow 持续跟踪'],
  help: ['状态与排障', '显示分组帮助；支持 --json'],
};
export function renderer({json = false, env = process.env, tty = process.stdout.isTTY, columns = process.stdout.columns || 80, write = console.log} = {}) {
  const color = !json && !('NO_COLOR' in env) && env.FORCE_COLOR !== '0' && (env.FORCE_COLOR ? true : tty && env.TERM !== 'dumb');
  const paint = (text, code) => color ? `\x1b[${code}m${text}\x1b[0m` : text;
  const width = text => [...text].reduce((n, c) => n + (c.codePointAt(0) > 255 ? 2 : 1), 0);
  const rule = () => write(paint('─'.repeat(Math.max(24, Math.min(76, columns))), 2));
  const row = (label, value, size = 10) => {
    const prefix = `  ${label}${' '.repeat(Math.max(2, size - width(label)))}`;
    if (width(prefix + value) > columns) { write(paint(`  ${label}`, 2)); write(`    ${value}`); }
    else write(paint(prefix, 2) + value);
  };
  const title = text => { rule(); write(paint(`  RustFRP · ${text}`, 1)); rule(); };
  const status = s => {
    if (json) return write(JSON.stringify(s));
    title('开发环境');
    row('运行状态', paint(s.healthy ? '● 运行正常' : s.unverified ? '⚠ 归属未验证' : s.running ? '⚠ 健康异常' : '○ 已停止', s.healthy ? 32 : 33));
    row('当前模式', 'local · 本机构建预览');
    row('访问地址', s.running ? paint(s.url || '未获取', 36) : s.unverified ? '未获取' : '未启动');
    if (s.running) for (const url of s.lanUrls || []) row('局域网', paint(url, 36));
    row('监听范围', 'HTTPS 0.0.0.0 · 后端仅回环');
    write('');
    row('健康检查', s.message);
    row('TLS 证书', '自签证书 · 浏览器信任未验证');
    write('');
    row('项目目录', s.root); row('仓库标识', s.repoKey);
    row('证书文件', s.cert); row('日志文件', s.log);
    rule();
    write(paint(s.running ? '  mode:logs 查看日志 · mode:reload 更新 · mode:stop 停止' : '  bun run mode:start 启动 · bun run mode:help 查看帮助', 36));
  };
  const help = () => {
    if (json) return write(JSON.stringify({mode: 'local', commands}));
    title('开发命令'); row('用法', 'bun run mode:<命令> [参数]'); row('当前模式', 'local · 单一本机预览，无需选择');
    for (const group of ['服务管理', '状态与排障', '项目任务']) {
      write(''); write(paint(`  ${group}`, 1));
      for (const [name, [category, description]] of Object.entries(commands)) if (group === category) {
        const command = `mode:${name}`;
        if (columns < 60) { write(paint(`    ${command}`, 36)); write(`      ${description}`); }
        else write(`    ${paint(command.padEnd(20), 36)}${description}`);
      }
    }
    write(''); write(paint('  常用操作', 1));
    for (const example of ['bun run mode:start', 'bun run mode:reload', 'bun run --silent mode:status --json', 'bun run mode:test -- --nocapture']) write(paint(`    ${example}`, 36));
    write(''); write('  说明'); write('    Linux + Bun + Cargo + OpenSSL + flock。首次构建需要网络。');
    write('    reload 重新构建后重启；restart 使用上次产物。无 HMR。');
    write('    默认不下载 frpc；在界面中安装版本后再启动隧道。');
    write('    API Token 位于 .local/mode/api-token，首次访问时填入界面。');
    rule();
  };
  return {status, help, health(s) { if (json) write(JSON.stringify(s)); else { title('健康检查'); row('检查结果', paint(`${s.healthy ? '✓' : '⚠'} ${s.message}`, s.healthy ? 32 : 31)); rule(); } }, step(text) { write(paint(`  → ${text}`, 36)); }};
}
