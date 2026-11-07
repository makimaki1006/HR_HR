# 面接数・採用数の月末集計

HubSpot のリクロジ_納品管理（パイプライン 21596025）の「面接数」(`mensetu`)・「採用数」(`syoudaku`) の変更履歴から、月ごとの値・月の増え・値を減らした書き換えを集計した記録。2026/11/06 作成。

- 報告: [interview-hire-counts-2026-11-06.html](interview-hire-counts-2026-11-06.html)
- 明細（取引名・担当者名を含むのでリポジトリに入れない）: `/Users/s_fujimaki/orca/workspaces/HR_HR/deliverables/interview_hire_monthly/`

## 決めたこと（ユーザー指示）

| 日付 | 決定 |
|---|---|
| 2026/11/05 | 月ごとに集計する。その月に契約が有効な取引だけを数え、面接数・採用数の両方が空の取引は除く |
| 2026/11/05 | 対象はリクロジ_納品管理だけ（請負契約_納品管理は除く） |
| 2026/11/05 | オプション取引: 主契約「にも」数がある取引（数が違う 222・同じ 52）は主契約の数を使う。オプション「しか」数が無い 21 件と、会社の結び付きが無い 15 件は数えない |
| 2026/11/05 | 契約満了による減りは除く。満了した月は、その月の月末の値を正とする。満了日の値と月末の値の差も出す |
| 2026/11/05 | 値を減らした書き換えを、数日以内の打ち間違いの訂正と、それ以外に分ける |

## 結果の要点

- 月の増え（満了による減りを除く）: 2026/09 は面接 474・採用 110、2026/10 は面接 446・採用 118。
- 2026/08 は 8/12 に 924 件の手での書き換えがあり（うち 598 件が値を減らした）、月の増えがマイナスになる。この月の数字は使えない。
- 満了日の値と月末の値の差: 245 件（取引×項目）、月末のほうが面接 +293・採用 +82 多い。
- 9/1 以降に値を減らした書き換えは 76 件。訂正とみられるのは 2 件、残り 74 件（0・空に 32、半分以上減 26、少し減 16）は訂正では説明しにくい。10/01 に 31 件が集中している。
- 月の初めの入れ直しではなかった。41 件は、継続の取引に開始前から入っていた前の契約の数を、今の契約の数に直したものとみられる。2026 年開始の継続の取引のうち 442 件（取引×項目）は開始前に数が入っており、123 件はまだ直されていない。8/12 の一斉の書き換えのうち 153 件もこの直しだった。

## 再現手順

スクリプトは `scripts/` にある。作業用のフォルダで、次の順に実行する。HubSpot の鍵は `~/Downloads/env` から `run_with_env.py` で読み込む（改行の `\r` は除く。値は出力しない）。

```sh
PY=/Users/s_fujimaki/orca/workspaces/hsa-mtg-link/.venv/bin/python   # dotenv が入っている Python
S=/Users/s_fujimaki/orca/workspaces/HR_HR/bobtail/docs/interview_hire_counts/scripts
$PY $S/run_with_env.py $S/fetch.py       # raw.json（取引と履歴）
$PY $S/run_with_env.py $S/stages.py      # stages.json（ステージの履歴）
$PY $S/run_with_env.py $S/companies.py   # deal_companies.json（取引→会社）
$PY $S/run_with_env.py $S/owners.py      # users.json（書き換えた人の ID→名前）
python3 $S/analyze.py                    # monthly.csv / detail.csv（月末の値）
python3 $S/increments.py                 # monthly_increase*.csv / expiry_vs_monthend.csv / decreases.csv
python3 $S/culprits.py                   # decrease_events_since_sep.csv
python3 $S/classify_decreases.py         # decrease_classified.csv（訂正かどうかの区分）
python3 $S/carryover.py                  # carryover.json（継続の取引に引き継いだ数）
python3 $S/build_report.py <CSV のフォルダ> <出力 HTML> <CSS>   # 報告の HTML
```

- `monthly_final.csv`（オプション取引の決まりを当てた月末の合計）と `option_vs_main.csv` は、その場で書いた処理で作り、スクリプトとして残していない。決まりは上の表のとおり。
- `build_report.py` の CSS は `docs/applicant_hearing/hearing-time-review-2026-11-05.html` の `<style>` を切り出したもの。
- 書き換えた人の名前は、リポジトリの HTML では「担当A〜」に伏せている（件数の多い順、6 人目以降は「ほか N 人」）。`build_report.py` に `--names` を付けると全員を名前入りで載せた版を作る。その版は明細のフォルダ（`interview-hire-counts-2026-11-06-names.html`）にだけ置く。

## 分け方（値を減らした書き換え）

1. 同じ人が同じ取引・項目を 30 分以内に続けて書き換えたものは 1 回にまとめる（「88 → 空 → 7」の打ち直しなど）。
2. 減らす前の値が 4 日以内しか残っておらず、新しい値がその前の値以上なら「訂正とみられる」。
3. それ以外は「訂正では説明しにくい」とし、減り方で 0・空に / 半分以上減 / 少し減 に分ける。

## 残っていること

- 継続の取引に引き継いだ数が残っている 123 件（うち契約中 22 件）の扱いを決める。
- 継続の取引を作るときに前の契約の数を入れるのか、0 から始めるのかを現場に確かめる（契約の中では累計で書いている）。
