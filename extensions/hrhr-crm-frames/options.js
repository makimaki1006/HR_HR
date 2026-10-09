import { loadConfig } from './lib/config.js';
import { parseBlocklist, DEFAULT_BLOCKLIST } from './lib/logic.js';

const $ = (id) => document.getElementById(id);
const cfg = await loadConfig();
$('list').value = cfg.blocklist.join('\n');
if (cfg.managedBlocklist) {
  $('managed').hidden = false;
  $('list').readOnly = true;
  $('save').disabled = true;
  $('reset').disabled = true;
}

function show(text, cls) { $('msg').textContent = text; $('msg').className = cls; }

$('save').addEventListener('click', async () => {
  const { domains, errors } = parseBlocklist($('list').value);
  if (errors.length) {
    show(errors.map((e) => `${e.line} 行目 "${e.text}": ${e.error}`).join('\n'), 'err');
    return; // 1 件でも不正なら保存しない
  }
  try {
    await chrome.storage.sync.set({ blocklist: domains });
    $('list').value = domains.join('\n');
    show(`保存しました (${domains.length} 件)`, 'ok');
  } catch (e) {
    show('保存に失敗しました: ' + e.message, 'err');
  }
});

$('reset').addEventListener('click', () => {
  $('list').value = DEFAULT_BLOCKLIST.join('\n');
  show('既定値を入力しました。保存を押すと反映されます。', 'ok');
});
