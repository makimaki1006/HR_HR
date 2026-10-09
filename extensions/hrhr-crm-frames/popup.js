import { loadConfig } from './lib/config.js';
import { isAppTab } from './lib/logic.js';

const cfg = await loadConfig();
const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
const on = tab && isAppTab(tab.url, cfg.appOrigins);
const st = document.getElementById('status');
st.textContent = on ? '有効 (この CRM タブ)' : '対象外';
st.className = 's ' + (on ? 'on' : 'off');
document.getElementById('detail').textContent = on
  ? 'このタブの中のフレームだけ、ブロックリスト以外のサイトを表示できます。'
  : 'CRM 画面 (/app/crm) 以外では何もしません。';
document.getElementById('count').textContent =
  `ブロックリスト: ${cfg.blocklist.length} 件` + (cfg.managedBlocklist ? ' (管理者設定)' : '');
document.getElementById('opts').addEventListener('click', (e) => {
  e.preventDefault();
  chrome.runtime.openOptionsPage();
});
