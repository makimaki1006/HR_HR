// コンサルダッシュボードの画面に埋め込んだ JS が、**構文として通るか**を見る。
//
// ------------------------------------------------------------------
// なぜ要るか
// ------------------------------------------------------------------
// 2026-09-21 の実害: テンプレートの JS に構文エラーが1つあり、
// `<script>` 全体が実行されなかった。結果、API が1本も呼ばれず
// 画面は真っ白のまま。それでも
//   - cargo test は 3,393 件すべて通る（Rust 側は無関係）
//   - curl で /api/consulting/* を叩くと 200 が返る（サーバは正常）
//   - /consulting は 200 を返す（HTML は出ている）
// ので、**どの確認にも引っかからなかった**。
//
// 原因は `"<h3 style=\\"...\\">"` と書いていたこと。JS では `\\` が
// 「バックスラッシュ1つ」になるので、その次の `"` で文字列が終わってしまう。
//
// ------------------------------------------------------------------
// 使い方
// ------------------------------------------------------------------
//     node tests/consulting_page_js.js
//
// サーバもブラウザも要らない。テンプレートを読んで構文を見るだけ。
// 落ちたら終了コード 1 と、何行目かを出す。
"use strict";

const fs = require("fs");
const path = require("path");
const vm = require("vm");

const TEMPLATES = [
  "templates/tabs/cs_dashboard.html",
];

let failed = 0;

for (const rel of TEMPLATES) {
  const file = path.join(__dirname, "..", rel);
  const html = fs.readFileSync(file, "utf-8");

  // Askama の差し込みは JS から見ると構文エラーになるので、先に潰す。
  // ここで見たいのは**自分が書いた JS**の構文だけ。
  const cleaned = html.replace(/\{\{[^}]*\}\}/g, '"__askama__"');

  const blocks = [...cleaned.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)];
  if (blocks.length === 0) {
    console.error(`FAIL ${rel}: <script> が1つも無い`);
    failed++;
    continue;
  }

  blocks.forEach((m, i) => {
    const js = m[1];
    try {
      // 実行はしない。構文が通るかだけを見る。
      new vm.Script(js, { filename: `${rel}#script[${i}]` });
      const lines = js.split("\n").length;
      console.log(`OK   ${rel} #script[${i}]  ${lines}行 / ${js.length}文字`);
    } catch (e) {
      console.error(`FAIL ${rel} #script[${i}]: ${e.message}`);
      // 何行目かを出す（V8 は filename:line の形で持っている）
      const at = (e.stack || "").split("\n").find((l) => l.includes(rel));
      if (at) console.error(`     ${at.trim()}`);
      failed++;
    }
  });

  // 🔴 今回の原因そのもの。JS の文字列の中で `\\"` と書くと、
  //    バックスラッシュ1つ + 文字列終わり になって以降が崩れる。
  //    HTML 属性を書きたいなら、外側をシングルクォートにすること。
  const bad = cleaned.split("\n")
    .map((l, i) => [i + 1, l])
    .filter(([, l]) => l.includes('\\\\"'));
  for (const [n, l] of bad) {
    console.error(`FAIL ${rel}:${n}: JS の文字列に \\\\" がある。` +
      `外側をシングルクォートにすること -> ${l.trim().slice(0, 90)}`);
    failed++;
  }
}

if (failed) {
  console.error(`\n${failed} 件の問題。画面の JS が動かない状態です。`);
  process.exit(1);
}
console.log("\n画面の JS は構文として通ります。");
