# 営業KPI: 旧画面と React 版の機能差 (2026-10-02)

対象: 旧画面 `templates/tabs/sales_kpi.html`(origin/main 8d6590f)と、React 版 `frontend/src/screens/sales-kpi/`(PR #37、origin/main に rebase 済み)。
**この文書の差は React にまだ入れていない。** 差の洗い出しだけ。

## 経緯

- React 版の元になった旧画面は dd1ffeb(PR #32 マージ時点)。その後、旧画面に 2 つの変更が入った。
  - #45 今月の成績カードの内訳(`72743a5` 以降 `943b99b` `90d544d` `9af7187`)
  - #46 担当なしの選択(`f54ac4a` `19fae49`)
- 旧画面テンプレートの差分は `git diff dd1ffeb origin/main -- templates/tabs/sales_kpi.html`(+215 / -40 行)。
- サーバ側(Rust)は今回の rebase で取り込み済み: `card_deals`(`SalesKpiCardDeals`)と `DealRow.url` を struct と ts-rs 型に足した。
  旧画面が受け取る JSON は変わっていない(origin/main と HEAD で同じ fixture・判定日 2026-09-04 の JSON が 587,838 バイト・sha256 一致)。

## 差の一覧

規模: S = 半日未満 / M = 半日〜1 日 / L = 1〜2 日。React 側の場所は現在の行ではなく関数名。

### A. #45 カードの内訳

| # | 旧画面の挙動 | React 版の現状 | React 版で必要な作業 | 規模 |
|---|---|---|---|---|
| A1 | 今月の成績 7 枚(① アポ / ③ 予定 / ④ 日が過ぎた分 / ② 実施 / ⑥ 商談化率 / ⑤ アンケート回収率 / ⑨ Cヨミ)を押すと内訳パネル `#panel1` が開く。カードの下に「一覧を見る ▾ / 閉じる ▲」 | カードは押せない。`monthView` の `CardSpec.key` はあるが onClick・isOpen が無い | `CardSpec` に開閉状態を足し、`Card` にボタン化と `.open` 表示。`openCard` 状態を Screen に追加 | S |
| A2 | `CARD_CONF`: 7 枚それぞれの題・出どころ(`pool` / `apo` / `cyomi`)・母集団の述語(④⑥⑤ は「これから」以外)・説明文・区分(`KIND_SEGS` 実施/未実施/未処理/要判定、⑤ は 回収済み/未回収)・分子分母の名前 | 無い | `calc.ts` に同じ表を移す(述語は `DealRow` を受ける純関数)。旧 JS は `legacy_sales_kpi.js` の切り出しに足して Vitest で同一性を確かめる | M |
| A3 | 見出し: 件数カードは「題（N件）」、率カード(⑥⑤)は「題 xx.x%（分子 実施 n件 ÷ 分母 日が過ぎた分 m件）」。`#panel1-title` に `data-total` / `data-num` | 無い | `cardPanelView()` を calc に作り、題・分子・分母を返す。id と data 属性は旧と同じにする(既存の `tests/e2e/sales_kpi_card_breakdown.py` を /app でも流せる) | S |
| A4 | 内 BPO チップ(`#panel1-bpo`、押すと BPO の行だけ) と、区分チップ(`data-seg`、分子には「分子 」付き、押すと切替) | 無い | state `bpoOnly` / `cardSeg` と、チップ描画 | S |
| A5 | 3 段の掘り下げ: 全社ならチーム別表 → チームを選ぶと担当者別表 → 担当者を選ぶと取引一覧。表は 件数・内 BPO・合計行(`tfoot` に `data-sum`)。「‹ チーム別に戻る」「‹ 担当者別に戻る」。上のチーム・個人の選択があるときは段を飛ばす(`teamPick` / `personPick`) | 無い | state `cardTeam` / `cardPerson`。表は既存の `DataTable` 相当を使わず、旧 DOM(`table.cdrill`、`tr[data-name]`)に合わせる。並びは「件数の多い順 → 名前(ja)」 | M |
| A6 | 内訳の行 `cardRows(src, base)` = `D.card_deals[src]` を **`inScope(owner)` で絞る**。カードの値と内訳の合計が絞り込みに関わらず一致する | `card_deals` を読んでいない(型だけ生成済み) | A1〜A5 の元データにする。値の一致は「カードの値 = 内訳の合計」を全チーム・全個人で Vitest 逆証明する | S |
| A7 | 取引一覧は既存の `listOf(mine, {})`(HubSpot リンク付き) | `Item` は `DealRow.url` を href にする(今回直した。以前は型に url が無く cast していた) | 一覧は既存の `ListOf` を再利用。一覧の頭 `#panel1-listhead`(`data-n`)と「（区分）」「（BPO だけ）」の表記 | S |
| A8 | `toggleCard`: 開閉で `cardTeam/cardPerson/cardSeg/bpoOnly` を捨て、`openKey/dayKey/weekOpen` を閉じ、`#panel1` へスクロール(50ms 後)。逆に ⑦⑤⑨ 等(`toggle`)を開くと `openCard` を閉じる。チーム・個人を変えると `openCard` を閉じる | `toggleOpen` は `openKey` だけ | Screen の `actions` に `toggleCard` を足し、相互排他と scrollIntoView(`#panel1`)を旧と同じにする | S |
| A9 | 絞り込みの規則を **`inScope(id, rowTeam)` 1 本**にした: `!hidden.has(id) && (person!==null ? id===person : team==='すべて' || teamOf(id,rowTeam)===team)`。`teamOf` は名簿に居る人は名簿のチーム、居ない人は行のチーム。`sumScope` / `pick` / `cardRows` が全部これを使う | `sumScope`(名簿 `teamOf` だけ)と `pickRows`(行の `team` だけ)が別々に書かれている | `calc.ts` に `inScope` を作り、`sumScope` / `pickRows` / A6 を寄せる。**名簿外の担当者の行**(チームを選んだときの扱い)が旧と変わるので、名簿外の合成データで旧 JS との一致を Vitest で見る | M |
| A10 | 内訳パネルが空のとき「該当はありません。」、絞り込みで 0 のとき「この条件に当てはまる取引はありません。」 | 無い | 文言と分岐を移す | S |

A の合計の目安: 1.5〜2 日(A2・A5・A9 が重い)。

### B. #46 担当なしの選択

| # | 旧画面の挙動 | React 版の現状 | 必要な作業 | 規模 |
|---|---|---|---|---|
| B1 | `person`: `null` = 個人を選んでいない / `""` = 担当なし(担当者が空の取引)/ id。判定は必ず `person!==null`。空文字を「未選択」と読むと担当なしを選んでも全員表示のまま | `Scope.person: string` で空文字 = 未選択。`calc.ts` の `scope.person ?` が 9 か所(sumScope・avgBase・pickRows・scopeText・callsView の人別・kadenListView 2 か所・ketteiView 2 か所)と Screen の `INITIAL_SCOPE` / `setTeam` / `updateHidden`。この真偽判定のままでは担当なしを選べない | `Scope.person: string \| null` に変え、全部 `!== null` に直す。型が変わるのでコンパイルが漏れを教える | M |
| B2 | 個人プルダウン: 担当なしの option の value は番兵 `__none__`(`personOfValue` / `valueOfPerson`)。未選択は `""` | `<select value={ui.scope.person}>` が id をそのまま使う | 番兵の変換 2 関数を calc に置き、select と `setPerson` の間だけで変換 | S |
| B3 | `avgBase`: `person===''` は null(平均は付けない) | 空文字は「未選択」扱いで全社の平均になる | B1 の後に分岐を足す | S |
| B4 | 見出し「今月の成績」付近の `scope` 文と、決定者の合計行の誰か(`personName()`): 担当なしは「担当なし」と出す(名簿に居なくても) | 名簿に無いと `''` や「この担当者」 | `personName` を旧と同じに(`person===''` → 「担当なし」) | S |
| B5 | 架電: 担当なしを選ぶと見出し `#h2kaden` が「架電（担当なしを選択中）」、`lead3` に警告(Zoom 架電は電話をかけた人で数える)。架電数・発信数・つながった率(hint は「—」)・5 分超・1 日あたりは値 `null`(「—」)、先週比は出さない。欄は隠さない | 見出しに id が無く、常に「架電」。担当なしの区別が無いので 0 件の人と同じ数字が出る | `callsView` に `noCall` を足す。見出し・リード文・各カードの null 化 | M |
| B6 | 架電リストの残り: 担当なしは `K.no_owner` を `scope` にし、見出し `#h2kadenlist` が「架電リストの残り（担当なしを選択中）」、`who` は「担当なし（担当者が入っていない取引）が持っている分」 | 見出しに id が無い。個人指定の分岐が `sumScope` 経由 | `kadenListView` に分岐を足し、見出しを返す | S |
| B7 | 決定者・決裁者: `NO=(person===''\|\|(team==='すべて'&&person===null))?KE.no_owner:null` | `team===全社 && !person` のとき | 条件を差し替え | S |
| B8 | 人別の架電表(`calls-by-person`)の絞り込みは `person!==null` | `scope.person` の truthy | B1 に含まれる | - |
| B9 | 個人のチェックを外したとき個人指定を解く先は `null`(以前は `''`) | `updateHidden` が `''` に戻す | `null` に | S |

B の合計の目安: 1 日(B1 の機械的な置換とテストが中心)。**A9 と B1 は同じ `inScope` を触るので、同じ PR で一緒に入れる方が安全。**

### C. それ以外の差(#45・#46 とは別)

| # | 差 | 内容 | 必要な作業 | 規模 |
|---|---|---|---|---|
| C1 | 見出しの月 | 旧画面は `'2026年9月'` を固定文字で書いている(10 月になっても 9 月と出る)。React 版も同じ固定文字だったので、**今回 `generated_at` の月に直した**(`monthLabel()`、`SalesKpiView.edge.test.tsx`)。旧画面は未修正 | 旧画面にも同じ修正を入れるか決める。入れるなら 1 行 | S |
| C2 | カードの HubSpot リンク | 旧画面は `r.url` を href にしていたが、#45 より前のサーバは url を返さず `href="undefined"` だった。#45 以降は url が付く。React 版は今回 `r.url` を href にした | 済み。ブラウザでリンク先が取引ページになることは E2E で確認 | 済 |
| C3 | ナビのリンク先 | `src/handlers/nav.rs` の営業KPI は `/sales-kpi` のまま | 移行手順 ⑦(ナビを `/app/sales-kpi` へ)は main の判断。旧 URL は残す | S |
| C4 | App Shell | React 版は画面の中に独自のヘッダー(「← ダッシュボードへ戻る」「◐ 表示切替」)を持つ。platform-team の App Shell には未接続 | Shell の `children` として差し込み、画面側のヘッダーを外す。明暗の保存キー `salesKpi.theme.v1` は Shell と衝突しないか確認 | M |
| C5 | ヘッダーフィルタ | 営業KPI は旧画面でもセッションのヘッダーフィルタを読まない(画面内のチーム・個人のみ) | 変更なし | - |
| C6 | 絞り込みの URL 反映 | 旧画面にも無い | 範囲外 | - |
| C7 | E2E | 旧画面の内訳は `tests/e2e/sales_kpi_card_breakdown.py` が検証。React には CSP 付きの E2E(`tests/e2e/pr/sales_kpi.spec.ts`)を今回足した(表示・値・CSP 違反 0) | A・B を入れるときに、内訳と担当なしの spec を足す | M |

## 入れる順番の提案

1. B1 + A9(`Scope.person` を `string | null` に、`inScope` 1 本化)。まず型で全箇所を直し、旧 JS との同一性テストを足す。
2. B2〜B9(担当なしの表示)。
3. A1〜A8, A10(内訳パネル)。`tests/e2e/sales_kpi_card_breakdown.py` を /app 向けにも流せる id・data 属性を保つ。
4. C3(ナビ切替)は A・B が入って旧新一致が取れてから。

## 注意(旧新一致の限界)

- `tests/e2e/sales_kpi_old_new_compare.py` は、画面が最初に描く領域(カード・架電・リスト・週次・決定者・在庫)を、チーム・個人・架電の期間・週次の切替・タブで切り替えながら旧新で比べる。
  **カードを押して開く内訳(`#panel1`)は操作しないので、A の未実装は不一致として出ない**(この文書で補う)。
- fixture(`tests/fixtures/sales_kpi`、判定日 2026-09-04)には担当者が空の取引が 0 件(`people` に id `""` が無い、`by_person` にも無い)ので、
  **担当なしの選択(B)も比較では出ない**。B を入れるときは、担当なしの行を足した合成データで旧 JS と同じ結果になることを別に確かめる必要がある
  (Rust 側の `build_payload` に、担当者が空の取引を足した合成シートを通した JSON を、React の Vitest fixture にする)。

## 実施状況(2026-10-04 追記 / レーン B)

上の A・B と、商談種別(#49・#50。`SALES_KPI_NEGOTIATION_TYPE_2026-10-02.md`)を React 版に入れた。3 つの PR に分けている(積み上げ順)。

| PR | ブランチ | 入れたもの |
|---|---|---|
| 1 | `feat/react-sales-kpi-parity` | B1〜B9(担当なしの選択)と A9(`inScope` 1 本化)。`Scope.person` は `string \| null` |
| 2 | `feat/react-sales-kpi-card-breakdown` | A1〜A8・A10(カードの内訳)。⑤ のカードのキーは `anq` → `anqrate`(下段の「⑤ アンケート未回収」の `anq` と別物) |
| 3 | `feat/react-sales-kpi-negotiation-type` | 商談種別の表・種別で絞る・区分 × 種別・分子の注釈・一覧の行の「・種別」。並びは payload の `negotiation_type_order` / `_fixed` に従う(JS で並べ直さない)。内部値 ⇄ ラベルの変換は Rust 側で済み、React では変換しない |

確かめ方(旧画面の `<script>` を happy-dom でそのまま動かし、同じ操作を流して `#panel1` などの DOM を突き合わせる): `frontend/src/screens/sales-kpi/__fixtures__/dual.tsx`、`parity.*.test.tsx`。
旧 JS の関数そのものとの比較は `legacy_sales_kpi.js`(`makeScoped`)。E2E は `tests/e2e/pr/sales_kpi.spec.ts`、`sales_kpi_card_breakdown.py --react <repo>`、`sales_kpi_old_new_compare.py`。

残る差(意図したもの・未対応):
- 決定者タブ・リストの在庫タブは、React 版では開いたときだけ描く(旧画面は隠れたまま常に描いてある)。ユーザーに見える違いは無い。E2E は先にタブを開いてから見る。
- C3(ナビを `/app/sales-kpi` へ)・C4(App Shell への接続)は未着手。
- PR 用 E2E の fixture(`tests/fixtures/sales_kpi`)には担当なしの取引と商談種別の列が無いので、担当なし・商談種別の中身は Vitest(合成データ、`dump_sales_kpi --negtype` の JSON)と `sales_kpi_card_breakdown.py` で確かめている。PR 用 E2E は「列が無いときの注記」までを見る。
