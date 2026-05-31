# Master Punch List — 2026-05-31 集約

最終更新: 2026-05-31
範囲: hellowork-deploy 全 pending タスク + 本セッション 2026-05-30〜31 完了済成果の総括

---

## 1. 完了済 (本セッション 2026-05-30〜31)

| Commit | 内容 | テスト |
|--------|------|--------|
| `ef7f057` | A1 #294 navy_report.rs 分割 (8004→11 module、mod 250 行 + common + 8 section + tests) | 1620 PASS |
| `22b3922` | Round 1-J 中立化 (P1 6 件: 構造的縮小/単一県集中/採用ニーズ集中/構成偏り/低水準 → 中立表現) + 出典明記 (P2 2 件) | 1620 PASS |
| `aea88e9` | Round 1-K 単位防御 P0 潜在 3 件 (daytime_nighttime_ratio / 失業率 380% / SalesNow share の二重×100) | 1620 PASS |
| `ba73114` | P0-8 Section 09 Market Intelligence variant (6 テーマ 9-A〜9-F、1030 行、test +8) | 1628 PASS |

### 本番反映確認 (2026-05-31)

hr-hw.onrender.com で MI variant レポート視覚確認:
- ✅ 11 navy section (cover/exec/region/salary/tightness/companies/demographics/lifestyle/**mi**/notes) すべて描画
- ✅ Section 09 (navy-mi) の 6 テーマ visible
- ✅ SalesNow grep 0 件 (HTML 出力)
- ✅ Round 1-J 中立化反映 (「構造的縮小」消滅、「減少局面」「低位水準」確認)
- ✅ Round 1-K 単位防御反映 (正常値域なので警告 trigger なし、debug_assert + tracing::warn は待機)

---

## 2. 着手中 (本セッション 2026-05-31 並列)

| ID | ブランチ | 内容 | 状態 |
|----|---------|------|------|
| #339 P2-4 | `fix/p2-4-pdf-visual` | PDF 視覚 12 件 (audit_2026_05_13 punch list) | 進行中 |
| #340 Round 1-K 残 | `fix/round1-k-remainder` | safe_pct 5 件 + 順序非決定 9 件 + 鮮度警告 3 件 | 進行中 |
| #341 MASTER_PUNCH_LIST | `docs/master-punch-list` | 本ドキュメント | 進行中 |
| #342 賃貸データ取得 | `feature/rental-data-acquisition` | e-Stat 住宅・土地統計 (statsCode 00200522) | 進行中 |

---

## 3. 設計メモ commit 待ち (5 件)

`salary_cluster_analysis_design.md` が repo 外。実装着手前にユーザー手元メモを `hellowork-deploy/docs/` に commit が必要。

| ID | 内容 | 既存実装の有無 |
|----|------|---------------|
| P0-9 (#238) | 顧客求人クラスタ当て込み + 適正値算出 (§9-10) | 🟡 部分: `nearest_cluster` / `cluster_so_what_text` あり、個別求人当て込みは未実装 |
| P0-10 (#239) | CSV 業界・職種推定 信頼度ラベル (§18.5-6) | 🟡 部分: 業界 85/70% バナーあり、職種は精度不足で非表示 |
| P1-7 (#241) | 給与構造クラスタへの導線 + 推定分類の根拠表示 (§18.8-10) | 🟡 部分: 文言誘導のみ、anchor/popover 未実装 |
| P2-5 (#242) | 給与構造補助フラグ (業務委託/歩合/管理職/夜勤) 抽出 (Phase 3/§17優先度C) | 🔴 未実装 |
| P2-6 (#243) | 業界・職種推定スコアリング (タイトル/タグ/仕事内容根拠別) (§17優先度C) | 🟡 部分: タグ+会社名の 2 信号源、合算スコアのみ |

**推奨着手順序**: P2-6 → P2-5 → P0-9 → P0-10 → P1-7 (Parallel B 報告書 PENDING_REQUIREMENTS_2026_05_30.md 参照)

---

## 4. 監査 pending (G/I/K/J/F)

タスク #213/#215/#216/#218/#219 は監査初期版が closed (#298/#299/#300/#303 で実施済) と見なされ、本リストでは completed task で履歴確認可能。

実装フォローアップ候補:
- **#168 Round 1-F** (既存 DB 作成可能分析確認) — 既存 Turso DB 14 テーブルから追加分析が可能なものを棚卸し
- **#171 Round 1-J** (法務/表現/出典監査) — 一部本日完了 (Round 1-J 8 件修正済)、残課題なし状態
- **#172 Round 1-K** (データ整合性/再現性/鮮度監査) — 本日 P0 潜在 3 件修正済 + 残 17 件着手中

---

## 5. 既存負債

| 項目 | 状態 | 着手判断 |
|------|------|---------|
| **security-audit 6 件 advisory** | main CI で連続 fail (依存クレート脆弱性 6 件) | cargo update / 個別 dep アップデート |
| **fac_2026 plaintext credentials** | 監査 D Critical 指摘済、ユーザー「アプリが使えるようになったら対処」と明言 | 後回し |
| **巨大ファイル top 20** | unwrap() top 20 は #263 で対応済 | navy_report 分割で 1 件 (`navy_report.rs` 8004 行) 解消、残り別ファイル |

---

## 6. 次サイクル候補 (priority 順)

| 優先度 | 候補 | 前提 |
|--------|------|------|
| P0 | 設計メモ commit (5 件着手前提) | ユーザー手元メモ提供 |
| P0 | security-audit cargo update | CI green 復活 |
| P1 | Round 1-F (DB 追加分析棚卸し) | なし |
| P1 | P0-8 Section 09 の本番デプロイ後 MI variant E2E (今は static rendering 確認のみ) | デプロイ反映後 |
| P2 | V1 ダッシュボード関連 (rust-dashboard リポ) | 別リポ |
| P2 | claude-mem / agent runtime 最適化 | なし |

---

## 7. 関連ドキュメント (参照リスト)

- `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` — Section 09 設計根拠
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` — DISPLAY_SPEC v1.0 (人数表示禁止等)
- `docs/audit_2026_05_13/pdf_visual_review.md` + `pdf_chart_fix_plan.md` — P2-4 punch list
- `docs/audit_2026_04_24/survey_data_activation_plan.md` — 案 R-A (賃貸データ取得設計)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md` — 賃貸データ前提条件
- `docs/NAVY_SECTION_09_DESIGN.md` — Section 09 詳細設計

---

## 8. 本日完了統計

- merge 済 PR: **3 件** (#3 #4 #5)
- 合計 commits: **11 件** (A1 #294 split 9 commit + fmt + clippy + Round 1-J/K + Section 09 + その後の整理)
- 新規行数: 約 +1500 行 (Section 09 1030 + 防御線 47 + 中立化 10 + その他)
- テスト追加: **+8 件** (Section 09)
- 1620 → 1628 PASS (-0 failed / 39 ignored 不変)
