# 営業KPI (`/sales-kpi`) 表示仕様の棚卸し — React 移行 (W2) 用

作成: 2026-09-29 / 元: `templates/tabs/sales_kpi.html` (origin/main ad7d918、1,491 行、JS は 308〜1488 行)、`src/handlers/sales_kpi/routes.rs build_payload()`。
React 実装: `frontend/src/screens/sales-kpi/`。ここに書いた式は `calc.ts` の同名関数に 1:1 で対応させ、`calc.legacy.test.ts` で旧 JS の関数コピーと同じ入力→同じ出力を確認する。

## 0. データ
- `GET /api/sales-kpi/data`(`location.search` に `refresh=1` があれば `?refresh=1` を付ける)。JSON 1 本。
- 失敗時: 「データを読み込めませんでした。」+ エラー文 + 「スプレッドシートの KPI営業_ シートが揃っているか、GAS の sales_kpi_sync が動いているかを確認してください。」
- `TODAY = generated_at.slice(0,10)`(時刻付きなので必ず切る)。
- 表示切替(明暗): `localStorage['salesKpi.theme.v1']`、`<html data-theme="dark">`。既定は明るい。
- 担当者チェック(この画面だけの除外): `localStorage['salesKpi.hidden.v1']` = ownerId の配列。サーバには送らない。

## 1. 共通の式(旧 JS → calc.ts)
- `fmt(n)`: null → `—`、それ以外 `toLocaleString('ja-JP')`。
- `pct(n)`: null → `—`、それ以外 `toFixed(1)+'%'`。
- `md(s)`: `M/D`(ゼロ埋めなし)。`wd(s)`: 曜日 1 文字(日月火水木金土)。`ago(s)`: `TODAY − s` の日数(四捨五入)。
- `TEAM_OF[id] = people[].team`。`isSalesTeam(t) = !!t && t !== 'チーム未設定'`。
- `sumIf(byPerson, ok)`: hidden に入っていない & ok(id) の人の Counts をキーごとに足す。
- `sumScope(byPerson)`: person 指定なら `id===person`、そうでなければ `team==='すべて' || TEAM_OF[id]===team`。
- `sumTeam(byPerson, teamName)`: `!teamName || TEAM_OF[id]===teamName`(個人選択は見ない。平均の分子)。
- `teamLabel(p)`: `p.team` + (team が「チーム未設定」かつ hsTeam あり → `・hsTeam`)。
- `headOf(t)`: `people.filter(team===t && !hidden).length`。
- `avgBase()`: person あり → `{label: team+' の平均', team, n: headOf(team)}`; team 指定 → `{label:'1人あたり', team, n: headOf(team)}`; すべて → `{label:'1人あたり', team:null, n: hidden を除いた people 数}`。
- `avgLine(ab, total, unit='件')`: `ab.n` が 0 なら空。`label + ' ' + (total/n).toFixed(1) + unit + '（n名）'`。率のカードには付けない。
- `pick(rows)`: `!hidden.has(owner) && (person ? owner===person : team==='すべて' || row.team===team)`。
- `wow(now, prev, unit, invert)`: prev null → なし。d=now−prev。d=0 → 「先週と同じ」(faint)。good = invert ? d<0 : d>0 → ok/alert 色。`▲|▼ |d|unit 先週 fmt(prev)unit`。
- `growText(n)`: null → `—`、0 → `±0`、正 → `+n`、負 → `n`。`growColor`: null/0 → faint、正 → ok、負 → alert。

## 2. ヘッダー・絞り込み
- h1「営業KPI」、range: `2026年9月　／　今週 M/D（曜）〜M/D（曜）　※<generated_at> 時点`(「2026年9月」は固定文字列)。
- 「← ダッシュボードへ戻る」(`/`)、「◐ 表示切替」。
- タブ(2 つ以上あるときだけバーを出す): 営業KPI / 決定者・決裁者(`kettei.rows.length>0 || kettei.no_owner` のとき) / リストの在庫(`list_stock.lists.length>0` のとき)。初期は営業KPI、選択は覚えない。
- チーム chip: 「すべて」+ `teams[]`(順序そのまま)。押すと person を空にし openKey を閉じる。
- 個人 select: 「個人で見る…」+ hidden を除き team に合う people(`name`、すべてのときは `（teamLabel）` 付き)。
- 「担当者を選ぶ」→ パネル開閉(ボタン文言「担当者を閉じる」)。pickcount: 外している人数 `N名を外しています`(warn) / `N名すべて入っています`(faint)。
- パネル: 見出し「数字に入れる担当者」+「全部戻す」。チームごと(営業チーム先、チーム未設定を最後、各内は name の ja ロケール順)。チーム見出しチェック = 全員入り切り(indeterminate 対応)、`on / total名`。個人チェックを外して person と同じなら person を空に。
- scope 文: person → `<name> の数字だけを表示しています。` + (Counts が空なら `この担当者には今月の商談がありません（架電リストの数字だけ出ます）。`); すべて → `全チームの合計を表示しています。チーム名か、右のプルダウンで絞り込めます。`; チーム → `<team> の数字だけを表示しています。`; 末尾に hidden があれば `　N名をチェックで外しています。`

