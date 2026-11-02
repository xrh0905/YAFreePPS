const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('module/webroot/app.js', 'utf8');
function setup(bridge = true, overrides = {}) {
  const calls = [], elements = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, {textContent: '', disabled: false, value: '', dataset: {}, listeners: {}, children: [],
      addEventListener(name, fn) { this.listeners[name] = fn; },
      append(...children) { this.children.push(...children); }, replaceChildren(...children) { this.children = children; }});
    return elements.get(id);
  };
  let enabled = true;
  const context = { setTimeout, clearTimeout, console, document: {hidden: false,
    getElementById: element, querySelector: () => element('control'),
    createElement: () => ({children: [], append(...children) {this.children.push(...children);}, textContent: '', className: ''}), addEventListener() {} }};
  context.window = context;
  if (bridge) context.ksu = {exec(command, options, callback) {
    calls.push(command); assert.equal(options, '{}');
    if (command.endsWith(' disable')) enabled = false;
    const data = {enabled, running: true, supported: true, adapter_svid: '<img src=x onerror=alert(1)>',
      protocol: 'Public PPS (candidate)', real_type: 'PD_PPS', status: 'Charging', pd_verifed: true,
      apdo_max: 55, capacity: 60, qct: 1, settle_ms: 2500, svid_stable_ms: 1500, ...overrides};
    context[callback](0, command.endsWith(' status') ? JSON.stringify(data) : '', '');
  }};
  vm.createContext(context); vm.runInContext(source, context);
  return {calls, element, context};
}
const flush = () => new Promise(resolve => setImmediate(resolve));
test('bridge status, pause, and refresh use only the fixed CLI', async () => {
  const {calls, element} = setup(); await flush();
  assert.equal(element('mode').textContent, '公版 PPS 解锁已启用');
  assert.equal(element('values').children.length, 8);
  element('toggle').listeners.click(); await flush();
  assert.equal(element('mode').textContent, '已暂停');
  assert.equal(calls[1], '/data/adb/modules/YAFreePPS/bin/yafreepps disable');
  assert(calls.every(cmd => cmd.startsWith('/data/adb/modules/YAFreePPS/bin/yafreepps ')));
});
test('without bridge, mutations are disabled and actionable help is shown', async () => {
  const {element} = setup(false); await flush();
  assert.equal(element('toggle').disabled, true);
  assert.match(element('message').textContent, /模块 Action/);
});
test('settings reject command injection before invoking root', async () => {
  const {calls, element} = setup(); await flush(); const before = calls.length;
  element('settle').value = '2500;id'; element('stable').value = '1500';
  element('settings').listeners.submit({preventDefault() {}}); await flush();
  assert.equal(calls.length, before); assert.match(element('message').textContent, /整数/);
});
test('valid settings use validated integer arguments', async () => {
  const {calls, element} = setup(); await flush();
  element('settle').value = '3000'; element('stable').value = '2000';
  element('settings').listeners.submit({preventDefault() {}}); await flush();
  assert.equal(calls[1], '/data/adb/modules/YAFreePPS/bin/yafreepps configure 3000 2000');
});

const report = {session_id: 4, active: true, classification: 'xiaomi', reason: 'xiaomi_kernel', writes_succeeded: 0, write_attempts: 0, last_write_result: 'not_attempted'};
test('kernel verification is not presented as a module write', async () => {
  const {element} = setup(true, {session: report, session_matches_current: true}); await flush();
  assert.equal(element('session-title').textContent, '本轮会话 #4');
  assert.equal(element('session-values').children[2].children[1].textContent, '否');
});
test('actual success and error receipts render as text', async () => {
  const {element} = setup(true, {session: {...report, writes_succeeded: 1, write_attempts: 2, last_write_result: 'failed', last_write_error: '<script>bad</script>'}, session_matches_current: false}); await flush();
  assert.equal(element('session-values').children[2].children[1].textContent, '是（成功 1 次）');
  assert.match(element('session-note').textContent, /节点已变化/);
  assert.equal(element('session-values').children[6].children[1].textContent, '<script>bad</script>');
});
test('missing report is shown as unknown, not as zero writes', async () => {
  const {element} = setup(true); await flush();
  assert.match(element('session-note').textContent, /等待当前服务上报/);
  assert.equal(element('session-values').children.length, 0);
});
