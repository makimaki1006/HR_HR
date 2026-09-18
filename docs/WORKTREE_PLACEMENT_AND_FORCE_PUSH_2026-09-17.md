# 作業ツリーの置き場所と force push の事故（2026-09-17）

同じ日に 2 つの事故が起きた。どちらも**作業場所**が根にある。

---

## 事故 1 : Temp 配下の作業ツリーからファイルが消える

### 何が起きたか

作業ツリーが `C:\Users\fuji1\AppData\Local\Temp\hr_probe` にあった。
Windows は `Temp` を一時ファイル置き場とみなし、空き容量が減ると
**ストレージセンサーが中身を自動削除する**。

同じ日に 2 回起きた。

| 回 | 消えたもの |
|---|---|
| 1 回目 | 298 ファイル / 62,904 行。`docs/` の文書、47 県ぶんの地図データ（`data/geojson_gz/`）、`.claude/` の設定、`assets/`、`.cargo/` |
| 2 回目 | 27 ファイル。VRT の基準画像（`tests/vrt/__screenshots__/`）、テスト用 CSV、`.dockerignore` |

**`src/` `static/` `templates/` `tests/*.rs` は無事だった。**
直前に触ったファイルは残り、古いものから消える。

### なぜ危険か

消えたことに気づかずコミットすると、**「削除する変更」として記録される**。

```
git status
  D docs/USER_GUIDE.md                  ← Windows が消しただけなのに
  D data/geojson_gz/13_tokyo.json.gz    ← git には「削除した」と見える
  D tests/vrt/__screenshots__/…
```

1 回目は 62,904 行の削除が混ざりかけた。`git status` を見て気づいたので防げたが、
**見落とせばそのまま GitHub に入る**。

### 直し方

`.git` 本体が別の場所（`OneDrive/デスクトップ/HR_HR`）にあるので復元できる。

```bash
git restore --source=HEAD -- .
```

### 再発防止

作業ツリーを `Temp` の外に移す。

```bash
git -C "<本体リポジトリ>" worktree move \
  "C:/Users/fuji1/AppData/Local/Temp/hr_probe" \
  "C:/Users/fuji1/dev/hr_probe"
```

移動前にサーバとビルドを止めること（ファイルを掴んでいると失敗する）。
移動後もビルドキャッシュは効く（実測 0.97 秒で `Finished`）。

**OneDrive の中は避ける。** Rust のビルドで `target/deps` が同期対象になり、
リンカが大量の `No such file` を出す事故が過去にある。

### まだ Temp 配下にあるもの（2026-09-17 時点）

```
C:/Users/fuji1/AppData/Local/Temp/HR_HR_salesnow_map   [feature/salesnow-map]
C:/Users/fuji1/AppData/Local/Temp/hr_indeed_wt4        [feat/indeed-charts]
C:/Users/fuji1/AppData/Local/Temp/hrhr_verify_p12p13   (detached HEAD)
```

いずれも未コミットが 839〜920 件ある（大半は Temp の削除による見かけ上の削除と思われる）。
使う前に `git status` を確認し、必要なら `git restore` してから移すこと。

---

## 事故 2 : force push で main が 6 日前に巻き戻る

### 何が起きたか

```
2026-09-17 14:39 JST   main が 891b379 → efe86c4 へ force push
                       efe86c4 は 9/11 02:54 作成の古いコミット
                       → 今日の作業 9 件が main から消えた
                       → Render が自動デプロイし、本番も巻き戻った

2026-09-17 14:41 JST   その 2 分後、同じ 9 件が別 SHA で main に戻る
```

**2 分での復旧は人の手では考えにくく、何らかの自動処理**と見られる。

### 分かったこと

- 実行元はこの機械ではない（`origin/main` の reflog に push の記録が無く、
  あるのは `forced-update` を検出した fetch だけ）
- ワークフローにもスクリプトにも `git push` は無い
- GitHub の PushEvent はアカウント `makimaki1006` から
- `main` に入った 40 件のうちに
  `Merge branch 'feat/weekly-snapshot' of C:/Users/fuji1/AppData/Local/hrhr_snapshot`
  があり、**別クローンの存在**を示す（そのフォルダは現在存在しない）

### 原因（判明済み）

**別プロジェクト（Hubspot）のセッションが、公開リポジトリ HR_HR の履歴から
実名を消すために `git push --force` を実行した。**

そのセッションが自分で記録を残していた
（`~/.claude/projects/…Hubspot/memory/feedback_force_push_refetch_first.md`、
2026-09-17 16:14 作成）。