## 3. 今月の成績(cards1)
- fixlinks: 名簿(②メンバー配置_入力)/集計除外(KPI営業_集計除外)へのリンク(URL は旧画面の定数)。
- bporule 文(固定)。scope2: `excluded['件数']>0` のとき「商談のもとデータからは <k n件 ／ …> を除いています（誰を除くかは KPI営業_集計除外 で変えられます。架電と架電リストは除いていません）。」
- `a = sumScope(by_person)`、`den = 実施+未実施+未処理+要判定`、`rate = den ? 実施/den*100 : null`。
- 内 BPO: `v = a['bpo_'+k]`、`t = a[k]`、v>0 のとき `内 BPO v件（round(v/t*100)%）`。
- `AB = avgBase()`、`TT = sumTeam(by_person, AB.team)`、`tden` は TT の 4 項の和。
- カード順(7 枚): ① 取ったアポ `a.apo`(hint 今月アポ日が確定した数、内BPO、avg TT.apo) / ③ 商談の予定 `a.pool` / ④ 日が過ぎた分 `den`(avg tden) / ② やった商談 `a['実施']` / ⑥ 商談化率 `rate`%(hint `実施 ÷ den 件`) / ⑤ アンケート回収率 `anq_den ? anq_num/anq_den*100 : null`(hint `num ÷ den 件（④ 日が過ぎた分と同じ母数）`) / ⑨ 持っているCヨミ `a.cyomi`。

## 4. いま手を打てること(cards2 + panel)
- `stale=pick(D.stale)`, `anq=pick(anq_missing)`, `cys=pick(cyomi_stale)`, `wk=pick(week_deals)`, `nx=pick(next_week_deals)`。
- カード(5): ⑦ ステージが止まっている(件数; alert/ok; hint「商談日が過ぎたのに動いていない」/「ありません」) / ⑤ アンケート未回収(warn/ok; 「今週・来週これからの商談のうち」) / ⑨ Cヨミで止まっている(warn/ok; 「30日以上ステージが動いていない」) / ③ 今週の商談(hint `うち N件 は日が過ぎました`、N = past の数) / ③ 来週の商談(hint `M/D〜M/D の予定`)。押すと `一覧を見る ▾`/`閉じる ▲`。
- パネル: 見出し `<title>（N件）` + 「閉じる ✕」+ 説明文。0 件は「該当はありません。」
  - stale: 行の左に `ago(date)日前`、`.stale`(左赤線)。cyomi: `days日`。anq: `time`。
  - week/next: 日別ストリップ(週の start..end 全日、0 件も出す)。`past = rows.length ? rows[0].past : dt < TODAY`、today、on。日を押すとその日の一覧(`done = rows[0].past` で薄く)、「この日を閉じる」。「全部の日をまとめて見る ▾」で全日展開、「たたむ」。
- 行(item): `M/D（曜）` + sub / 取引名 / ownerName / 「HubSpotを開く ›」(href=row.url、新規タブ)。※サーバ JSON に `url` は無い(旧画面でも href は undefined)。

