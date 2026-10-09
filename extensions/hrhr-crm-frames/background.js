import { loadConfig } from './lib/config.js';
import { buildRules, isAppTab, RULE_ID } from './lib/logic.js';

// 更新は直列化する。毎回「全タブを見て作り直す」ので、イベントの取りこぼしや順序に強い。
let chain = Promise.resolve();
function refresh() {
  chain = chain.then(doRefresh).catch((e) => console.error('[hrhr-crm-frames] refresh failed', e));
  return chain;
}

async function doRefresh() {
  const cfg = await loadConfig();
  const tabs = await chrome.tabs.query({});
  const tabIds = tabs.filter((t) => isAppTab(t.url, cfg.appOrigins)).map((t) => t.id);
  await chrome.declarativeNetRequest.updateSessionRules({
    removeRuleIds: [RULE_ID],
    addRules: buildRules(tabIds, cfg.blocklist, cfg.appOrigins),
  });
}

chrome.tabs.onUpdated.addListener((_id, info) => { if (info.url || info.status) refresh(); });
chrome.tabs.onRemoved.addListener(refresh);
chrome.tabs.onReplaced.addListener(refresh);
chrome.tabs.onCreated.addListener(refresh);
chrome.storage.onChanged.addListener(refresh);
chrome.runtime.onInstalled.addListener(refresh);
chrome.runtime.onStartup.addListener(refresh);
refresh(); // Service Worker 起動のたびに再構築
