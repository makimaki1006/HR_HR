# V2 → V1 バックポート可能機能

**作成日**: 2026-03-01

V2（ハローワーク版）で新規開発した機能のうち、V1（ジョブメドレー版）に
逆輸入できる機能をまとめる。

---

## バックポート可能

### 1. Layer B-1: TF-IDF キーワード抽出（MeCab + sklearn）

| 項目 | 内容 |
|------|------|
| **概要** | 求人原稿テキストをMeCabで形態素解析 → TF-IDFでキーワード抽出 |
| **V2スクリプト** | `hellowork_compute_layers.py` の `compute_layer_b()` |
| **V1への適用** | `compute_layer_b.py` を改修してジョブメドレーのテキストカラムに対応 |
| **依存パッケージ** | `mecab-python3`, `unidic-lite`, `scikit-learn` |
| **改修ポイント** | テキスト元カラム名の変更（V2: `job_description` → V1: `headline` or 類似） |
| **DB出力先** | `layer_b_keywords` テーブル（V1に同名テーブル追加） |
| **工数** | 小（カラム名マッピング変更のみ） |

**V2での成果**: 390キーワード行、職種×都道府県ごとにTOP10キーワード

### 2. Layer B-2: 共起分析（co-occurrence）

| 項目 | 内容 |
|------|------|
| **概要** | キーワード同士の共起頻度を計算 → ペアリスト生成 |
| **V2スクリプト** | `hellowork_compute_layers.py` の `compute_layer_b()` 内 |
| **V1への適用** | Layer B-1と同時にバックポート可能 |
| **DB出力先** | `layer_b_cooccurrence` テーブル |
| **工数** | 小（B-1と一体） |

### 3. Layer B-3: 原稿品質スコア

| 項目 | 内容 |
|------|------|
| **概要** | text_entropy, kanji_ratio, content_richness_score で原稿品質を地域別集計 |
| **V2スクリプト** | `hellowork_compute_layers.py` の `compute_layer_b()` 内 |
| **V1への適用** | V1のCSVにtext_entropy/kanji_ratioカラムがあれば即適用可能 |
| **前提** | V1のETL（job_medley_analyzer.py）にtext_entropy追加が必要 |
| **DB出力先** | `layer_b_text_quality` テーブル |
| **工数** | 中（V1 ETLにtext_entropyカラム追加が必要） |

### 4. Layer C: k-meansクラスタリング

| 項目 | 内容 |
|------|------|
| **概要** | 16次元特徴量でk-meansクラスタリング → クラスタプロファイル + 地域ヒートマップ |
| **V2スクリプト** | `hellowork_compute_layers.py` の `compute_layer_c()` |
| **V1への適用** | 特徴量カラムのマッピング変更が必要 |
| **DB出力先** | `layer_c_clusters`, `layer_c_cluster_profiles`, `layer_c_region_heatmap` |
| **工数** | 中（特徴量カラムの再定義が必要） |

### 5. ストップワード辞書

| 項目 | 内容 |
|------|------|
| **概要** | MeCab解析時の日本語ストップワード（助詞・助動詞・一般動詞等の除外リスト） |
| **現在の課題** | V2のB-1で「です」「など」が混入 → 除外リスト整備が必要 |
| **V1への適用** | ストップワード辞書を共通化すれば両方で利用可能 |
| **工数** | 小 |

---

## バックポート不可

| 機能 | 理由 |
|------|------|
| 6タブUI | データ構造・目的が異なる |
| 地域中心フィルタ | V1は職種中心が正しい |
| hellowork_etl.py | データソースが異なる |
| Dockerfile (1DB構成) | V1は3DB構成 |
| config.rs (HELLOWORK_DB_PATH) | V1は3つのDBパス |

---

## バックポート手順（推奨）

```
1. V1のcompute_layer_b.pyにMeCab対応を追加
   - テキストカラム: V1のjob_description相当を特定
   - ストップワード辞書: V2と共通化

2. pip install mecab-python3 unidic-lite（V1環境に）

3. Layer B再計算
   python compute_layer_b.py --db data/geocoded_postings.db

4. Rust側: analysis.rsのキーワードタブはV1にも存在するため変更不要

5. DB再圧縮+デプロイ
```
