# E2E テスト指示書: レポートデザインテーマ切替の本番検証

**作成日**: 2026-05-03
**対象 commit**: `ea0e060` (origin/main HEAD)
**実施者**: 別 AI / 別エンジニア (本セッション内のテスト実施が不能のため外注)
**ねらい**: ?theme= クエリで切替えた Working Paper / Editorial デザインが、本番環境で実 CSV 分析結果に正しく適用されているかを **目視 + DOM 検査** の両面で確認する

---

## 1. 背景

### 1.1 何を作ったか

`hellowork-deploy` (Render: https://hr-hw.onrender.com) の媒体分析タブのレポート出力に、URL クエリ `?theme=v8|v7a|default` で切替可能な **3 デザインテーマ** を追加した。

- `default`: 既存スタイル
- `v8`: Statistical Working Paper 風 (BIZ UDPGothic + 勝色 #1E3A8A + severity 多色)
- `v7a`: Editorial 風 (Noto Serif JP + アイボリー #FAFAF7 + 朱色アクセント)

実装は 5 commit (`977ff38`〜`ea0e060`)、push 済 (`origin/main` HEAD = `ea0e060`)、Render Manual Deploy 済 (ユーザー報告)。

### 1.2 なぜ別 AI に依頼するか

実装担当 AI (Claude) が同セッションで Playwright MCP プロセスを誤って kill し、再接続不能になった。本セッションでブラウザ自動化が使えないため、検証だけを切り出して別 AI に委ねる。

### 1.3 設計上の前提と制約

- マークアップは default テーマと共通。テーマ別 CSS は `[data-theme="v8"]` / `[data-theme="v7a"]` 属性セレクタで上書き形式。
- テーマ CSS は `src/handlers/survey/report_html/style.rs` の末尾に定義 (`render_theme_v8_workingpaper` / `render_theme_v7a_editorial`)。
- 既存マークアップのクラス名 (`.exec-kpi-grid-v2`, `.kpi-card-v2.kpi-good` 等) に対して上書きする方式のため、**既存マークアップに存在しないセレクタを書いてしまった場合に「効かない」事故が発生し得る**。これが本検証の主目的。

---

## 2. 検証対象 URL

ログイン後、以下 3 URL を比較する:

| URL | 期待 |
|---|---|
| `https://hr-hw.onrender.com/report/survey?session_id=<SESSION_ID>&variant=full&theme=default` | 既存の見た目 (青系 + Hiragino Kaku Gothic) |
| `https://hr-hw.onrender.com/report/survey?session_id=<SESSION_ID>&variant=full&theme=v8` | Working Paper: 紺基調 + BIZ UDPGothic + 章境界の太罫 + 黄色アクセント |
| `https://hr-hw.onrender.com/report/survey?session_id=<SESSION_ID>&variant=full&theme=v7a` | Editorial: アイボリー背景 + 明朝体 + 朱色アクセント + 余白広め |

`<SESSION_ID>` は CSV をアップロードした後に得られる文字列 (例: `s_9733e63f-aa1a-4af5-bc39-f657c69e9a37`)。本セッションで作成済の上記 ID が **現在も有効かは不明**。失効していれば 「分析データが期限切れです」と表示されるので、セットアップ手順 4 で新規取得すること。

---

## 3. セットアップ手順

### 3.1 ログイン

1. ブラウザで https://hr-hw.onrender.com/ を開く → ログインページにリダイレクトされる
2. 許可ドメイン (`@f-a-c.co.jp`, `@cyxen.co.jp`) のメールアドレス + パスワードでログイン
3. ダッシュボードトップが表示されればログイン成功

**注意**: AI がパスワードを入力するのは Anthropic の `user_privacy` ルールにより禁止。**ユーザー本人がログインしてから AI に session を引き継ぐ**こと。

### 3.2 CSV アップロード (新規 session_id 取得)

1. 上部タブから「**媒体分析**」を選択
2. ソース媒体: **Indeed** (デフォルト) のまま
3. 給与単位: **月給ベース** (デフォルト) のまま
4. テスト用 CSV をアップロード:
   - リポジトリ内 `tests/e2e/fixtures/indeed_test_50.csv` (54 行、UTF-8、約 7.5KB)
5. アップロード成功後、画面下部に分析結果が表示される
6. 以下のいずれかで `session_id` を取得:
   - 結果 HTML 内の `/report/survey?session_id=...` リンク
   - または以下の JS を browser console で実行:
     ```js
     document.body.innerHTML.match(/session_id=(s_[a-f0-9-]+)/)?.[1]
     ```

### 3.3 ブラウザ自動化での代替手順 (UI input が hidden の場合)

UI のドロップゾーンに file input が hidden 状態で、Playwright `setInputFiles` がそのままでは動かないことがある。その場合は以下の JS で fetch アップロードする:

```js
// 事前: tests/e2e/fixtures/indeed_test_50.csv の中身を base64 で window.__b64 に注入
// 例: window.__b64 = "44K/44Kk44OI..."  (ファイル全体の base64)
const bin = atob(window.__b64);
const bytes = new Uint8Array(bin.length);
for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
const file = new File([new Blob([bytes], { type: 'text/csv' })], 'indeed_test_50.csv', { type: 'text/csv' });
const fd = new FormData();
fd.append('csv_file', file);
fd.append('source_type', 'indeed');
fd.append('wage_mode', 'monthly');
const r = await fetch('/api/survey/upload', { method: 'POST', body: fd, credentials: 'include' });
const text = await r.text();
const sessionId = text.match(/session_id=(s_[a-f0-9-]+)/)?.[1];
console.log('session_id:', sessionId);
```

---

## 4. 検証項目

### 4.1 共通: テーマ切替 UI の存在確認

3 テーマすべてで、レポートページ上部に以下の UI が表示されていること (`.no-print` で印刷時非表示):

```
デザインテーマ: 現在 <テーマ名> (<説明>)
| 切替: [標準デザイン] [Working Paper 版] [Editorial 版]
```

クリックで他テーマへ遷移すること (`?theme=...` クエリの書換)。

### 4.2 V8 Working Paper (`?theme=v8`)

| # | 検証項目 | 期待値 | DOM 検査 (例) |
|---|---|---|---|
| V8-1 | `<html data-theme="v8">` 属性 | `"v8"` | `document.documentElement.getAttribute('data-theme')` |
| V8-2 | body フォント | `"BIZ UDPGothic"` を含む | `getComputedStyle(document.body).fontFamily` |
| V8-3 | 可視 h2 の border-top | `1.33px solid rgb(19, 19, 19)` (1pt = 1.33px) | `getComputedStyle(h2).borderTop` |
| V8-4 | 可視 h2 の border-bottom | `0.67px solid rgb(202, 138, 4)` (黄色 0.5pt) | 同上 |
| V8-5 | section 章境界の border-top | `~15px solid rgb(30, 58, 138)` (4mm @ 96dpi、勝色) | `.section:not(:first-child)` の computed |
| V8-6 | **table thead 背景色 (全 table)** | `rgb(30, 58, 138)` (勝色) | 全 `table thead tr` の `backgroundColor` を **配列で集める** |
| V8-7 | **table thead 文字色 (全 table)** | `rgb(255, 255, 255)` (白) | 全 `table thead th` の `color` を集める |
| V8-8 | **KPI severity 罫**: `.kpi-card-v2.kpi-good` の border-top | `~4px solid rgb(22, 163, 74)` (緑 3pt) | 同 selector の computed |
| V8-9 | KPI severity 罫: `.kpi-warn` | `~4px solid rgb(234, 88, 12)` (オレンジ) | 同 |
| V8-10 | KPI severity 罫: `.kpi-crit` | `~4px solid rgb(220, 38, 38)` (赤) | 同 |
| V8-11 | テーマ切替 UI 表示 | あり | `document.querySelector('.theme-indicator')` が `null` でない |

**重点項目**: V8-6, V8-7, V8-8〜10 が Step 5 (`ea0e060`) で修正された箇所。**default 同等の灰色のままならまだ反映されていない**ため、Render auto-deploy / cache の影響を疑うこと。

### 4.3 V7a Editorial (`?theme=v7a`)

| # | 検証項目 | 期待値 |
|---|---|---|
| V7A-1 | `<html data-theme="v7a">` | `"v7a"` |
| V7A-2 | body 背景色 | `rgb(250, 250, 247)` (オフホワイト #FAFAF7) |
| V7A-3 | body フォント | `"Noto Serif JP"` を含む |
| V7A-4 | 可視 h1 の font-size | `~42.6px` (32pt @ 96dpi) |
| V7A-5 | h1 border-top | `~5.3px solid rgb(26, 26, 26)` (4pt 黒) |
| V7A-6 | section 罫 | 細い (`~0.67px solid` 程度) |
| V7A-7 | KPI severity 罫: `.kpi-crit` | `~2px solid rgb(139, 0, 0)` (朱色 #8B0000) |
| V7A-8 | テーブル thead 背景 | `transparent` (`rgba(0, 0, 0, 0)`) |
| V7A-9 | テーブル thead 文字色 | `rgb(107, 107, 107)` (--ed-muted) |
| V7A-10 | テーブル thead font-family | `"Helvetica Neue"` 系 (Sans) |

### 4.4 Default (`?theme=default` または theme 未指定)

| # | 検証項目 | 期待値 |
|---|---|---|
| D-1 | `<html data-theme="default">` | `"default"` |
| D-2 | body フォント | `"Hiragino"` 系を含む (V8/V7a 適用前のまま) |
| D-3 | h2 border-bottom | `2px solid rgb(30, 58, 138)` (既存スタイル) |

---

## 5. テスト実行コード (Playwright JS、コピペ実行可)

### 5.1 V8 検査スクリプト

ログイン済みブラウザで以下 URL を navigate した後、page.evaluate に渡して結果を JSON で受け取る:

```js
async () => {
  const html = document.documentElement;
  const themeAttr = html.getAttribute('data-theme');

  // 可視 h2 を取得 (sr-only 除外)
  const allH2 = Array.from(document.querySelectorAll('h2'));
  const visibleH2 = allH2.filter(h => !h.classList.contains('sr-only') && h.offsetParent !== null)[0];
  const h2Style = visibleH2 ? {
    text: visibleH2.innerText.slice(0, 30),
    borderTop: getComputedStyle(visibleH2).borderTop,
    borderBottom: getComputedStyle(visibleH2).borderBottom,
    fontSize: getComputedStyle(visibleH2).fontSize,
    color: getComputedStyle(visibleH2).color,
  } : null;

  // 全 table thead 検査
  const theads = Array.from(document.querySelectorAll('table thead tr'));
  const theadAudit = theads.map((tr, i) => {
    const th = tr.querySelector('th');
    return {
      idx: i,
      bgTr: getComputedStyle(tr).backgroundColor,
      bgTh: th ? getComputedStyle(th).backgroundColor : null,
      colorTh: th ? getComputedStyle(th).color : null,
      tableClass: tr.closest('table')?.className || '(no-class)',
    };
  });

  // KPI severity
  const kpiAudit = ['kpi-good', 'kpi-warn', 'kpi-crit'].map(cls => {
    const el = document.querySelector('.' + cls);
    return el ? { cls, borderTop: getComputedStyle(el).borderTop, bg: getComputedStyle(el).backgroundColor } : { cls, missing: true };
  });

  // 章境界
  const visibleSection = Array.from(document.querySelectorAll('section, .section'))
    .filter(s => parseFloat(getComputedStyle(s).borderTopWidth) > 1)[0];
  const secStyle = visibleSection ? {
    borderTop: getComputedStyle(visibleSection).borderTop,
    marginTop: getComputedStyle(visibleSection).marginTop,
  } : null;

  // body
  const bodyFont = getComputedStyle(document.body).fontFamily;
  const bodyBg = getComputedStyle(document.body).backgroundColor;

  // テーマ UI
  const indicatorPresent = !!document.querySelector('.theme-indicator');

  return { themeAttr, bodyFont, bodyBg, h2Style, theadAudit, kpiAudit, secStyle, indicatorPresent };
}
```

### 5.2 全フローを 1 関数で

```js
async function auditAllThemes(sessionId) {
  const themes = ['default', 'v8', 'v7a'];
  const results = {};
  for (const t of themes) {
    await page.goto(`https://hr-hw.onrender.com/report/survey?session_id=${sessionId}&variant=full&theme=${t}`);
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: `theme_${t}.png`, fullPage: true });
    results[t] = await page.evaluate(/* 上記 5.1 のコード */);
  }
  return results;
}
```

---

## 6. 期待される失敗パターンと対処

| 観測 | 推定原因 | 対処 |
|---|---|---|
| `theadAudit` で `bgTh` が `rgb(229, 231, 235)` (灰色) のまま | Step 5 修正未デプロイ、または Render cache | Render Manual Deploy → ハード reload |
| `kpiAudit` の `kpi-good` 等が `null` (= class 未存在) | マークアップでクラス名が変更された | `src/handlers/survey/report_html/executive_summary.rs` 等で実 class を grep して報告 |
| `secStyle.borderTop` が `22.66px` のまま (= 6mm) | Step 5 修正未デプロイ | 同上 |
| `data-theme` 属性が設定されない | Rust 側で `theme` クエリが取得できていない | `src/handlers/survey/handlers.rs` の `query.theme` 周辺を確認、ログ収集 |

---

## 7. 報告要求

### 7.1 提出物

別 AI は以下を含む報告を提出すること:

1. **判定サマリ表**: 4.2 / 4.3 / 4.4 の検証項目それぞれに `PASS` / `FAIL` / `INCONCLUSIVE` を付与
2. **DOM 検査結果 (raw JSON)**: 5.1 のスクリプトの戻り値を 3 テーマ分そのまま貼付
3. **スクリーンショット 3 枚**: `theme_default.png`, `theme_v8.png`, `theme_v7a.png` (フルページ)
4. **FAIL 項目の根拠**: それぞれの FAIL に対し、期待値・実測値・推定原因を 1 〜 3 行で
5. **再現条件**: ログイン情報以外の前提 (使用 CSV, session_id, ブラウザ, 検証時刻 UTC)
6. **未実施項目**: 検証できなかった項目があれば理由とともに列挙

### 7.2 提出フォーマット (Markdown)

```markdown
## E2E テスト報告: テーマ切替 (audit_2026_05_03)

実施日時: YYYY-MM-DD HH:MM (UTC)
session_id: s_xxxxxxxx
ブラウザ: <Chrome version> / Playwright vX.Y

### 判定サマリ

| カテゴリ | PASS | FAIL | INCONCLUSIVE |
|---|---|---|---|
| V8 (11 項目) | x/11 | x/11 | x/11 |
| V7a (10 項目) | x/10 | x/10 | x/10 |
| Default (3 項目) | x/3 | x/3 | x/3 |
| 共通 (UI) | x/1 | x/1 | x/1 |

### V8 検証結果

| # | 検証項目 | 期待 | 実測 | 判定 |
|---|---|---|---|---|
| V8-1 | data-theme | "v8" | "v8" | PASS |
| V8-6 | thead bg | rgb(30,58,138) | rgb(229,231,235) | FAIL |
| ... | ... | ... | ... | ... |

### FAIL 根拠

- **V8-6**: 全 6 件の table の thead bg が灰色のまま (rgb(229,231,235))。
  推定原因: `[data-theme="v8"] table thead tr { background: var(--wp-brand) !important; }` が Render に反映されていない。
  対処案: Render Manual Deploy 完了確認、または特定テーブルが iframe 内などで親 selector に当たらない。

### DOM 検査 raw JSON

```json
{
  "default": { ... },
  "v8": { ... },
  "v7a": { ... }
}
```

### スクリーンショット

(3 枚添付)

### 未実施

- V7A-7 KPI severity 罫: kpi-crit が default ページには存在せず確認不能。
```

---

## 8. 関連ファイル (実装側、本検証では読み取り専用)

| パス | 役割 |
|---|---|
| `src/handlers/survey/report_html/style.rs` | テーマ CSS 本体 (L2199- が V8、L2459- が V7a) |
| `src/handlers/survey/report_html/mod.rs` | `ReportTheme` enum、`render_css_for_theme`、`render_theme_indicator` |
| `src/handlers/survey/handlers.rs` | `IntegrateQuery.theme`、`survey_report_html` で v3_themed 呼出 |
| `src/handlers/survey/report_html/executive_summary.rs` | KPI マークアップ (`.kpi-card-v2.kpi-good` 等) |
| `tests/e2e/fixtures/indeed_test_50.csv` | テスト用 CSV (54 行) |
| `docs/audit_2026_05_01_csv_panic_fix.md` | 直前のセッションでの CSV panic 修正記録 |

## 9. 参考: 直近 commit 履歴

```
ea0e060 Step 5/N: theme CSS selector fixes from live render audit
c437e4b Step 4/N: retract static sample HTMLs (misimplementation)
acfa84b Step 3/N: theme switch UI (3 themes inline)
c98d01a Step 2/N: V8 Working Paper / V7a Editorial theme CSS
977ff38 Step 1/N: report theme switch piping (default behavior preserved)
```

## 10. 既知の限界

- 本指示書は **本番デプロイ済**を前提。Render Manual Deploy が完了していない場合は Step 4 (`c437e4b`) 時点の挙動が観測される (table thead 灰色、KPI severity 罫なし、章境界 6mm)。
- Playwright 自動化を使う場合、UI の file input が hidden で `setInputFiles` 直接呼出が動かない既知問題あり。3.3 の fetch アップロード手順を使うこと。
- 認証は人間による手動ログインのみ可。AI による password 入力は禁止。
- ログイン済みブラウザの cookie を別プロセスに引き継ぐのは難しい。**1 ブラウザインスタンス内で全フローを完結**させること。
