'use strict';
const $ = id => document.getElementById(id);
const BIN = '/data/adb/modules/YAFreePPS/bin/yafreepps';
let current = null, busy = false, sequence = 0;
function exec(command) {
  return new Promise((resolve, reject) => {
    if (!window.ksu || typeof window.ksu.exec !== 'function') {
      reject(new Error('未发现 KernelSU WebUI 桥接。请从兼容管理器打开；也可使用模块 Action。')); return;
    }
    const callback = `yafreepps_${++sequence}`;
    const timer = setTimeout(() => { delete window[callback]; reject(new Error('模块响应超时，请刷新重试。')); }, 10000);
    window[callback] = (code, stdout, stderr) => {
      clearTimeout(timer); delete window[callback];
      if (code === 0) resolve(stdout); else reject(new Error(stderr || `命令失败：${code}`));
    };
    try { window.ksu.exec(command, '{}', callback); }
    catch (error) { clearTimeout(timer); delete window[callback]; reject(error); }
  });
}
function render(s) {
  current = s;
  $('mode').textContent = s.enabled ? '公版 PPS 解锁已启用' : '已暂停';
  document.querySelector('.control').dataset.enabled = s.enabled;
  $('health').textContent = !s.supported ? '当前设备不支持所需验证节点' : s.running ? '服务运行中 · 小米头由内核管理' : '服务未运行，请检查 daemon.log 或重启';
  $('toggle').textContent = s.enabled ? '暂停' : '启用';
  $('settle').value = s.settle_ms; $('stable').value = s.svid_stable_ms;
  const rows = [
    ['适配器', s.protocol], ['SVID', s.adapter_svid || '未知'],
    ['协议', s.real_type || '未知'], ['电池状态', s.status || '未知'],
    ['验证节点', s.pd_verifed ? '1' : '0'], ['APDO 能力', `${s.apdo_max} W`],
    ['电量', s.capacity === null ? '未知' : `${s.capacity}%`], ['内核快充类型', s.qct === null ? '不可读' : s.qct],
  ];
  $('values').replaceChildren(...rows.map(([key, value]) => {
    const cell = document.createElement('div'), label = document.createElement('div'), text = document.createElement('div');
    label.className = 'key'; text.className = 'val'; label.textContent = key; text.textContent = value;
    cell.append(label, text); return cell;
  }));
}
async function perform(action) {
  if (busy) return;
  busy = true; $('message').textContent = '';
  ['toggle', 'refresh', 'save'].forEach(id => $(id).disabled = true);
  try { if (action) await exec(`${BIN} ${action}`); render(JSON.parse(await exec(`${BIN} status`))); }
  catch (error) { $('message').textContent = error.message; if (!current) $('health').textContent = '未连接模块'; }
  finally {
    busy = false; $('refresh').disabled = false;
    $('toggle').disabled = !current; $('save').disabled = !current;
  }
}
$('toggle').addEventListener('click', () => perform(current.enabled ? 'disable' : 'enable'));
$('refresh').addEventListener('click', () => perform());
$('settings').addEventListener('submit', event => {
  event.preventDefault();
  const settle = Number($('settle').value), stable = Number($('stable').value);
  if (!Number.isInteger(settle) || !Number.isInteger(stable) || settle < 2000 || settle > 30000 || stable < 1000 || stable > 30000) {
    $('message').textContent = '请输入范围内的整数。'; return;
  }
  perform(`configure ${settle} ${stable}`);
});
// Refresh on user return only. No background polling or sysfs write from JavaScript.
document.addEventListener('visibilitychange', () => { if (!document.hidden) perform(); });
perform();