## 5. 架電(cards3 + kadenbar)
- 期間 chip 順: 今週 / 先週 / 今日 / 昨日 / 今月。初期「今週」。
- `per = calls.periods[callPeriod]`、`prevP = callPeriod==='this_week' ? periods.prev_week_same : null`。`cur = sumScope(per.by_person)`、`prv = sumScope(prevP.by_person)`、`KT = sumTeam(per.by_person, AB.team)`。
- `calls=cur.calls, conn=cur.connected, lng=cur.long, dl=per.days`。`upto = calls.last_day || calls.generated_at`、`partial = last_day_partial`、`asof = fetched_at.slice(11,16)`。
- lead3: 固定文 + fresh(dl 空 → 「この期間の架電はまだ集計されていません。（集計済みは M/D まで）」/ partial && dl 末尾===upto → 「M/D は HH:MM 時点の数です。その日の途中までしか入っていません。取り直すのは平日の 6:30 と 18:00 で、その間は数字が変わりません。」)。
- カード(5): 架電数 `dl.length ? conn : null`(hint 期間 M/D〜M/D、sub2「Zoomでつながった通話の数（M/D は HH:MM 時点）」、avg KT.connected、wow vs prv.connected) / 発信した回数 `calls`(avg KT.calls) / つながった率 `calls ? conn/calls*100 : null`(hint `conn ÷ calls 件`) / 5分超の通話 `lng`(avg KT.long) / 1日あたりの架電数 `dl.length ? round(conn/dl.length) : null`(hint `N日で割った平均`、平均なし)。
- 日別グラフ: `daily.filter(calls>100)`。SVG 880×150、ml44 mr10 mt12 mb28。y 軸 0/mx/2/mx、バー幅 min(46, bw*0.64)、灰=calls、青=connected(高さ最小 2)。ツールチップ 架電/つながった/5分超。凡例 つながった/つながらず。hidden があれば「チェックの絞り込みは効きません」。
- 人別表: `per.by_person` を hidden/person/team(calls.people の team)で絞り、conn 降順 40 名。列: 担当/チーム/架電数(太字)/発信回数/つながった率/5分超/(prevP のとき)前週比 `+d`(ok/alert/faint)。注記: `担当者に紐づいた発信は cur.calls件 ／ 全体 per.total.calls件。紐づかない分は他部署の発信です（unmatched_by_dept 上位3）。`
- periods が無いとき lead3「架電データがありません。」

## 6. 架電リストの残り(cards3b + listbar)
- `salesCls = sumIf(K.by_person, isSalesTeam(TEAM_OF))`、`unCls = sumIf(K.by_person, !isSalesTeam)` + `no_owner` の 未架電/未接触/接触済み/base。`noOwn = no_owner.base`。`nTeams = by_team のキーから チーム未設定 を除いた数`。
- scope: `!has_by_owner` → 全社のとき `{c: K.cls, base: K.base, who:'会社全体', whole}`、それ以外 null(「担当者ごとの内訳がまだ集計されていません。」); 全社 → `{salesCls, who:'営業Nチームの合計', whole}`; それ以外 `sumScope(K.by_person)`、who = `<name> が持っている分` / `<team> が持っている分`。base 0 → `who：架電リストに取引がありません。`
- カード(4): まだかけていない 未架電(hint `pct(未架電/sb*100) を占めます`) / つながらず 未接触 / 話せた 接触済み / 手をつけた割合 `(未接触+接触済み)/sb*100`%(hint `n ÷ sb 件`、sub2 `母数 sb件`)。
- 帯: 未架電 warn / 未接触 alert / 接触済み accent、幅 %。凡例 `k n件`。
- 決定者・決裁者の入力状況(アポ前リスト全体 K.total 件): `fill[k]/total` の % 並び + 「ほぼ入っていません。」+ (kettei タブありなら「担当者ごとの件数は上の「決定者・決裁者」タブにあります。」)。
- まだ配られていないリスト(全社 && has_by_owner && unCls.base): `unCls.base` 件、内訳文、表(unassigned.people の base>0 && !hidden 上位 8 + 担当者なし行)、母数注記(`all.base` 件、`base_trend` があれば `<week> の記録 n件 より ＋/−n`)。

## 7. 先週との比べ方(snapbox)
- chip: その週の商談(week) / 当月の累積(month)。初期 week。lead4: S.length<2 なら前置き。全社の記録である注記、hidden なら「担当者のチェックも効きません」。
- `shown = snapshots.slice(-8)`。行: 週(`week` + `M/D の週`) / 商談・やった・商談化率(`src = week ? week_totals : totals`。null なら colspan 3「この数え方で記録する前の週です」、missing++) / 架電数(`zoom_called` null → `—`、zoom_partial → `N日目まで（集計中）`) / 取ったアポ `totals.apo` / 止まっている `stale` / 架電リスト母数(`kaden_base` 0 → `—`、前行との差 `＋/−n`)。
- week && week_partial → pool の下に「週の途中（集計中）」。rate = `dn ? pct(実施/dn*100) : '—'`。
- 表下の注記(モード別文 + missing 文 + 固定文)。

## 8. 決定者・決裁者タブ
- `rows = pick(kettei.rows)`、`NO = (すべて && !person) ? kettei.no_owner : null`。asof `M/D（曜）` or `—`。
- lead5: 「毎朝6:30 に取り直した数」、prev_date ありなら「「本日増加」は前の記録（M/D 朝）からの増加です。」なければ warn 文。合計は項目の数、の注記。
- rows も NO も無ければ「選んでいる範囲に、決定者・決裁者を入力した担当者がいません。」
- 表: 担当者 / cols… / 合計(項目の数) / 本日増加(`M/D 朝からの増加` or `前の記録がありません`)。行: `fmt(o[c])`、合計太字、増加 growText/growColor。NO 行「（担当者が入っていない）担当者が居ないのでチェックでは外せません」。
- tfoot: `合計 <who>・N名(＋担当なし)` / 各列合計 / 合計 / 増加(grewKnown があれば growText(grew) + `M行は前の記録なし`、無ければ `—`)。who = person 名 / 全社 / team。

