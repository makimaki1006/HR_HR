"""
V2独自分析: Phase 5 予測・推定モデル 事前計算スクリプト
=====================================================
5-1: 充足困難度予測 (fulfillment_prediction) - 機械学習による欠員リスク予測
5-2: 地域間流動性推定 (mobility_estimate) - 重力モデルによる人材流動シミュレーション
5-3: 給与分位テーブル (shadow_wage) - 地域×産業×雇用形態の詳細給与分布

全指標は employment_type（正社員/パート/その他）でセグメント化
"""
import sqlite3
import math
import sys
import os
from collections import defaultdict

# --- 依存ライブラリのチェック ---
try:
    import lightgbm as lgb
    USE_LIGHTGBM = True
except ImportError:
    USE_LIGHTGBM = False

try:
    from sklearn.linear_model import LogisticRegression
    from sklearn.model_selection import StratifiedKFold
    from sklearn.metrics import roc_auc_score
    import numpy as np
    USE_SKLEARN = True
except ImportError:
    USE_SKLEARN = False

DB_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "data", "hellowork.db")

MIN_SAMPLE = 5   # 一般的な最小サンプルサイズ
MIN_SAMPLE_PRED = 10  # 予測モデル用の最小サンプルサイズ


def emp_group(et):
    """雇用形態をグループ化"""
    if et is None:
        return "その他"
    if "パート" in et:
        return "パート"
    if et == "正社員":
        return "正社員"
    return "その他"


def percentile(sorted_list, pct):
    """ソート済みリストからパーセンタイル値を計算"""
    if not sorted_list:
        return None
    k = (len(sorted_list) - 1) * (pct / 100.0)
    f = int(k)
    c = f + 1
    if c >= len(sorted_list):
        return sorted_list[-1]
    return sorted_list[f] + (k - f) * (sorted_list[c] - sorted_list[f])


def haversine(lat1, lon1, lat2, lon2):
    """2点間のハーバーサイン距離(km)を計算"""
    R = 6371
    dlat = math.radians(lat2 - lat1)
    dlon = math.radians(lon2 - lon1)
    a = (math.sin(dlat / 2) ** 2
         + math.cos(math.radians(lat1)) * math.cos(math.radians(lat2))
         * math.sin(dlon / 2) ** 2)
    return R * 2 * math.asin(math.sqrt(a))


