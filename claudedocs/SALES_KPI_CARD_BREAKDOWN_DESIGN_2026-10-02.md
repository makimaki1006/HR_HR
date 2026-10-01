# 営業KPI「今月の成績」カードの内訳 設計書 (段階A)

作成: 2026-10-02 / ブランチ `feat/sales-kpi-card-breakdown`(origin/main 004e2d5 から)
対象: 旧画面 `/sales-kpi`(`templates/tabs/sales_kpi.html`、`src/handlers/sales_kpi/`)
状態: **設計と、実装前に落ちるテストまで。実装はまだしていない。**

## 0. 依頼(2026-10-02 ユーザー)

「今月の成績」7 枚(① 取ったアポ / ③ 商談の予定 / ④ 日が過ぎた分 / ② やった商談 / ⑥ 商談化率 / ⑤ アンケート回収率 / ⑨ 持っているCヨミ)を押すと、下段 ⑦ と同じ形の内訳パネルが開くようにする。

- チーム別 → 担当者別の件数 → 担当者を押すとその人の取引一覧(日付・取引名・担当者・HubSpotを開く)
- ⑥ は分母 ④ を 実施・未実施・未処理・要判定 に分ける。⑤ は分母 ④ を 回収済み・未回収 に分ける。分子と分母の両方を出す
- 「内 BPO n件」も件数と一覧で分かるようにする
- いまの絞り込み(チーム・担当者・チェックで外した人)にそのまま従う。**カードの数字と内訳の合計は必ず一致する**
- 行はサーバが返す。集計除外・名簿の扱いは、カードの件数と同じ関数・同じ材料から作る(routes.rs:169 の「入口で 1 回だけ落とす」)

## 1. いまの件数の作られ方(ファイル:行)

### サーバ `build_payload`(src/handlers/sales_kpi/routes.rs:153)

| 段 | 場所 | 内容 |
|---|---|---|
| 入口で除外 | routes.rs:174-196 `keep_counted` | 商談・アポ・Cヨミの 3 シートから `Person.counted == false`(メンバーシートの `集計対象 = 対象外`。mod.rs:574)の人の行を落とす。名簿に無い人は `unknown_person`(mod.rs:526)で `counted = true` |
| BPO 判定(既定) | routes.rs:197 `bpo_of` | `is_bpo(deal, 前月1日, 翌月1日)`(mod.rs:865)。前月+当月の窓 |
| ③ 母集団 | routes.rs:218-223 | `商談予定日時` が `[当月1日 00:00, 翌月1日 00:00)`。文字列比較 |
| 仕分け | routes.rs:228 `classify(deal, cutoff)`(mod.rs:189) | cutoff = 今日 00:00。ステージ/PL で確定 → 日付によらず 実施/未実施 |
| 件数キー | routes.rs:230-260 | `pool`、`<区分ラベル>`、`bpo_pool`、`bpo_<区分>`、`anq_den`(区分 ≠ これから)、`anq_num`(+アンケートあり)、`bpo_anq_den`、`bpo_anq_num` |
| ① 取ったアポ | routes.rs:264-271 | アポシートの**全行**(月で切らない。シート自体が「当月にアポ日確定へ入った取引」)。**BPO は当月の窓だけ** `is_bpo(deal, 当月1日, 翌月1日)` |
| ⑨ Cヨミ | routes.rs:275-280 | Cヨミシートの**全行**(月で切らない)。BPO は `bpo_of`(前月+当月) |
| 集計先 | routes.rs:205-216 `add` | `by_team[team][key]` と `by_person[owner][key]` を同時に +1。team は `note()`(routes.rs:540)= `person_of(members, owner).team` |

### クライアント(templates/tabs/sales_kpi.html)

