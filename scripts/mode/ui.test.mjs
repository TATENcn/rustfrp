import {test, expect} from 'bun:test';
import {renderer} from './ui.mjs';
const sample = {running: false, healthy: false, message: '服务未启动', root: '/workspace/中文目录/' + 'long-path/'.repeat(9), repoKey: 'sample', cert: '/tmp/cert.pem', log: '/tmp/service.log'};
function render(method, options = {}, state = sample) { const lines = []; renderer({...options, write: line => lines.push(line)})[method](state); return lines.join('\n'); }
test('human panels survive redirected and NO_COLOR output', () => {
  for (const options of [{tty: false}, {tty: true, env: {NO_COLOR: '1', FORCE_COLOR: '1'}}]) {
    expect(render('status', options)).toContain('○ 已停止');
    expect(render('status', options)).not.toContain('\x1b[');
    expect(render('help', options)).toContain('服务管理');
  }
});
test('semantic TTY colors, forced colors, and explicit JSON', () => {
  expect(render('status', {tty: true, env: {TERM: 'xterm'}})).toContain('\x1b[33m○ 已停止\x1b[0m');
  expect(render('help', {tty: false, env: {FORCE_COLOR: '1'}})).toContain('\x1b[36m');
  expect(render('status', {tty: true, env: {FORCE_COLOR: '0'}})).not.toContain('\x1b[');
  expect(JSON.parse(render('status', {json: true, env: {FORCE_COLOR: '1'}}))).toEqual(sample);
});
test('narrow and long-path panels retain full values and failed health detail', () => {
  const failed = {...sample, running: true, message: '证书主机名不匹配', url: 'https://127.0.0.1:23456'};
  expect(render('status', {columns: 40}, failed)).toContain('⚠ 健康异常');
  expect(render('status', {columns: 40}, failed)).toContain(sample.root);
  expect(render('health', {}, failed)).toContain('证书主机名不匹配');
  expect(render('help', {columns: 40})).toContain('mode:reload\n      重新构建');
});