# =====================================================================
# 5-1: 充足困難度予測
# =====================================================================
def compute_fulfillment_prediction(db):
    """5-1: 充足困難度予測
    recruitment_reason_code をラベルとした機械学習モデルで
    各施設の充足困難度を予測する。
    LightGBM優先、未インストール時はLogisticRegressionにフォールバック。
    """
    if not USE_SKLEARN and not USE_LIGHTGBM:
        print("5-1: sklearn / lightgbm が未インストールのためスキップ")
        print("  pip install scikit-learn (または lightgbm) でインストールしてください")
        return

    print("5-1: 充足困難度予測を計算中...")
    if USE_LIGHTGBM:
        print("  モデル: LightGBM")
    else:
        print("  モデル: LogisticRegression (LightGBM未インストール)")

    # --- 学習データ取得 ---
    rows = db.execute("""
        SELECT prefecture, municipality, facility_name, employment_type,
               salary_min, annual_holidays, bonus_months, employee_count,
               education_required, recruitment_reason_code
        FROM postings
        WHERE recruitment_reason_code IS NOT NULL
          AND salary_min > 0
          AND prefecture IS NOT NULL AND prefecture != ''
    """).fetchall()

    if len(rows) < MIN_SAMPLE_PRED:
        print(f"  警告: データが少なすぎます ({len(rows)} 件)。スキップします。")
        return

    # --- 教育レベルのエンコード ---
    edu_map = {"不問": 0, "高卒": 1, "専門": 2, "大卒": 3}

    def encode_education(val):
        if val is None:
            return 0
        for key, code in edu_map.items():
            if key in str(val):
                return code
        return 0

    def encode_emp_group(grp):
        if grp == "正社員":
            return 0
        if grp == "パート":
            return 1
        return 2

    # --- 特徴量とラベルの構築 ---
    features = []
    labels = []
    meta = []  # (prefecture, municipality, facility_name, emp_group)

    for (pref, muni, fac, et, smin, holidays, bonus,
         emp_count, edu, reason_code) in rows:
        grp = emp_group(et)
        label = 1 if reason_code == 1 else 0

        features.append([
            smin,
            holidays if holidays is not None else None,
            bonus if bonus is not None else None,
            emp_count if emp_count is not None else None,
            encode_education(edu),
            encode_emp_group(grp),
        ])
        labels.append(label)
        meta.append((pref, muni or "", fac or "", grp))

    X = np.array(features, dtype=np.float64)
    y = np.array(labels, dtype=np.int32)

    # --- 欠損値を中央値で埋める ---
    for col_idx in range(X.shape[1]):
        col = X[:, col_idx]
        mask = np.isnan(col)
        if mask.any():
            valid = col[~mask]
            median_val = np.median(valid) if len(valid) > 0 else 0.0
            X[mask, col_idx] = median_val

    print(f"  学習データ: {len(y)} 件 (欠員=1: {y.sum()}, その他=0: {len(y) - y.sum()})")

    # --- 5-fold CV でAUC評価 ---
    n_splits = 5
    skf = StratifiedKFold(n_splits=n_splits, shuffle=True, random_state=42)
    auc_scores = []

    for fold_idx, (train_idx, val_idx) in enumerate(skf.split(X, y)):
        X_train, X_val = X[train_idx], X[val_idx]
        y_train, y_val = y[train_idx], y[val_idx]

        if USE_LIGHTGBM:
            model = lgb.LGBMClassifier(
                n_estimators=200, max_depth=5, learning_rate=0.05,
                random_state=42, verbose=-1,
            )
            model.fit(X_train, y_train)
            y_prob = model.predict_proba(X_val)[:, 1]
        else:
            model = LogisticRegression(C=1.0, max_iter=500, random_state=42)
            model.fit(X_train, y_train)
            y_prob = model.predict_proba(X_val)[:, 1]

        # ラベルが1クラスしかない場合AUC計算不可
        if len(set(y_val)) < 2:
            continue
        auc = roc_auc_score(y_val, y_prob)
        auc_scores.append(auc)

    if auc_scores:
        mean_auc = sum(auc_scores) / len(auc_scores)
        print(f"  5-fold CV AUC: {mean_auc:.4f}")
        if mean_auc < 0.55:
            print("  警告: AUC < 0.55 のためモデル精度が低い可能性があります。結果は参考値です。")
    else:
        print("  警告: AUCを計算できませんでした（ラベル不均衡の可能性）")

    # --- 全データで再学習してスコア予測 ---
    if USE_LIGHTGBM:
        final_model = lgb.LGBMClassifier(
            n_estimators=200, max_depth=5, learning_rate=0.05,
            random_state=42, verbose=-1,
        )
    else:
        final_model = LogisticRegression(C=1.0, max_iter=500, random_state=42)

    final_model.fit(X, y)
    all_proba = final_model.predict_proba(X)[:, 1]

    # --- 施設単位で平均スコアを算出 ---
    facility_scores = defaultdict(list)  # (pref, muni, fac, grp) → [score, ...]
    for i, (pref, muni, fac, grp) in enumerate(meta):
        facility_scores[(pref, muni, fac, grp)].append(all_proba[i] * 100)

    def grade(score):
        """スコアからグレードを判定（低いほど充足しやすい）"""
        if score < 25:
            return "A"
        if score < 50:
            return "B"
        if score < 75:
            return "C"
        return "D"

    # --- v2_fulfillment_score テーブル作成 ---
    db.execute("DROP TABLE IF EXISTS v2_fulfillment_score")
    db.execute("""
        CREATE TABLE v2_fulfillment_score (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            facility_name TEXT NOT NULL,
            emp_group TEXT NOT NULL,
            score REAL NOT NULL,
            grade TEXT NOT NULL,
            PRIMARY KEY (prefecture, municipality, facility_name, emp_group)
        )
    """)

    score_rows = []
    for (pref, muni, fac, grp), scores in facility_scores.items():
        avg_score = sum(scores) / len(scores)
        score_rows.append((pref, muni, fac, grp, round(avg_score, 2), grade(avg_score)))

    db.executemany("""
        INSERT OR REPLACE INTO v2_fulfillment_score VALUES (?,?,?,?,?,?)
    """, score_rows)
    print(f"  → v2_fulfillment_score: {len(score_rows)} 行を挿入")

    # --- v2_fulfillment_summary テーブル作成（3レベル集計） ---
    db.execute("DROP TABLE IF EXISTS v2_fulfillment_summary")
    db.execute("""
        CREATE TABLE v2_fulfillment_summary (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            total_count INTEGER NOT NULL,
            avg_score REAL NOT NULL,
            grade_a_pct REAL,
            grade_b_pct REAL,
            grade_c_pct REAL,
            grade_d_pct REAL,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    # 3レベル集計: 都道府県×市区町村×emp_group / 都道府県×emp_group
    summary_data = defaultdict(list)  # (pref, muni, grp) → [score, ...]
    for (pref, muni, _fac, grp), scores in facility_scores.items():
        avg_s = sum(scores) / len(scores)
        # 市区町村レベル
        summary_data[(pref, muni, grp)].append(avg_s)
        # 都道府県レベル
        summary_data[(pref, "", grp)].append(avg_s)

    summary_rows = []
    for (pref, muni, grp), scores in summary_data.items():
        n = len(scores)
        if n < MIN_SAMPLE:
            continue
        avg = sum(scores) / n
        a_pct = sum(1 for s in scores if s < 25) / n * 100
        b_pct = sum(1 for s in scores if 25 <= s < 50) / n * 100
        c_pct = sum(1 for s in scores if 50 <= s < 75) / n * 100
        d_pct = sum(1 for s in scores if s >= 75) / n * 100
        summary_rows.append((
            pref, muni, grp, n,
            round(avg, 2), round(a_pct, 1), round(b_pct, 1),
            round(c_pct, 1), round(d_pct, 1),
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_fulfillment_summary VALUES (?,?,?,?,?,?,?,?,?)
    """, summary_rows)
    print(f"  → v2_fulfillment_summary: {len(summary_rows)} 行を挿入")

    # グレード分布を表示
    grade_dist = defaultdict(int)
    for row in score_rows:
        grade_dist[row[5]] += 1
    print(f"  グレード分布: {dict(sorted(grade_dist.items()))}")