```
目的       公開リポジトリの履歴から実名を消す
クローン   9/11 の e94faf5（6 日前の状態）
実行       9/17 14:39   git push --force
出力       + 891b379...efe86c4 (forced update)
結果       9/11〜9/15 の 9 コミットが消えた
```

**remote の取り違えではない。** 意図して HR_HR に押したが、
**自分のクローンが 6 日前で止まっていることに気づいていなかった。**

14:41 の「2 分後の復旧」も自動処理ではなく、そのセッションが push 出力の
`891b379...` を見て即座に気づき、他のローカルクローンから 9 件を回収して
仮名化後の履歴の上に rebase したもの。ツリーのハッシュが元と一致することも
確認されている。

なお `main` の履歴は仮名化された別物に置き換わっている。
本 PR 群（#20 / #21）はその新しい履歴の上に乗っている。
作者名は `makimaki1006` のみであることを確認済み。

### 調査で回り道した点

「別プロジェクトが HR_HR を remote に持っているのでは」と推測し、
スクレイピングのプロジェクト（`recruit-media-pipeline`）を疑ったが**外れ**だった。
そちらの remote は自分のリポジトリを指しており、設定に `HR_HR` の文字列は無い。

**別セッションのメモリを先に探すべきだった。**
`~/.claude/projects/*/memory/` を横断で検索すれば、
自認の記録が残っていることにすぐ気づけた。

```bash
find ~/.claude/projects -name 'feedback_force_push*'
```

### 相手セッションが残した教訓（取り込み）

- 🔴 **`--force` を使わない。`--force-with-lease=<branch>:<期待するSHA>` を使う。**
  押す先が動いていれば失敗して止まる。**今回もこれなら消えなかった**
- 押す直前に必ず `git fetch` して `<クローン地点>..origin/<branch>` を見る。
  空でなければ取り込んでから押す
- 履歴を書き換えるときは先に `--all` の bundle を取る
- 消えたものは他のローカルクローン／worktree から回収できる。
  `git cat-file -t <sha>` で持っているクローンが見つかる

### 復旧の手順（記録）

1. 退避ブランチを作る（**先にこれをやる**）
   ```bash
   git branch rescue/before-force-push <巻き戻し前の SHA>
   ```
2. 自分の作業だけを新しい main の上に乗せ直す
   ```bash
   git rebase --onto origin/main <分岐点> <自分の先端>
   ```
3. CI と同じ検査を全部通す（後述）
4. **新しいブランチで push し、PR 経由でマージする**。
   `main` に直接 force push しない（また巻き戻っても被害が出ない）

### 保全したもの

```
rescue/before-force-push   891b379   巻き戻し前の main
```

---

## 副産物 : CI が赤いまま放置されていた

force push で巻き戻ったとき、**整形済みの状態も一緒に失われていた**。

```
891b379（巻き戻し前）   CI success
efe86c4（force push 後） CI failure
```

隠れていた壊れが 2 つ見つかった。

| | |
|---|---|
| `cargo fmt` の崩れ | 432 箇所 / 40 ファイル |
| `examples/dump_sales_kpi.rs` | `Sheets` に足された 3 項目に追随しておらず `E0063`。**`cargo build` は examples を組み立てないので、`cargo clippy --all-targets` でしか検出できない**。その CI が `fmt` の段階で先に落ちていたため隠れていた |

さらに `security-audit` も赤で、中身が分からなかった
（GitHub のログにも注釈にも脆弱性 ID が出力されない）。
手元で `cargo audit` を走らせて特定した。

```
RUSTSEC-2026-0258  h2 0.4.13      空の DATA フレームを無制限に受け付ける（2026-08-17 公表）
RUSTSEC-2026-0285  rustls 0.23.37 TLS 1.3 のハンドシェイクを暗号化レベルをまたいで
                                  誤って受理（深刻度 5.3 medium、2026-09-14 公表）
```

どちらも 8 月・9 月に公表されたばかりで、`.cargo/audit.toml` の許容リスト
（2026-08-02 評価）に無いのは当然。
**「新しく増えた脆弱性を赤で気づく」という仕組みは設計どおり働いていた。**

---

## push 前に回す検査（CI と同じ手順）

`cargo fmt -- --check` だけ見て push し、次の `cargo clippy` で落ちて
1 往復無駄にした。**4 つ全部を回してから push すること。**

```bash
cargo fmt -- --check          # 1
cargo clippy --all-targets    # 2  ← examples の壊れはここでしか出ない
cargo build --release         # 3
cargo test --lib              # 4
```

あわせて、この画面固有の検査も回す。

```bash
cargo test --test css_classes_exist   # 配布 CSS に無いクラスを検出
cargo test --test indeed_data_test
cargo audit                           # 脆弱性
```