## 9. リストの在庫タブ(絞り込み非連動)
- `LS_KINDS=['アクティブ','保管']`、`LS_NAMED = has_named`、`LS_ALL = all_band`。
- `lsNames(kind)`: 全リストの groups から kind の name を初出順に集める。`lsRows(l)`: kind ごとに names 行 → `<kind>の計`(sub) → 「その他」(sub, note「区分シートに書かれていない人・担当者なし」)。
- 表 1(リクロジ/大分/計): 列 = リスト名 + 計、named ありなら各 2 列(件数/名前あり)。sub 行は `件数 + 全体の pct(n/whole)`、名前ありは `件数の pct(named/n)`。tfoot「全体(企業人数で絞っていない)」。
- 注記: groups が無ければ「区分がまだ決まっていません。…」; trend null → 「前の週の記録がまだ無いので、増減はまだ出せません。」; あれば `<week> の記録（M/D の週）からの件数の増減: <list> 全体 +n ／ アクティブ +n ／ 保管 +n …`。
- 表 2(帯ごと、リストごとに 1 表): 列 = bands + 計。sub 行に `全体の pct(n / total[band])`、named 2 段目。tfoot 全体。`band_gap` があれば注記。

## 10. フッター
- 固定文(1人あたりの説明、⑧ 決定者以上の割合は未提供、止まっている = 直近 `stale_days` 日、架電の定義、突合 `this_week.total.calls` 件のうち `matched` 件、`データ取得: generated_at ／ HubSpot・Zoom（読み取りのみ）。自動更新はまだ行っていません。`)。

## 11. 新旧で意図的に変えた点(報告用)
- `md/wd`: 旧は `new Date(s+'T00:00:00+09:00')` をブラウザのローカル TZ で読む(JST 以外のブラウザでは日付がずれる)。新は文字列から直接組み立てる(JST ブラウザでは同一)。読めない日付は旧 `NaN/NaN` → 新 `—`。
- HTML 文字列組み立て → JSX(エスケープは React 既定)。
- エラー表示: `apiGet` は非 2xx の body を返さないため、旧の `message`/`sheet` の詳細は出ず `HTTP <status>` になる。

## 12. 実装の置き場と型の正本(2026-09-30 追記、統合担当)
- JSON の正本は Rust `src/handlers/sales_kpi/payload.rs`(`SalesKpiData` ほか、`#[derive(Serialize, TS)]`)。`serde_json::Value` から struct に置き換えたが JSON は変えていない。証明は `src/handlers/sales_kpi/tests.rs` の `payloadのjsonは置換前のスナップショットと一致する`(置換前のコードで書き出した `tests/fixtures/sales_kpi/payload_2026-09-04.json` と `serde_json::to_string` を丸ごと比較)。
- TS 型は ts-rs が `frontend/src/generated/SalesKpi*.ts` に書き出す(`cargo test --lib app_api::tests::export_ts_bindings`、i64 は `number`)。画面は `frontend/src/screens/sales-kpi/types.ts` の再エクスポート経由で引く(generated を直接 import しない)。`KetteiRow` / 週次 `totals` は列固定の型になった(`kettei.cols` で動的に引く所は `ketteiCell()` が文字列で引く)。
- Vitest の fixture `frontend/src/screens/sales-kpi/__fixtures__/payload_2026-09-04.ts` は上の JSON と同一(Rust テスト `frontendのfixtureはスナップショットと同じjson` が留める)。fixture を取り直したら両方を作り直す。
- Sheets なしで起動する: `SALES_KPI_FIXTURE_DIR=tests/fixtures/sales_kpi SALES_KPI_FIXTURE_TODAY=2026-09-04`(`src/handlers/sales_kpi/fixture.rs`。本番には置かない)。旧 `/sales-kpi` と新 `/app/sales-kpi` が同じ JSON を読む。
- 画面の登録: `spa_shell.rs` の `KNOWN_SCREENS` に `sales-kpi`、`vite.config.ts` の input に `sales-kpi`。利用記録: `lib.rs` の `meaningful_activity` が `/app/*` も `view_tab` として記録する(旧 `/tab/*` と同じ物差し)。
- 旧新一致の確認: scratchpad の `wave-a/sales-kpi/compare_sales_kpi.py`(Python Playwright)。同じ fixture サーバで 22 状態 × 両画面の textContent / class / option を比べる。結果は同ディレクトリの `compare_report.txt` とスクショ。

