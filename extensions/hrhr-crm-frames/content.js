// アプリ側との契約はこれだけ: 拡張が入っていることと、そのバージョンを DOM に出す。
document.documentElement.dataset.hrhrFrames = chrome.runtime.getManifest().version;
