/**
 * 段 2 の照合が機能していることを、ブラウザ抜きで確かめる（逆証明）。
 *
 * 「未定義 0 件」は「検査が働いた」ではない。照合が壊れて何も拾えなくなっても
 * 同じ出力になる。実在しないクラスを「無い」と、実在するクラスを「ある」と
 * 言えることを、毎回の CI で確かめる。
 *
 *   node scripts/audit_css_classes.selftest.js
 */
const fs = require('fs');
const path = require('path');
const { tokenPattern } = require('./audit_css_classes.js');

let ng = 0;
const check = (label, ok) => {
  console.log((ok ? 'OK   ' : 'NG   ') + label);
  if (!ok) ng++;
};

// ---- 作った CSS 断片で、エスケープの扱いを確かめる -------------------------
// Tailwind の出力そのままの形。ここが読めないと `/` `:` `.` を含む
// クラスを軒並み「無い」と誤検出する（前回それで 10 件ほど出した）
check('不透明度つき（エスケープあり）', tokenPattern('bg-blue-500/20').test('.bg-blue-500\\/20{}'));
check('不透明度つき（エスケープなし）', tokenPattern('bg-blue-500/20').test('.bg-blue-500/20{}'));
check('擬似クラス付きセレクタ', tokenPattern('hover:text-white').test('.hover\\:text-white:hover{}'));
check('小数を含むクラス', tokenPattern('p-1.5').test('.p-1\\.5{}'));
check('@media の中', tokenPattern('sm:gap-4').test('@media(min-width:640px){.sm\\:gap-4{gap:1rem}}'));
check('前方一致で誤って当たらない', tokenPattern('bg-blue-5').test('.bg-blue-500{}') === false);
check('別クラスの途中に当たらない', tokenPattern('gap-4').test('.sm\\:gap-40{}') === false);
// `.bg-amber-500\/10` は bg-amber-500/10 の定義であって bg-amber-500 のものではない。
// 直後の `\` を許すと 20 種以上を「定義済み」と誤判定する（2026-09-11 に実測）
check(
  '不透明度つきの定義を、素のクラスの定義と取り違えない',
  tokenPattern('bg-amber-500').test('.bg-amber-500\\/10{}') === false
);
check('不透明度つき自身は当たる', tokenPattern('bg-amber-500/10').test('.bg-amber-500\\/10{}'));
// 旧 src/lib.rs がベタ書きで見ていた 5 個。角括弧つきは正規表現で壊れやすい
for (const [t, sel] of [
  ['min-h-[44px]', '.min-h-\\[44px\\]{min-height:44px}'],
  ['mt-0.5', '.mt-0\\.5{margin-top:.125rem}'],
  ['gap-1.5', '.gap-1\\.5{gap:.375rem}'],
  ['text-[10px]', '.text-\\[10px\\]{font-size:10px}'],
  ['text-[11px]', '.text-\\[11px\\]{font-size:11px}'],
]) {
  check('旧テストの 5 個を照合できる: ' + t, tokenPattern(t).test(sel));
  check('旧テストの 5 個は無ければ無いと言える: ' + t, tokenPattern(t).test('.unrelated{}') === false);
}

// ---- 本物の CSS で、事故のクラスと正しいクラスの両方を通す -----------------
const cssPath = path.join(__dirname, '..', 'static', 'css', 'tailwind-precompiled.css');
if (!fs.existsSync(cssPath)) {
  console.error('NG   ' + cssPath + ' が見つかりません');
  process.exit(1);
}
const css = fs.readFileSync(cssPath, 'utf8');

// 2026-09-10 に素通りしたもの。CSS に無いと言えなければならない
for (const t of ['text-rose-400', 'bg-sky-900/30', 'text-zzz-999']) {
  check('CSS に無いと言える: ' + t, tokenPattern(t).test(css) === false);
}
// 実在するもの。あると言えなければならない
for (const t of ['text-slate-400', 'bg-navy-800', 'tabular-nums']) {
  check('CSS にあると言える: ' + t, tokenPattern(t).test(css) === true);
}

if (ng) {
  console.error('\n照合が壊れています（' + ng + ' 件）。段 2 の結果は信用できません。');
  process.exit(1);
}
console.log('\n照合は機能しています。');