- **チームの合計はサーバの `by_team` を使わず `by_person` から足し直す**(:395-413 `sumIf` / `sumScope`)。チェックで外した人 `hidden`(localStorage、:384-390)を抜くため。
- 絞り込みの規則(:409): `hidden` に無い かつ (担当者を選んでいれば その人 / でなければ チーム = 選択チーム、`TEAM_OF[id]` は `D.people` から :392)。
- カードの値(:599-659): `a = sumScope(D.by_person)`、④ `den = 実施+未実施+未処理+要判定`(**クライアントで足す**)、⑥ = 実施/den、⑤ = `anq_num/anq_den`(サーバのキー)、内BPO は `oi(k)` が `bpo_<k>` を読む(①③②⑨ のみ表示)。
- 下段 ⑦⑤⑨・今週来週は行の配列を `pick()`(:494)で絞る。こちらは `r.team`(行に入れた team)で比べている。中身は同じ `person_of` 由来なので現状は一致するが、規則が 2 か所にある。
- 一覧の部品 `itemEl()`(:528)は `a.href = r.url` を読むが、**`DealRow` に `url` が無い**(mod.rs:873)。既存の ⑦ 等の「HubSpotを開く」は `href="undefined"` になっている(初版 a8a8596 から。下段 §2-K)。

## 2. 逆証明: 素朴に作った内訳がカードとずれる入力

fixture(`tests/fixtures/sales_kpi/`、判定日 2026-09-04)を Python で数えた実値と、合成入力(テストの `synthetic_sheets()`)の例。

| # | ずれ | 入力例 | 素朴な内訳 | 防ぎ方 |
|---|---|---|---|---|
| A | **① の内BPO は窓が違う** | fixture dealId `22995477277`、BPOアポ取得日 2026-08-27(前月)。合成 `A001` | 行の bpo を他と同じ `bpo_of` で作ると 52 件、カードは 51 件 | 行の `bpo` を**カードごとの BPO 規則**で作り、件数 `bpo_apo` はその行の `bpo` から数える |
| B | ⑨ の BPO を当月窓にすると 14 → 1 件 | fixture Cヨミ 13 件が前月取得(例 `13686571845` 8/31) | — | ⑨ は `bpo_of`(現行どおり) |
| C | **④ は「日付 < 今日」ではない** | fixture 49 件(例 `13016767126`)が予定日は先だが 実施/未実施 確定。合成 `D004`(実施・9/20)`D005`(キャンセル・9/25) | 日付で切ると ④ が 49 件少ない | ④ = 区分 ≠ これから。行に `kind` を持たせ、それで切る |
| D | cutoff 当日 0:00 | fixture 当日 9/04 の予定 52 件。合成 `D003`(9/04 00:00、アポ日確定) | `date <= today` で切ると これから が 未処理 に化ける | `classify` の結果(`kind`)だけを使う。日付を見直さない |
| E | 月またぎ | fixture 8/31・10/01 の行 30 件。合成 `D001`(8/31 23:30)`D011`(10/01 00:00) | 今週の一覧 `week_deals`(週頭 8/31)を流用すると、当月外の 30 件が混ざる(今週 260 件中 30 件が 8 月) | ③ の行は ③ の件数と同じ月フィルタの出力から作る。`week_deals` を流用しない |
| F | ⑦ を ④ の「未処理」に流用 | fixture ⑦ 9 件のうち当月は 4 件(60 日さかのぼるため) | 未処理が 9 件になる | 同上。⑦ の配列を流用しない |
| G | ① は月で切らない | fixture アポ 4 件(例 `12845080757`)は予定日が当月外。合成 `A004`(10/15) | 「当月の予定」で切ると ① が 4 件少ない | ① の行はアポシートの全行(除外後) |
| H | ⑨ は「当月のステージ=C」ではない | fixture ⑨ 123 行(除外後)のうち当月予定は 37 件。予定日が空の行が 3 件(`63027908113` など)。合成 `C002` | 母集団から stage=C で拾うと 37 件 | ⑨ の行は Cヨミシートの全行(除外後)。date 空は「—」で出す |
| I | 同じ取引が複数カードに | fixture 当月の 241 件がアポシートにもある。合成 `D004` は ② と ⑨ の両方 | dealId で 1 本にまとめる(dedup)と片方から消える | **カード(=出どころのシート)ごとに別の配列**で持つ。dealId でまとめない。シート内の重複 dealId は fixture では 0 件だが、件数は行で数えているので行で持つ |
| J | 集計除外・名簿にいない人・担当なし | fixture 除外者の行: 商談 3・Cヨミ 4(例 `34145414434` は両方にある)。商談の担当なし(ownerId 空)5 行(名簿に無い owner はこの 5 行だけ。いずれも当月外)。合成 `D008`(除外)`D009`(名簿外 → `owner_7777`/チーム未設定)`D012`(担当なし) | `deals_of(sheet)` を読み直すと除外者が内訳に出る | 行は `keep_counted` 後の `all` / `apo_deals` / `cyomi_deals` から作る。名前とチームは `deal_row` → `person_of`(件数と同じ) |
| K | HubSpot リンク | 既存 `DealRow` に `url` が無い | `itemEl` を流用すると `href="undefined"` | `DealRow` に `url` を足す(既存の一覧も直る) |
| L | 絞り込みの規則が 2 つ | カードは `TEAM_OF[id]`、下段は `r.team` | 片方だけ変えるとずれる | JS に `inScope(ownerId)` を 1 つ作り、カードの合計と内訳の行の両方に使う |

