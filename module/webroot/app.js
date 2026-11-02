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
  $('values').replaceChildren(...rows.map(cell));
  renderSession(s);
}
function cell([key, value]) {
    const cell = document.createElement('div'), label = document.createElement('div'), text = document.createElement('div');
    label.className = 'key'; text.className = 'val'; label.textContent = key; text.textContent = value;
    cell.append(label, text); return cell;
}
const classifications = {disconnected:'未连接', xiaomi:'小米 MIPPS（内核管理）', public_candidate:'公版 PPS 候选', public_pps:'公版 PPS', unknown:'尚未识别'};
const reasons = {starting:'服务正在初始化', disconnected:'未连接，无写入', paused:'已暂停', unknown_status:'电池状态未知，跳过', xiaomi_kernel:'本轮见过 2717，保持内核原生协商', waiting_svid:'等待 SVID 出现', unsupported_svid:'SVID 不受支持，跳过', not_pps:'当前不是 PPS', settling:'等待握手保护窗口结束', capacity_unreadable:'电量不可读，跳过', full_or_trickle:'满电或涓流，跳过', verified:'验证位已为 1，无需重复写入', write_completed:'本轮已执行写入', eligible:'符合条件，等待处理'};
const outcomes = {not_attempted:'未尝试', success:'成功', failed:'失败', cancelled_by_recheck:'写前复查条件变化，已取消'};
function renderSession(s) {
  const r = s.session;
  $('session-title').textContent = r ? `${r.active ? '本轮' : '最近'}会话 #${r.session_id}` : '本轮会话';
  if (!s.running || !r) {
    $('session-note').textContent = !s.running ? '服务未运行；没有可确认的本轮写入记录。' : '等待当前服务上报，旧进程记录不会用于判断。';
    $('session-values').replaceChildren(); return;
  }
  $('session-note').textContent = s.session_matches_current ? '以下为当前服务的事件记录。' : '节点已变化，以下为服务最近一次事件记录；请稍后刷新。';
  const rows = [
    ['本轮识别', classifications[r.classification] || '未知'],
    ['当前决策', reasons[r.reason] || r.reason],
    ['本轮写入 1', r.writes_succeeded > 0 ? `是（成功 ${r.writes_succeeded} 次）` : '否'],
    ['实际写入尝试', `${r.write_attempts} 次`],
    ['最近执行结果', outcomes[r.last_write_result] || r.last_write_result],
    ['最近写入时间', r.last_write_at_ms ? new Date(r.last_write_at_ms).toLocaleString() : '无'],
  ];
  if (r.reason === 'settling' && r.settle_deadline_ms) rows.push(['保护窗口', `${Math.max(0, r.settle_deadline_ms - Date.now())} ms 后可重新判定`]);
  if (r.last_write_error) rows.push(['写入错误', r.last_write_error]);
  $('session-values').replaceChildren(...rows.map(cell));
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