# =====================================================================
# 5-2: 地域間流動性推定（重力モデル）
# =====================================================================
def compute_mobility_estimate(db):
    """5-2: 地域間流動性推定
    重力モデルで各市区町村間の人材流動ポテンシャルを推定する。
    gravity_score = (avg_salary * n_postings) / distance^1.5
    """
    print("5-2: 地域間流動性推定を計算中...")

    # --- ジオコードカバレッジの確認 ---
    total = db.execute("SELECT COUNT(*) FROM postings").fetchone()[0]
    geocoded = db.execute(
        "SELECT COUNT(*) FROM postings WHERE latitude > 0 AND longitude > 0"
    ).fetchone()[0]

    if total == 0:
        print("  警告: postingsテーブルが空です。スキップします。")
        return

    coverage = geocoded / total * 100
    print(f"  ジオコードカバレッジ: {geocoded}/{total} ({coverage:.1f}%)")

    if coverage < 50:
        print("  警告: ジオコードカバレッジが50%未満のためスキップします。")
        return

    # --- 市区町村ごとの重心・統計を算出 ---
    rows = db.execute("""
        SELECT prefecture, municipality, employment_type,
               salary_min, latitude, longitude
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND latitude > 0 AND longitude > 0
          AND salary_min > 0
    """).fetchall()

    # (pref, muni, grp) → {lats, lons, salaries}
    muni_data = defaultdict(lambda: {"lats": [], "lons": [], "salaries": []})

    for pref, muni, et, smin, lat, lon in rows:
        grp = emp_group(et)
        muni = muni or ""
        key = (pref, muni, grp)
        d = muni_data[key]
        d["lats"].append(lat)
        d["lons"].append(lon)
        d["salaries"].append(smin)

    # 重心と統計を計算
    centroids = {}  # (pref, muni, grp) → (lat, lon, avg_salary, n_postings)
    for key, d in muni_data.items():
        n = len(d["lats"])
        if n < MIN_SAMPLE:
            continue
        c_lat = sum(d["lats"]) / n
        c_lon = sum(d["lons"]) / n
        avg_sal = sum(d["salaries"]) / n
        centroids[key] = (c_lat, c_lon, avg_sal, n)

    print(f"  対象市区町村: {len(centroids)} グループ")

    if len(centroids) < 2:
        print("  警告: 市区町村グループが2未満のため計算できません。スキップします。")
        return

    # --- 重力モデルの計算 ---
    # 同一emp_groupの市区町村ペア間でのみ計算
    # 最適化: 緯度差 < 1.5度（約170km）のペアのみ
    attractiveness = defaultdict(float)  # key → 他市区町村から当該市区町村への重力合計
    outflow = defaultdict(float)         # key → 当該市区町村から他市区町村への重力合計
    top_dest = defaultdict(list)         # key → [(dest_name, score), ...]

    keys_by_grp = defaultdict(list)
    for key in centroids:
        keys_by_grp[key[2]].append(key)

    pair_count = 0
    for grp, keys in keys_by_grp.items():
        n_keys = len(keys)
        for i in range(n_keys):
            key_i = keys[i]
            lat_i, lon_i, sal_i, n_i = centroids[key_i]

            for j in range(n_keys):
                if i == j:
                    continue
                key_j = keys[j]
                lat_j, lon_j, sal_j, n_j = centroids[key_j]

                # 緯度差による事前フィルタ（約170km以内）
                if abs(lat_i - lat_j) >= 1.5:
                    continue

                dist = haversine(lat_i, lon_i, lat_j, lon_j)
                if dist < 1.0:
                    dist = 1.0  # ゼロ除算防止（最低1km）
                if dist > 100.0:
                    continue  # 100km超は対象外

                # i → j への重力スコア（jの吸引力）
                gravity = (sal_j * n_j) / (dist ** 1.5)

                attractiveness[key_j] += gravity
                outflow[key_i] += gravity
                top_dest[key_i].append((f"{key_j[0]}{key_j[1]}", gravity))
                pair_count += 1

    print(f"  計算ペア数: {pair_count}")

    # --- テーブル作成 ---
    db.execute("DROP TABLE IF EXISTS v2_mobility_estimate")
    db.execute("""
        CREATE TABLE v2_mobility_estimate (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            emp_group TEXT NOT NULL,
            local_postings INTEGER NOT NULL,
            local_avg_salary REAL,
            gravity_attractiveness REAL NOT NULL,
            gravity_outflow REAL NOT NULL,
            net_gravity REAL NOT NULL,
            top3_destinations TEXT,
            PRIMARY KEY (prefecture, municipality, emp_group)
        )
    """)

    insert_rows = []
    for key, (c_lat, c_lon, avg_sal, n_post) in centroids.items():
        pref, muni, grp = key
        attr = attractiveness.get(key, 0.0)
        out = outflow.get(key, 0.0)
        net = attr - out

        # 上位3流出先を取得
        dests = top_dest.get(key, [])
        # 同名のdestinationを集約
        dest_agg = defaultdict(float)
        for name, score in dests:
            dest_agg[name] += score
        sorted_dests = sorted(dest_agg.items(), key=lambda x: -x[1])[:3]
        top3_str = ",".join(f"{name}:{score:.0f}" for name, score in sorted_dests) if sorted_dests else None

        insert_rows.append((
            pref, muni, grp, n_post,
            round(avg_sal, 0),
            round(attr, 2), round(out, 2), round(net, 2),
            top3_str,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_mobility_estimate VALUES (?,?,?,?,?,?,?,?,?)
    """, insert_rows)
    print(f"  → v2_mobility_estimate: {len(insert_rows)} 行を挿入")

    # 純流入トップ5を表示
    sorted_by_net = sorted(insert_rows, key=lambda r: -r[7])[:5]
    print("  純流入トップ5:")
    for r in sorted_by_net:
        print(f"    {r[0]}{r[1]} ({r[2]}): net={r[7]:,.0f}, 求人数={r[3]}")


# =====================================================================
# 5-3: 給与分位テーブル
# =====================================================================
def compute_shadow_wage(db):
    """5-3: 給与分位テーブル
    地域×産業×雇用形態×給与種類ごとの詳細な給与分位分布を算出する。
    月給・時給・日給のフィルタ基準を分けて適用。
    """
    print("5-3: 給与分位テーブルを計算中...")

    db.execute("DROP TABLE IF EXISTS v2_shadow_wage")
    db.execute("""
        CREATE TABLE v2_shadow_wage (
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL DEFAULT '',
            industry_raw TEXT NOT NULL DEFAULT '',
            emp_group TEXT NOT NULL,
            salary_type TEXT NOT NULL DEFAULT '月給',
            total_count INTEGER NOT NULL,
            p10 REAL, p25 REAL, p50 REAL, p75 REAL, p90 REAL,
            mean REAL, stddev REAL, iqr REAL,
            PRIMARY KEY (prefecture, municipality, industry_raw, emp_group, salary_type)
        )
    """)

    # 給与種類別の最低フィルタ基準
    salary_floor = {
        "月給": 50000,
        "時給": 500,
        "日給": 5000,
    }

    rows = db.execute("""
        SELECT prefecture, municipality, industry_raw, employment_type,
               salary_type, salary_min
        FROM postings
        WHERE prefecture IS NOT NULL AND prefecture != ''
          AND salary_min > 0
    """).fetchall()

    # 3レベル集計: (pref, muni, industry, grp, stype) → [salary_min, ...]
    data = defaultdict(list)

    for pref, muni, industry, et, stype, smin in rows:
        grp = emp_group(et)
        muni = muni or ""
        industry = industry or ""
        stype = stype or "その他"

        # 給与種類別の最低フィルタ
        floor = salary_floor.get(stype, 0)
        if smin < floor:
            continue

        # 3レベル集計
        for key in [
            (pref, muni, industry, grp, stype),  # 詳細
            (pref, muni, "", grp, stype),          # 市区町村
            (pref, "", "", grp, stype),            # 都道府県
        ]:
            data[key].append(smin)

    insert_rows = []
    for (pref, muni, industry, grp, stype), vals in data.items():
        n = len(vals)
        if n < MIN_SAMPLE_PRED:
            continue

        sorted_vals = sorted(vals)
        p10 = percentile(sorted_vals, 10)
        p25 = percentile(sorted_vals, 25)
        p50 = percentile(sorted_vals, 50)
        p75 = percentile(sorted_vals, 75)
        p90 = percentile(sorted_vals, 90)

        mean_val = sum(vals) / n

        # 母標準偏差（Nで割る）
        variance = sum((v - mean_val) ** 2 for v in vals) / n
        stddev = math.sqrt(variance)

        iqr = (p75 - p25) if p75 is not None and p25 is not None else None

        insert_rows.append((
            pref, muni, industry, grp, stype, n,
            round(p10, 0) if p10 is not None else None,
            round(p25, 0) if p25 is not None else None,
            round(p50, 0) if p50 is not None else None,
            round(p75, 0) if p75 is not None else None,
            round(p90, 0) if p90 is not None else None,
            round(mean_val, 0),
            round(stddev, 0),
            round(iqr, 0) if iqr is not None else None,
        ))

    db.executemany("""
        INSERT OR REPLACE INTO v2_shadow_wage VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
    """, insert_rows)
    print(f"  → v2_shadow_wage: {len(insert_rows)} 行を挿入")

    # 給与種類別の行数を表示
    type_dist = defaultdict(int)
    for row in insert_rows:
        type_dist[row[4]] += 1
    print(f"  給与種類別: {dict(sorted(type_dist.items()))}")


# =====================================================================
# 検証
# =====================================================================
def verify(db):
    """検証: テーブル行数と東京都サンプルを確認"""
    print("\n=== 検証 ===")
    tables = [
        "v2_fulfillment_score", "v2_fulfillment_summary",
        "v2_mobility_estimate", "v2_shadow_wage",
    ]
    for table in tables:
        try:
            count = db.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
            print(f"  {table}: {count} 行")
        except sqlite3.OperationalError:
            print(f"  {table}: テーブルなし（スキップされた可能性）")

    # --- 5-1 検証: 東京都正社員の充足困難度 ---
    try:
        row = db.execute("""
            SELECT avg_score, grade_a_pct, grade_b_pct, grade_c_pct, grade_d_pct, total_count
            FROM v2_fulfillment_summary
            WHERE prefecture='東京都' AND municipality='' AND emp_group='正社員'
        """).fetchone()
        if row:
            print(f"\n  東京都 正社員 充足困難度:")
            print(f"    平均スコア: {row[0]:.1f}, 施設数: {row[5]}")
            print(f"    A(容易): {row[1]:.1f}%, B: {row[2]:.1f}%, C: {row[3]:.1f}%, D(困難): {row[4]:.1f}%")
    except sqlite3.OperationalError:
        pass

    # --- 5-2 検証: 東京都正社員の流動性 ---
    try:
        row = db.execute("""
            SELECT local_postings, local_avg_salary, gravity_attractiveness,
                   gravity_outflow, net_gravity, top3_destinations
            FROM v2_mobility_estimate
            WHERE prefecture='東京都' AND municipality='' AND emp_group='正社員'
        """).fetchone()
        if row:
            print(f"\n  東京都 正社員 流動性:")
            print(f"    求人数: {row[0]}, 平均給与: {row[1]:,.0f}円")
            print(f"    吸引力: {row[2]:,.0f}, 流出: {row[3]:,.0f}, 純流入: {row[4]:,.0f}")
            if row[5]:
                print(f"    上位流出先: {row[5]}")
        else:
            # 市区町村レベルで確認
            sample = db.execute("""
                SELECT prefecture, municipality, emp_group, local_postings,
                       net_gravity, top3_destinations
                FROM v2_mobility_estimate
                WHERE prefecture='東京都' AND emp_group='正社員'
                ORDER BY net_gravity DESC LIMIT 3
            """).fetchall()
            if sample:
                print(f"\n  東京都 正社員 流動性トップ3:")
                for r in sample:
                    print(f"    {r[1]}: 求人数={r[3]}, 純流入={r[4]:,.0f}")
    except sqlite3.OperationalError:
        pass

    # --- 5-3 検証: 東京都正社員月給の分位 ---
    try:
        row = db.execute("""
            SELECT total_count, p10, p25, p50, p75, p90, mean, stddev, iqr
            FROM v2_shadow_wage
            WHERE prefecture='東京都' AND municipality='' AND industry_raw=''
              AND emp_group='正社員' AND salary_type='月給'
        """).fetchone()
        if row:
            print(f"\n  東京都 正社員 月給分布:")
            print(f"    件数: {row[0]}")
            print(f"    p10: {row[1]:,.0f}, p25: {row[2]:,.0f}, p50(中央値): {row[3]:,.0f}")
            print(f"    p75: {row[4]:,.0f}, p90: {row[5]:,.0f}")
            print(f"    平均: {row[6]:,.0f}, 標準偏差: {row[7]:,.0f}, IQR: {row[8]:,.0f}")
    except sqlite3.OperationalError:
        pass


# =====================================================================
# メイン
# =====================================================================
def main():
    if not os.path.exists(DB_PATH):
        print(f"Error: DB not found at {DB_PATH}")
        sys.exit(1)

    print(f"DB: {DB_PATH}")
    print(f"LightGBM: {'有効' if USE_LIGHTGBM else '無効'}")
    print(f"scikit-learn: {'有効' if USE_SKLEARN else '無効'}")
    print()

    db = sqlite3.connect(DB_PATH)
    db.execute("PRAGMA journal_mode=WAL")

    try:
        compute_fulfillment_prediction(db)
        db.commit()

        compute_mobility_estimate(db)
        db.commit()

        compute_shadow_wage(db)
        db.commit()

        verify(db)
        print("\nPhase 5 予測・推定モデル 完了")
    except Exception as e:
        db.rollback()
        print(f"Error: {e}")
        raise
    finally:
        db.close()


if __name__ == "__main__":
    main()