## 3. 設計

### 3.1 payload への追加(既存キーは変えない)

```jsonc
"card_deals": {
  // ③ 当月の母集団(除外後)。②④⑥⑤ はここを kind / anq で切る
  "pool":  [ { "id","name","date","time","owner","ownerName","team","bpo","kind","why","anq","url" } ],
  // ① アポシートの全行(除外後)。bpo は当月の窓
  "apo":   [ { "id","name","date","time","owner","ownerName","team","bpo","kind","why","url" } ],
  // ⑨ Cヨミシートの全行(除外後)。bpo は前月+当月の窓
  "cyomi": [ { "id","name","date","time","owner","ownerName","team","bpo","kind","why","url" } ]
}
```

- 行は既存の `DealRow`(mod.rs:873)をそのまま使う。`pool` は `anq: Some(has_survey)`。`apo` / `cyomi` の `kind` は内訳では使わない(③④⑥⑤ は `pool` だけを見る)。
- `DealRow` に `pub url: String` を足す(`deal_row()` で埋める)。既存の `stale` / `week_deals` / `next_week_deals` / `cyomi_stale` / `anq_missing` も url を持つようになり、⑦ 等のリンクも直る。
- 大きさ: fixture で 537 + 245 + 123 行。1 行 200 バイト前後で 200KB 弱増える(gzip 前)。

### 3.2 件数と行を同じ関数から出す

`routes.rs` の当月ループ・①・⑨ を「**行を先に作り、件数はその行から数える**」に組み替える。

```rust
// 1. 行を作る(BPO はここでカードごとの規則で決める)
let pool_rows: Vec<DealRow> = month.iter().map(|d| {
    let (kind, why) = classify(d, &cutoff);
    let mut r = deal_row(d, kind, why, &members, bpo_of(d));
    r.anq = Some(d.has_survey);
    r
}).collect();
let apo_rows: Vec<DealRow> = apo_deals.iter()
    .map(|d| deal_row(d, Kind::Unknown, String::new(), &members, is_bpo(d, &month_lo, &month_hi)))
    .collect();
let cyomi_rows: Vec<DealRow> = cyomi_deals.iter()
    .map(|d| deal_row(d, Kind::Unknown, String::new(), &members, bpo_of(d)))
    .collect();

// 2. 件数は行と「カードの述語」から数える。述語は 1 か所に置く
for (src, rows) in [(Src::Pool, &pool_rows), (Src::Apo, &apo_rows), (Src::Cyomi, &cyomi_rows)] {
    for r in rows {
        note(&mut people, &members, &r.owner);
        for c in CARD_KEYS.iter().filter(|c| c.src == src && (c.pred)(r)) {
            add(&r.team, &r.owner, c.key);
            if r.bpo { add(&r.team, &r.owner, c.bpo_key); }
        }
    }
}
```

`CARD_KEYS`(新設、mod.rs に置く)は テストの `card_preds()` と同じ表:

