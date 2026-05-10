# Round 10 Phase 1: ユーザー手動投入手順書

**日付**: 2026-05-10
**性質**: ユーザー手動オペレーション (Claude DB 書込禁止)
**対象**: target_thickness を percentile-based に置換 + Local DB 再生成 + 検証 + Turso 投入

---

## 前提

- build script 修正済 (`scripts/build_municipality_target_thickness.py:1189-1217` の `derive_thickness_index`)
- Pre snapshot 取得済 (`data/_tmp_phase1_pre_snapshot.csv`、Phase 1 前の 1,895 muni 順位)
- 検証スクリプト作成済 (`scripts/_tmp_phase1_verify.py`)

---

## ステップ 1: ローカル CSV 再生成

```powershell
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts\build_municipality_target_thickness.py --csv-only
```

期待出力:
- `data/generated/v2_municipality_target_thickness.csv` (~21,066 行)
- thickness 値が cap=200 ベタ→ percentile×200 (0..200 連続分布) に変化

---

## ステップ 2: Local DB に再投入

```powershell
python scripts\ingest_v2_thickness_to_local.py --apply
```

期待:
- `v2_municipality_target_thickness` 全行 DROP + INSERT
- 行数 ≈ 20,845 (変動なし)

加えて `municipality_recruiting_scores` も再生成 (thickness 値が変わったため):

```powershell
python scripts\build_municipality_recruiting_scores.py --apply
# (既存の build → ingest スクリプトを使用、コマンド名は要確認)
```

---

## ステップ 3: ローカル検証

```powershell
python scripts\_tmp_phase1_verify.py
```

**期待出力**:
```
--- Q1: 同値率 ---
  distinct=11: > 1700 muni (Phase 1 前 1,522 → 増加)
  distinct=1:  < 5 muni (Phase 1 前 104 → 大幅削減)

--- Q2: cap saturation ---
  全職業 cap muni: 0 (Phase 1 前 104)

--- Q3-Q4: ランキング整合性 ---
  上位 100 重複率: ≥ 80%
  順位差中央値: ≤ 50

--- Q5: range 不変条件 ---
  score / thickness range OOR: 0 / 0

✅ 全不変条件 PASS
```

不変条件違反が出た場合は **Turso 投入を中止**して原因調査。

---

## ステップ 4: ローカル PDF 検証

```powershell
# 8080 で rust_dashboard.exe 起動 (release ビルド)
$env:PORT="8080"
Start-Process -FilePath "target\release\rust_dashboard.exe" -PassThru -WindowStyle Hidden

# spec 実行 (旧 PDF 削除→再生成、固定手順)
Remove-Item -Force out\round8_p0_1_local\mi_local.pdf
$env:E2E_RUN_PDF="1"
$env:BASE_URL="http://localhost:8080"
$env:E2E_EMAIL="test@f-a-c.co.jp"
$env:E2E_PASS="test123"
npx playwright test _round8_p0_1_local --project=chromium
```

**確認ポイント**:
- 配信ランキング P25 で大都市自治体が複数代表職種を持つ (cap saturation 解消)
- KPI「配信検証候補(>=160)」件数が大きく変動していないか
- 「大都市圏の cap saturation」注記が削除/更新されているか (本ラウンドで Rust 側更新)

---

## ステップ 5: Turso 投入 (本番反映)

無料枠リセット後に実行 (memory `feedback_turso_upload_once` 準拠、1 回のみ)。

```powershell
python scripts\upload_phase3_step5.py
# または
python scripts\upload_to_turso.py
# (既存スクリプトの正確なコマンド名は確認)
```

---

## ステップ 6: 本番 PDF 検証

```powershell
Remove-Item -Force out\round8_p0_1_prod\mi_prod.pdf
$env:BASE_URL="https://hr-hw.onrender.com"
$env:E2E_EMAIL="s_fujimaki@f-a-c.co.jp"
$env:E2E_PASS="fac_2026"
npx playwright test _round8_p0_1_prod --project=chromium
```

固定手順 (PDF 削除 → 新規生成 → 文言 grep + regression check) で確認。

---

## ステップ 7: 後始末

```powershell
Remove-Item -Force scripts\_tmp_phase1_dump_pre.py
Remove-Item -Force scripts\_tmp_phase1_verify.py
Remove-Item -Force data\_tmp_phase1_pre_snapshot.csv
```

---

## ロールバック手順 (失敗時)

build script を git revert:
```powershell
git checkout HEAD~1 -- scripts/build_municipality_target_thickness.py
python scripts\build_municipality_target_thickness.py --csv-only
python scripts\ingest_v2_thickness_to_local.py --apply
```

Turso 投入後に失敗が判明した場合は、git revert 後に再投入 (無料枠 1 回追加消費)。
