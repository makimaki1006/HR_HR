import { resolveConfig } from './logic.js';

async function readArea(area) {
  try { return await chrome.storage[area].get(null); } catch { return {}; }
}

export async function loadConfig() {
  const [managed, sync] = await Promise.all([readArea('managed'), readArea('sync')]);
  return resolveConfig({ managed, sync });
}
