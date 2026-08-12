// js-verify が報告した「null で toLocaleString が落ちる」欠陥の修正確認
const fs = require('fs');
const src = fs.readFileSync('static/js/laborflow.js', 'utf8');
// IIFE 内の関数を取り出すため、末尾に export を足したコピーを評価する
const patched = src.replace(/\}\)\(\);\s*$/, `
  module.exports = { num: num, isSuppressed: isSuppressed, formatHeadcountRate: formatHeadcountRate };
})();`);
const Module = require('module');
const m = new Module('t');
global.window = {}; global.document = { createElement: () => ({ appendChild(){}, innerHTML: '' }), createTextNode: (s)=>s, addEventListener(){} };
global.window.addEventListener = () => {};
m._compile(patched, 'laborflow.js');
const { num, isSuppressed, formatHeadcountRate } = m.exports;

let fail = 0;
const t = (label, got, want) => {
  const ok = Object.is(got, want);
  if (!ok) fail++;
  console.log(`${ok ? 'OK ' : 'NG '} ${label}: got=${JSON.stringify(got)} want=${JSON.stringify(want)}`);
};
// num のガード
t('num(null)', num(null), 0);
t('num(undefined)', num(undefined), 0);
t('num(NaN)', num(NaN), 0);
t('num(Infinity)', num(Infinity), 0);
t('num("abc")', num('abc'), 0);
t('num(-3271)', num(-3271), -3271);
t('num(0)', num(0), 0);
// null でも toLocaleString が落ちないこと
try { num(null).toLocaleString(); console.log('OK  num(null).toLocaleString() が例外にならない'); }
catch (e) { fail++; console.log('NG  例外: ' + e.message); }
// isSuppressed: NaN も抑制扱いにする (旧実装は false で "NaN%" を出していた)
t('isSuppressed({rate:0})', isSuppressed({headcount_rate_1y:0}), false);
t('isSuppressed({rate:NaN})', isSuppressed({headcount_rate_1y:NaN}), true);
t('isSuppressed({rate:null})', isSuppressed({headcount_rate_1y:null}), true);
t('isSuppressed({rate:2.6})', isSuppressed({headcount_rate_1y:2.6}), false);
// NaN が "NaN%" にならないこと
t('formatHeadcountRate({rate:NaN})', formatHeadcountRate({headcount_rate_1y:NaN}), '—');
t('formatHeadcountRate({rate:-0.04})', formatHeadcountRate({headcount_rate_1y:-0.04}), '-0.0%');
t('formatHeadcountRate({rate:0})', formatHeadcountRate({headcount_rate_1y:0}), '+0.0%');
console.log(`\n不合格: ${fail} 件`);
process.exit(fail ? 1 : 0);