| key | bpo_key | 出どころ | 述語 |
|---|---|---|---|
| `apo` | `bpo_apo` | apo | 全行 |
| `pool` | `bpo_pool` | pool | 全行 |
| `cyomi` | `bpo_cyomi` | cyomi | 全行 |
| `実施`/`未実施`/`未処理`/`これから`/`要判定` | `bpo_<同>` | pool | `kind == ラベル` |
| `anq_den` | `bpo_anq_den` | pool | `kind != これから`(= ④) |
| `anq_num` | `bpo_anq_num` | pool | `kind != これから && anq` |

注意:
- `bpo_total`(routes.rs:235-236)も `pool_rows` の `bpo` から数える。
- `cyomi_stale` の数え方(routes.rs:281-293、`Kind::Unknown` を入れている)は変えない。`cyomi_stale` キーも件数に残す。
- `note()` の呼び順が変わっても `people` の中身は同じ(HashMap)。
- `team` は `deal_row` が入れる `person.team` と `note()` の戻り値が同じ `person_of` 由来であることをテストで確認している(`内訳の行の担当者とチームは絞り込みの名簿と同じ`)。
- 週次シートを書く Python 側(`weekly_cells()`)の数え方は変わらない(同じ材料・同じ述語。行に組み替えるだけ)。

### 3.3 HubSpot URL

- コンサルKPI と同じ形 `https://app.hubspot.com/contacts/<portal>/record/0-3/<dealId>/`、portal は `HUBSPOT_PORTAL_ID`(未設定・空白なら `23708633`)。
- `src/handlers/cs_dashboard/routes.rs:410` の `hubspot_portal_id()` は `pub(super)`。`pub(crate)` に上げて営業KPI から呼ぶ(別の定数を作らない)。`call_quality` は `/contacts/23708633/deal/<id>` を直書きしているが、今回は触らない。
- `build_payload` は 1 回だけ portal を読み、`deal_row` に渡す(行ごとに環境変数を読まない)。

### 3.4 テンプレ JS の変更方針

1. 絞り込みを 1 つにする: `const inScope = id => !hidden.has(id) && (person ? id===person : (team==='すべて' || TEAM_OF[id]===team));` を作り、`sumScope` と内訳の行の絞り込みの両方で使う(`pick()` もこれに寄せてよい。r.team ではなく `TEAM_OF[r.owner]`)。
2. カード 7 枚に `key` と `onClick: toggle(key)` を付ける(下段と同じ `openKey` / `#panel` を共有。上段 `#cards1` 用に別のパネル要素を置くかは見た目で決める。下段と同時に開かない方が簡単)。
3. パネルの中身(⑦ と同じ見た目):
   - 見出し: カード名と件数(= カードの値。内訳行の数と同じになる)。内 BPO n件 を併記し、押すと BPO の行だけに絞れるトグル。
   - チーム別の表(全社のときだけ)→ チームを押すと担当者別 → 担当者を押すと `listOf(rows)`(既存の `itemEl`)。担当者を選んでいるときは最初から一覧。
   - ⑥: 分子 = `kind==='実施'`、分母 = ④。分母の内訳 4 区分(実施・未実施・未処理・要判定)を件数で並べ、区分を押すとその行の一覧。
   - ⑤: 分子 = 回収済み(`anq===true`)、分母 = ④。分母の内訳 2 区分(回収済み・未回収)。
   - ④: 4 区分の内訳と、各区分の一覧。
   - ①③②⑨: 担当者別の件数と一覧。
4. カードの値は従来どおり `by_person` から作る(既存表示を変えない)。内訳は `D.card_deals` の行を `inScope` で絞って数える。両者の一致はサーバ側のテストで担保(§4)。
5. 取引名は fixture では列が落ちているため「（取引名なし）」。本番は `取引名` 列がある。

### 3.5 クライアントの絞り込みをどう検証するか(判断)

**Rust 側で同じ規則を再現する方を採った**(テストの `Scope::has`)。理由:

- 件数(`by_person`)も行(`card_deals`)も担当者ごとに持っているので、「担当者ごとに 件数 == 行数」が成り立てば、担当者の集合で切るだけの絞り込みは**どの組み合わせでも一致する**。JS 側で守ることは「カードと内訳に同じ `inScope` を使う」の 1 点だけになる。
- 絞り込みの JS はテンプレートの中のインラインで、Node で単体テストするには切り出しが要る。営業KPI は W2 で React に移す対象(React 版は別 PR)で、そちらでは vitest で同じ規則を試せる。旧テンプレートのために切り出す費用に見合わない。
- 残るリスク(JS で別の規則を使ってしまう)は、実装後に Playwright で「カードの値 == パネル見出しの件数 == 一覧の行数」をチーム・担当者・チェック外しの数通りで見て潰す(実装者の検証項目)。

React 版(PR #37 系)も同じ `card_deals` を読めば同じ保証が効く。React 側の型は ts-rs で生成する場合 `DealRow` に `#[derive(TS)]` が要る(今回の旧画面の実装範囲外。React 担当へ申し送り)。

## 4. 実装前に落ちるテスト

`src/handlers/sales_kpi/tests/card_breakdown.rs`(`tests.rs` 末尾の `mod card_breakdown;` で読み込む)。JSON のキーで見ているのでコンパイルは通り、実装前は `card_deals` が無いところで落ちる。

| テスト | 入力 | 確かめること | 実装前 |
|---|---|---|---|
| カードの件数と内訳の行数が担当者ごとに一致する | fixture | 全担当者 × 全キー(§3.2 の表)で `by_person` == 行数、`bpo_*` == bpo 行数。pool 537 行 | 落ちる |
| チームの全組み合わせと担当者選択でカードと内訳が一致する | fixture | チームの全部分集合をチェックで外す × チーム選択(すべて+各チーム)、担当者 1 人ずつ。①③④②⑨・⑥⑤ の分子分母・内BPO・⑥分母の 4 区分の和 == ④・⑤ 回収済み+未回収 == ④ | 落ちる |
| 内訳の行の担当者とチームは絞り込みの名簿と同じ | fixture | 行の owner が `people` に居て team が一致 | 落ちる |
| 取ったアポの内bpoは当月の取得日だけ | fixture | `22995477277` の bpo = false、bpo 行 51 == `bpo_apo` | 落ちる |
| cヨミの内bpoは前月と当月の窓 | fixture | bpo 行 14 == `bpo_cyomi` | 落ちる |
| 集計除外の取引は内訳にも出ない | fixture | 除外者の owner と `34145414434` が行に無い | 落ちる |
| 日が過ぎた分には予定日が先でも結果が出た取引が入る | fixture | ④ の行のうち予定日 >= 9/04 が 49 件 | 落ちる |
| 内訳と既存の一覧の行はhubspotの取引ページを開ける | fixture | card_deals と既存 5 配列の全行の `url` | 落ちる(既存の url 欠落も含む) |
| 合成入力のカードの数字は現行の数え方どおり | 合成 | ③9・未処理2・これから1・実施4・未実施1・要判定1・④8・⑤分子1・内BPO 2・①3(BPO 1)・⑨2(BPO 1)・除外 3 | **通る**(現行の数え方の固定。実装で数字が動いたら落ちる) |
| 合成入力の内訳の行はカードの取引そのもの | 合成 | 各配列の dealId 集合、kind、bpo(A001=false / D007=true / D010=false)、anq、名簿外 `owner_7777`、担当なし、date 空 | 落ちる |
| 合成入力でもチームの全組み合わせと担当者選択でカードと内訳が一致する | 合成 | 上の全組み合わせ検査を合成入力で | 落ちる |

実装後に実装者がやること(逆証明): `apo_rows` の bpo を `bpo_of` に変える / ④ を日付で切る / `deals_of(&sheets.cyomi)` を読み直す、をそれぞれ 1 つずつ入れて、上のテストが落ちることを確かめてから戻す。

## 5. 未決・申し送り

- パネルを上段専用に置くか、下段の `#panel` を共有するか(見た目の判断)。
- React 版(W2、PR #37 系)へ: `card_deals` と `DealRow.url` を同じ契約で使う。ts-rs の型追加は React 側の作業。
- `call_quality` の HubSpot URL 直書き(`/contacts/23708633/deal/<id>`)は今回の範囲外。
