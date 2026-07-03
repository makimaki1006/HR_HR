//! navy_report 横断 helper モジュール (Commit 1 / γ Common Team, 2026-05-29 抽出)。
//!
//! 元 `navy_report.rs` 内で複数 Section から参照されていた以下を集約:
//! - SKEW 判定 (compute_skew_severity / severity_label / SKEW_*_THRESHOLD_PCT)
//! - 給与分布統計 (DistStats / compute_distribution_stats / format_mm)
//! - SVG 描画 (build_navy_histogram_svg / build_navy_salary_scatter_svg /
//!   build_salary_scatter_summary)
//! - フォーマッタ (fmt_ratio / fmt_pct / fmt_pct_from_ratio / format_mm)
//! - 数値防衛 (safe_pct / safe_pct_like)
//! - HTML helper (leak / push_page_head / push_region_scope_banner / push_kpi)
//!
//! 設計方針:
//! - すべて `pub(super)` で navy_report mod 内のみに公開 (外部 API 不変)。
//! - 内容は元 navy_report.rs の物理コピー (挙動変更なし)。
//! - テスト (`navy_report::tests`) は mod.rs 側の `pub(super) use common::*;`
//!   再エクスポートにより `use super::*;` で従来通り参照可能。

#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};

// ============================================================
// Ext-3 (2026-05-28): SKEW 判定の閾値定数化
// ------------------------------------------------------------
// Round 2 P2-3: 70.0/85.0 が `compute_skew_severity` 本体にハードコードされており、
//   A/B test や閾値チューニング時に「定数 → テスト → docstring → ガイドライン」の
//   4 箇所を同期更新する必要があった (分散して保守事故の温床)。
// 修正: 単一の定数定義に集約し、関数本体・境界値テストの双方がここを参照する。
// 不変条件:
//   - `SKEW_NEU_THRESHOLD_PCT < SKEW_WARN_THRESHOLD_PCT` (定数の順序保証)
//   - 比較は **strict greater** (`>`) で統一: 70.0% ちょうどは "pos"、
//     85.0% ちょうどは "neu" (境界一致は下位 severity 側)
// ============================================================

/// SKEW 判定: WARN しきい値 (この値を **超える** ＝ "warn")。
pub(super) const SKEW_WARN_THRESHOLD_PCT: f64 = 85.0;
/// SKEW 判定: NEU しきい値 (この値を **超える** ＝ "neu", 超えなければ "pos")。
pub(super) const SKEW_NEU_THRESHOLD_PCT: f64 = 70.0;

/// 分類分布 (`(name, count)` 配列) の偏り度を判定して severity tag と説明文を返す。
///
/// # Severity (navy_report 内 4 値タグ準拠: pos / warn / neg / neu)
/// - `max_share > SKEW_WARN_THRESHOLD_PCT` → `"warn"` (顕著な偏り、サンプル代表性 低い)
/// - `max_share > SKEW_NEU_THRESHOLD_PCT`  → `"neu"`  (偏りあり、データ代表性に注意)
/// - 上記以下                              → `"pos"`  (バランス良好)
/// - `counts.is_empty()` または `total <= 0` → `"neu"` (「{label}データなし」)
///
/// # 引数
/// - `counts`: `(分類名, 件数)` のスライス。順序不問 (内部で `max_by_key` で
///   top を抽出する)。
/// - `label`: 主語ラベル。例: `"産業大分類"` / `"職種"`。
///   生成文字列の先頭 (`{label}偏り 顕著 ...`) に挿入される。
///
/// # 戻り値
/// `(severity_tag, message)` の組。severity_tag は `severity_label` で
/// "POS"/"WARN"/"NEU" バッジに変換される。message には HTML エスケープが
/// 必要 (呼出側で `escape_html` を通すこと)。
///
/// # silent fallback 監査
/// `_ => ...` 的な暗黙分岐は無く、`empty` / `total<=0` / 3 段階閾値 (>85 / >70 /
/// それ以下) の **4 条件全て** を明示的にカバーする。閾値境界は `>` (strict
/// greater) で統一: 70.0% ちょうどは "pos"、85.0% ちょうどは "neu"。
///
/// # 不変条件 (テストで検証)
/// - `total > 0` のときのみ `max_share = top_count / total * 100.0` を計算
///   (zero-div 不可)
/// - `max_share ∈ [0.0, 100.0]` (counts.iter().max() の戻り値 ≤ total)
/// - `counts.is_empty()` または `total <= 0` の早期 return で NEU 固定
pub(super) fn compute_skew_severity(
    counts: &[(String, i64)],
    label: &str,
) -> (&'static str, String) {
    if counts.is_empty() {
        return ("neu", format!("{}データなし", label));
    }
    let total: i64 = counts.iter().map(|(_, c)| *c).sum();
    if total <= 0 {
        return ("neu", format!("{}データなし", label));
    }
    // unwrap: counts.is_empty() は上で除外済み → max_by_key は必ず Some
    let (top_label, top_count) = counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .expect("counts.is_empty() guarded above");
    // top_count >= 0 ∧ total > 0 ∧ top_count <= total ⇒ max_share ∈ [0, 100]
    let max_share = (*top_count as f64 / total as f64) * 100.0;
    // Ext-3 (2026-05-28): ハードコード値 (70.0 / 85.0) を `SKEW_*_THRESHOLD_PCT` 定数参照に変更。
    if max_share > SKEW_WARN_THRESHOLD_PCT {
        (
            "warn",
            format!(
                "{}偏り 顕著 (上位「{}」{:.1}%、サンプル代表性 低い)",
                label, top_label, max_share
            ),
        )
    } else if max_share > SKEW_NEU_THRESHOLD_PCT {
        (
            "neu",
            format!(
                "{}偏りあり (上位「{}」{:.1}%、データ代表性に注意)",
                label, top_label, max_share
            ),
        )
    } else {
        (
            "pos",
            format!(
                "{}上位カテゴリへの偏りは限定的 (最大構成「{}」{:.1}%)",
                label, top_label, max_share
            ),
        )
    }
}

/// severity tag → 表示用 3 文字英略語ラベル
///
/// 2026-05-21 docstring 追加: 本関数は他の i18n / label 関数と異なり、出力自体が
/// **意図的に英語短縮形** ("POS"/"WARN"/"NEG"/"NEU") である (採用コンサル
/// レポートのバッジ表示で短く統一するため、navy_report.rs 全体の意匠決定)。
/// `_ => "NEU"` は silent fallback ではなく明示的な「中立」マッピング。
///
/// 新規 severity 種別 (例: "critical") を追加する場合は以下も同時に確認:
/// - `build_business_findings` / `build_geo_findings` 等の sev tag 生成側
/// - 配色 CSS (`.tag-pos`, `.tag-warn`, `.tag-neg`, `.tag-neu`)
/// 上記を更新せず本 match だけ広げると silent fallback と同じパターンに陥る。
pub(super) fn severity_label(tag: &str) -> &'static str {
    match tag {
        "pos" => "POS",
        "warn" => "WARN",
        "neg" => "NEG",
        _ => "NEU",
    }
}

// ============================================================
// 給与分布統計 (DistStats / compute_distribution_stats / format_mm)
// ============================================================

// 分布統計 (月給換算済の i64 円 を入力。万円単位での出力用)
pub(super) struct DistStats {
    pub(super) n: usize,
    pub(super) p25: i64,
    pub(super) median: i64,
    pub(super) p75: i64,
    pub(super) p90: i64,
    pub(super) mean: i64,
    pub(super) min: i64,
    pub(super) max: i64,
    pub(super) mode_bin_yen: i64, // 10000 円刻み bin の代表値
    pub(super) bins: Vec<usize>,  // ヒストグラム頻度
    pub(super) bin_step: i64,     // bin 幅 (円)
    pub(super) bin_start: i64,    // bin 0 の下端 (円)
}

/// Phase 2-A (2026-05-29): `bin_step` 引数化。
///
/// 旧シグネチャ: `compute_distribution_stats(values)` (bin_step は 10_000 固定)。
/// 新シグネチャ: `compute_distribution_stats(values, bin_step)` で時給モード対応。
///
/// 推奨値:
/// - 月給モード: `bin_step = 10_000` (= 1 万円刻み)
/// - 時給モード: `bin_step = 50` (= 50 円/時 刻み)
///
/// `bin_step <= 0` は不正値として `None` を返す (silent fallback 防止)。
pub(super) fn compute_distribution_stats(values: &[i64], bin_step: i64) -> Option<DistStats> {
    if values.is_empty() || bin_step <= 0 {
        return None;
    }
    let mut v: Vec<i64> = values.iter().copied().filter(|x| *x > 0).collect();
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    let pct = |p: f64| -> i64 {
        let idx = ((n as f64 - 1.0) * p).round() as usize;
        v[idx.min(n - 1)]
    };
    let p25 = pct(0.25);
    let median = pct(0.5);
    let p75 = pct(0.75);
    let p90 = pct(0.90);
    let min = v[0];
    let max = v[n - 1];
    let sum: i64 = v.iter().sum();
    let mean = sum / n as i64;

    // ヒストグラム: bin_step 刻みで P95 まで (それ以上は overflow バケット)
    let bin_start: i64 = (min / bin_step) * bin_step;
    let p95 = pct(0.95);
    let upper = (p95 / bin_step + 1) * bin_step;
    let n_bins = (((upper - bin_start) / bin_step).max(1) as usize) + 1; // 最後はoverflow
    let mut bins = vec![0usize; n_bins];
    for &x in &v {
        let idx = ((x - bin_start) / bin_step) as i64;
        let idx_u = idx.clamp(0, (n_bins - 1) as i64) as usize;
        bins[idx_u] += 1;
    }
    // mode = 最頻 bin
    let (mode_idx, _) = bins
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| **c)
        .unwrap_or((0, &0));
    let mode_bin_yen = bin_start + mode_idx as i64 * bin_step + bin_step / 2;

    Some(DistStats {
        n,
        p25,
        median,
        p75,
        p90,
        mean,
        min,
        max,
        mode_bin_yen,
        bins,
        bin_step,
        bin_start,
    })
}

pub(super) fn format_mm(yen: i64) -> String {
    format!("{:.1}", yen as f64 / 10000.0)
}

/// Phase 2-A (2026-05-29): `unit_label` / `bin_step` 引数化。
///
/// - `unit_label`: x 軸の単位表記 (例: `"万円"` (月給) / `"円/時"` (時給))
/// - `bin_step`: 表示時の bin 1 つあたりの値域 (円)。x 軸ラベル換算に使用。
///   - 月給モード: `bin_step = 10_000` (= 1 万円刻み)、軸ラベルは「万円」単位で値/10000 表示
///   - 時給モード: `bin_step = 50` (= 50 円/時 刻み)、軸ラベルは値そのまま表示 (円/時)
///
/// 縦線ラベル (P50/平均/最頻) は単位非依存のテキストだが、x 軸ラベルだけ unit_label に応じて切替。
///
/// navy ヒストグラム SVG (固定 720×280 / 罫線 var(--rule) / バー var(--ink-soft))
pub(super) fn build_navy_histogram_svg(
    _values: &[i64],
    s: &DistStats,
    unit_label: &str,
    _bin_step: i64,
) -> String {
    let w: f64 = 720.0;
    let h: f64 = 280.0;
    let pad_l = 56.0;
    let pad_r = 16.0;
    // 2026-05-18: pad_t を 16 → 36 に拡大、平均/中央値/最頻 ラベルを y-stagger で
    //   重ねないため (ユーザー報告: 「項目の高さが全て同じでかぶると見れない」)
    let pad_t = 36.0;
    let pad_b = 44.0;
    let inner_w = w - pad_l - pad_r;
    let inner_h = h - pad_t - pad_b;
    let n_bins = s.bins.len();
    let max_count = *s.bins.iter().max().unwrap_or(&1).max(&1) as f64;
    let bw = inner_w / n_bins as f64;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"給与ヒストグラム\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64
    ));
    // y 軸グリッド + ラベル (5 段)
    for i in 0..=5 {
        let y = pad_t + inner_h * i as f64 / 5.0;
        let count = (max_count * (5 - i) as f64 / 5.0).round() as i64;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ECE7DA\" stroke-width=\"0.5\"/>\n",
            pad_l,
            y,
            w - pad_r,
            y
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"end\">{}</text>\n",
            pad_l - 6.0,
            y + 3.0,
            count
        ));
    }
    // bars
    for (i, c) in s.bins.iter().enumerate() {
        let bh = (*c as f64 / max_count) * inner_h;
        let bx = pad_l + i as f64 * bw;
        let by = pad_t + inner_h - bh;
        svg.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#1F2D4D\"/>\n",
            bx + 0.5,
            by,
            (bw - 1.0).max(1.0),
            bh
        ));
    }
    // x 軸ラベル: bin の代表値 (~6 ラベル)
    //   月給 (unit_label="万円"): 円 → 万円換算 (値/10000) で表示
    //   時給 (unit_label="円/時"): 円のまま表示
    let label_step = (n_bins / 6).max(1);
    // Phase 2-A (2026-05-29): unit_label に応じて x 軸ラベル換算式を切替
    let is_man_unit = unit_label.contains("万円");
    let fmt_x_label = |yen: i64, is_overflow: bool| -> String {
        let v_disp: f64 = if is_man_unit {
            yen as f64 / 10000.0
        } else {
            yen as f64
        };
        if is_overflow {
            format!("{}+", v_disp)
        } else {
            format!("{}", v_disp)
        }
    };
    for (i, _c) in s.bins.iter().enumerate() {
        if i % label_step == 0 || i == n_bins - 1 {
            let cx = pad_l + (i as f64 + 0.5) * bw;
            let yen = s.bin_start + i as i64 * s.bin_step;
            let is_overflow = i == n_bins - 1 && n_bins > 1;
            let label = fmt_x_label(yen, is_overflow);
            svg.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
                cx,
                h - pad_b + 14.0,
                label
            ));
        }
    }
    // x 軸タイトル: 月給→「月給 (万円)」、時給→「時給 (円/時)」
    let axis_title = if is_man_unit {
        "月給 (万円)".to_string()
    } else {
        format!("時給 ({})", unit_label)
    };
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}</text>\n",
        w / 2.0,
        h - 6.0,
        axis_title
    ));
    // 中央値 (緑), 平均 (gold), 最頻 (灰) 縦線
    let x_of = |yen: i64| -> f64 {
        let bin_idx = ((yen - s.bin_start) as f64 / s.bin_step as f64).max(0.0);
        pad_l + (bin_idx + 0.5) * bw
    };
    let lines = [
        (x_of(s.median), "#1F6B43", "P50"),
        (x_of(s.mean), "#C9A24B", "平均"),
        (x_of(s.mode_bin_yen), "#9CA0AB", "最頻"),
    ];
    // 2026-05-18: ラベル y を index で stagger (近接時の重なりで「どれか見えない」を解消)
    //   idx 0 (P50): y = 8   (一番上)
    //   idx 1 (平均): y = 20  (中)
    //   idx 2 (最頻): y = 32  (下、線の真上に最も近い)
    for (idx, (x, color, lbl)) in lines.iter().enumerate() {
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"1.5\" stroke-dasharray=\"3 2\"/>\n",
            x, pad_t, x, pad_t + inner_h, color
        ));
        let label_y = 8.0 + (idx as f64) * 12.0;
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"{}\" text-anchor=\"middle\" font-weight=\"700\">{}</text>\n",
            x, label_y, color, lbl
        ));
    }
    svg.push_str("</svg>\n");
    svg
}

// ============================================================
// フォーマッタ (Option<f64> → 表示文字列)
// ============================================================

pub(super) fn fmt_ratio(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.2}", x),
        None => "—".to_string(),
    }
}
pub(super) fn fmt_pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.1}%", x),
        None => "—".to_string(),
    }
}
pub(super) fn fmt_pct_from_ratio(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.1}", x * 100.0),
        None => "—".to_string(),
    }
}

// leak helper: format! の戻り String を &'static に変えるためのトリック。
// build_navy_tightness_gauges 内の (&str, ..., &str) ベクタ要素が
// 一時的に str を借りる用途。本関数は短時間のみ使う(関数内のみ参照)ので
// メモリリークは無視可能 (実利用上、Section 04 を 1 回しか呼ばないため
// 文字列の総量は最大十数バイト×4 件 = 100 バイト未満)。
pub(super) fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// ============================================================
// 共通: page-head / kpi cell
// ============================================================

pub(super) fn push_page_head(html: &mut String, section_code: &str, title: &str, sub: &str) {
    html.push_str(&format!(
        "<div class=\"page-head\">\
         <div class=\"ph-sec\">{}</div>\
         <div class=\"ph-title\">{}</div>\
         <div class=\"ph-sub\">{}</div>\
         <div class=\"ph-rule\" aria-hidden=\"true\"></div>\
         </div>\n",
        escape_html(section_code),
        escape_html(title),
        escape_html(sub),
    ));
}

/// 2026-05-22: 各 Section 冒頭で集計範囲を明示する共通 banner。
///
/// 「総人口 0 名」「高校 0」のような表示でユーザーが
/// 「これは都道府県単位か市区町村単位か」と困惑する UX 課題への対応。
/// target_region (例: "長崎県 東彼杵町") を判定して 3 種類の scope label を出力。
///
/// 適用先: Section 02 / 04 / 05 / 06 / 07 (集計データを含む全 Section)。
/// Section 03 (CSV 統計) と 08 (注記) は適用外 (集計単位の概念が異なる)。
pub(super) fn push_region_scope_banner(html: &mut String, target_region: &str) {
    let scope_label = if target_region == "全国" {
        "全国単位 (47 都道府県集計)"
    } else if target_region.contains(' ') {
        "市区町村単位 (該当市区町村のみ)"
    } else {
        "都道府県単位 (該当都道府県集計)"
    };
    html.push_str(&format!(
        "<div class=\"region-scope-banner\" style=\"margin:4mm 0;padding:6px 12px;background:#fef3c7;border-left:4px solid #f59e0b;border-radius:3px;font-size:10pt;\">\
         📍 集計範囲: <strong>{}</strong> ({})\
         </div>\n",
        escape_html(target_region),
        scope_label
    ));
}

/// Finding #7 (2026-07-01): section_07_5 / section_07_6 のローカル `push_kpi_card` を統一。
///
/// 旧: 各 section に `push_kpi_card` がローカル定義、出力クラスが `.kpi-card` (旧グローバル)。
/// 新: `common::push_kpi_card_simple` を呼び出し、クラスを `.kpi` に統一して
///   navy theme (Roboto Mono 22pt 等) が適用される構造を維持する。
///
/// 内部構造: `.kpi > .kpi-label / .kpi-value / .kpi-foot`
pub(super) fn push_kpi_card_simple(html: &mut String, label: &str, value: &str, foot: &str) {
    html.push_str(&format!(
        "<div class=\"kpi\">\
         <div class=\"kpi-label\">{}</div>\
         <div class=\"kpi-value\">{}</div>\
         <div class=\"kpi-foot\">{}</div>\
         </div>\n",
        escape_html(label),
        escape_html(value),
        escape_html(foot),
    ));
}

pub(super) fn push_kpi(
    html: &mut String,
    label: &str,
    value: &str,
    unit: &str,
    dot: &str,
    foot: &str,
    emphasis: bool,
) {
    let cls = if emphasis { "kpi kpi-emphasis" } else { "kpi" };
    html.push_str(&format!(
        "<div class=\"{cls}\">\
         <div class=\"kpi-label\">{label}</div>\
         <div class=\"kpi-value\">{value}<span class=\"kpi-unit\">{unit}</span></div>\
         <div class=\"kpi-foot\"><span class=\"dot {dot}\"></span>{foot}</div>\
         </div>\n",
        cls = cls,
        label = escape_html(label),
        value = escape_html(value),
        unit = escape_html(unit),
        dot = dot,
        foot = foot,
    ));
}

// ============================================================
// 給与散布図 SVG + サマリ + 数値防衛ヘルパ
// ============================================================

/// 図 3-6 給与レンジ 散布図 SVG (P2-1, 2026-05-28)。
///
/// 各点 1 求人で (下限給与, 上限給与) を打点し、対角線 (下限=上限) を金破線で重ねる。
/// 対角線から右上方向 (上方向) に離れるほどレンジが広い (歩合・等級制傾向)。
///
/// スタイル方針 (`build_navy_pyramid_svg` 踏襲):
/// - 配色: 散布点 = `#1F2D4D` (navy ink-soft), 対角線 = `#C9A24B` (gold)
/// - 散布点 opacity 0.4 で重なり可視化 (要件指定)
/// - 軸 / グリッド色 = `#D8D2C4` (rule-soft), ラベル色 = `#6A6E7A`
/// - 背景 = `var(--paper-pure)`、枠 = `1px solid var(--rule-soft)`
///
/// レンジ:
/// - 月給モード (`is_hourly=false`): X / Y 軸とも 15-60 万円固定
/// - 時給モード (`is_hourly=true`):  X / Y 軸とも 800-2500 円/時固定
///   - 範囲外データはクランプ (打点位置のみ端に寄る)。
///   - n 自体は配列長そのまま使うため caption と整合する。
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数化。
///   呼出側は agg.is_hourly を渡す。月給モードは旧動作と完全互換 (軸 15-60 万円固定)。
pub(super) fn build_navy_salary_scatter_svg(pairs: &[(f64, f64)], is_hourly: bool) -> String {
    if pairs.is_empty() {
        return String::new();
    }

    let w: f64 = 720.0;
    let h: f64 = 360.0;
    let margin_left: f64 = 56.0; // Y 軸ラベル列幅
    let margin_right: f64 = 16.0;
    let margin_top: f64 = 24.0;
    let margin_bottom: f64 = 36.0; // X 軸ラベル + タイトル下余白

    let plot_w: f64 = w - margin_left - margin_right;
    let plot_h: f64 = h - margin_top - margin_bottom;

    // Phase 2-A (2026-05-29): is_hourly に応じて軸スケールを切替
    //   - 月給: 15-60 万円固定 (旧動作維持)
    //   - 時給: 800-2500 円/時固定 (パート/アルバイトの一般的レンジを網羅)
    let (x_min_disp, x_max_disp, axis_unit_label, axis_tick_labels): (f64, f64, &str, &[i32]) =
        if is_hourly {
            (800.0, 2500.0, "円/時", &[800, 1200, 1600, 2000, 2400][..])
        } else {
            (15.0, 60.0, "万円", &[15, 25, 35, 45, 55][..])
        };
    let y_min_disp = x_min_disp;
    let y_max_disp = x_max_disp;

    // 円 → 表示単位 変換 (月給=万円換算 / 時給=円のまま)
    let to_disp = |yen: f64| if is_hourly { yen } else { yen / 10_000.0 };

    // 表示単位 → SVG x 座標 (左端 = margin_left)
    let x_of = |x_disp: f64| -> f64 {
        let clamped = x_disp.clamp(x_min_disp, x_max_disp);
        margin_left + (clamped - x_min_disp) / (x_max_disp - x_min_disp) * plot_w
    };
    // 表示単位 → SVG y 座標 (上が大きい値、Y 軸反転)
    let y_of = |y_disp: f64| -> f64 {
        let clamped = y_disp.clamp(y_min_disp, y_max_disp);
        margin_top + plot_h - (clamped - y_min_disp) / (y_max_disp - y_min_disp) * plot_h
    };

    let mut svg = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" \
         role=\"img\" aria-label=\"給与レンジ 散布図 (下限給与 × 上限給与)\" \
         style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n\
         <title>給与レンジ 散布図 (下限給与 × 上限給与)</title>\n",
        w = w as i64,
        h = h as i64
    );
    // R2-P1-3 (ultrathink Round 2, 2026-05-28): a11y のため SVG 直後に <title> を挿入。

    // タイトル
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"16\" font-size=\"11\" fill=\"#0B1E3F\" font-weight=\"700\">\
         (散布図) 下限給与 × 上限給与</text>\n",
        margin_left
    ));

    // プロットエリア枠 (薄い罫線)
    svg.push_str(&format!(
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" \
         fill=\"none\" stroke=\"#D8D2C4\" stroke-width=\"0.5\"/>\n",
        margin_left, margin_top, plot_w, plot_h
    ));

    // X 軸目盛り + ラベル
    for tick in axis_tick_labels {
        let x = x_of(*tick as f64);
        // 目盛り線 (プロット内に薄い縦線)
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#E8E2D2\" stroke-width=\"0.5\"/>\n",
            x, margin_top, x, margin_top + plot_h
        ));
        // ラベル (プロット下)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"middle\">{}{}</text>\n",
            x, margin_top + plot_h + 14.0, tick, axis_unit_label
        ));
    }

    // Y 軸目盛り + ラベル
    for tick in axis_tick_labels {
        let y = y_of(*tick as f64);
        // 目盛り線
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#E8E2D2\" stroke-width=\"0.5\"/>\n",
            margin_left, y, margin_left + plot_w, y
        ));
        // ラベル (プロット左)
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\" text-anchor=\"end\">{}{}</text>\n",
            margin_left - 4.0, y + 3.0, tick, axis_unit_label
        ));
    }

    // 軸ラベル (X = 下限給与、Y = 上限給与) — Phase 2-A: 単位を可変
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\" font-weight=\"600\">\
         下限給与 ({})</text>\n",
        margin_left + plot_w / 2.0,
        h - 4.0,
        axis_unit_label
    ));
    // Y ラベルは縦書き (回転)
    svg.push_str(&format!(
        "<text x=\"12\" y=\"{:.1}\" font-size=\"10\" fill=\"#6A6E7A\" text-anchor=\"middle\" font-weight=\"600\" \
         transform=\"rotate(-90 12 {:.1})\">上限給与 ({})</text>\n",
        margin_top + plot_h / 2.0,
        margin_top + plot_h / 2.0,
        axis_unit_label
    ));

    // 対角線 (下限=上限ライン): 金破線
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
         stroke=\"#C9A24B\" stroke-width=\"1.2\" stroke-dasharray=\"4,3\" opacity=\"0.7\"/>\n",
        x_of(x_min_disp),
        y_of(y_min_disp),
        x_of(x_max_disp),
        y_of(y_max_disp),
    ));

    // 散布点 (各 1 求人、半径 2.5px、navy ink-soft、opacity 0.4)
    for (lo_yen, hi_yen) in pairs {
        let lo_disp = to_disp(*lo_yen);
        let hi_disp = to_disp(*hi_yen);
        let cx = x_of(lo_disp);
        let cy = y_of(hi_disp);
        svg.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"#1F2D4D\" opacity=\"0.4\"/>\n",
            cx, cy
        ));
    }

    // 凡例 (右上、対角線の説明)
    let legend_x: f64 = margin_left + plot_w - 140.0;
    let legend_y: f64 = margin_top + 4.0;
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" \
         stroke=\"#C9A24B\" stroke-width=\"1.2\" stroke-dasharray=\"4,3\"/>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9\" fill=\"#6A6E7A\">下限=上限ライン</text>\n",
        legend_x,
        legend_y + 6.0,
        legend_x + 24.0,
        legend_y + 6.0,
        legend_x + 28.0,
        legend_y + 9.0,
    ));

    svg.push_str("</svg>\n");
    svg
}

/// 図 3-6 散布図直下の統計サマリ HTML (P2-1, 2026-05-28)。
///
/// n / 平均レンジ幅 / レンジ <5万円割合 (定額求人傾向) / レンジ >=10万円割合
/// (歩合・等級制傾向) を 1 段組で記す。
///
/// R2-P0-1 (ultrathink Round 2, 2026-05-28): build_navy_salary_scatter_svg が
/// 軸 15-60 万円固定で範囲外データをクランプして描画する仕様に対し、
/// caption で「N 件 (X%) が範囲外のため端点に表示」と明記し、ユーザーが
/// 散布図の打点位置を誤読しないよう注記する。
///
/// R2-P1-1 (ultrathink Round 2, 2026-05-28): n=0 を早期 return で防御済みのため
/// 0 除算は発生しないが、`safe_pct` ヘルパで NaN/Inf も明示的に 0.0% に丸める
/// (二重防衛)。
///
/// 不変条件:
/// - `pairs.len() == n >= 0`
/// - 全 `(lo, hi)` で `hi >= lo` (fetch_salary_scatter_pairs SQL でフィルタ済)
/// - `avg_width >= 0`, `0 <= narrow_pct <= 100`, `0 <= wide_pct <= 100`
/// - `clamp_count <= n`, `0 <= clamp_pct <= 100`
///
/// Phase 2-A (2026-05-29): `is_hourly` 引数化。
/// - 月給モード: narrow=5万円未満 / wide=10万円以上、軸範囲 15-60 万円
/// - 時給モード: narrow=100 円/時 未満 / wide=300 円/時 以上、軸範囲 800-2500 円/時
pub(super) fn build_salary_scatter_summary(pairs: &[(f64, f64)], is_hourly: bool) -> String {
    if pairs.is_empty() {
        return String::new();
    }

    let n = pairs.len();
    // レンジ幅 (円ベース、表示単位換算は最後)
    let widths_yen: Vec<f64> = pairs.iter().map(|(lo, hi)| hi - lo).collect();
    let sum_width: f64 = widths_yen.iter().sum();
    let avg_width_yen: f64 = sum_width / n as f64;

    // Phase 2-A: 月給/時給 で閾値 + 軸範囲 + 単位表示を切替
    let (
        narrow_threshold_yen,
        wide_threshold_yen,
        x_min_disp,
        x_max_disp,
        avg_disp_text,
        narrow_label,
        wide_label,
        axis_range_text,
    ): (f64, f64, f64, f64, String, String, String, String) = if is_hourly {
        // 時給モード: 100 円/時 = narrow / 300 円/時 = wide
        let avg_disp = safe_pct_like(avg_width_yen); // 円のまま
        (
            100.0,
            300.0,
            800.0,
            2500.0,
            format!("{:.0}円/時", avg_disp),
            "100円/時".to_string(),
            "300円/時".to_string(),
            "800-2500円/時".to_string(),
        )
    } else {
        // 月給モード (旧動作): 5 万円 / 10 万円
        let avg_disp_man = safe_pct_like(avg_width_yen / 10_000.0);
        (
            50_000.0,
            100_000.0,
            15.0,
            60.0,
            format!("{:.1}万円", avg_disp_man),
            "5万円".to_string(),
            "10万円".to_string(),
            "15-60万円".to_string(),
        )
    };

    let narrow_count = widths_yen
        .iter()
        .filter(|w| **w < narrow_threshold_yen)
        .count();
    let wide_count = widths_yen
        .iter()
        .filter(|w| **w >= wide_threshold_yen)
        .count();

    let narrow_pct: f64 = safe_pct(narrow_count as f64 / n as f64 * 100.0);
    let wide_pct: f64 = safe_pct(wide_count as f64 / n as f64 * 100.0);

    // build_navy_salary_scatter_svg と同じ軸範囲でクランプ件数を算出 (表示単位ベース)
    let clamp_count = pairs
        .iter()
        .filter(|(lo, hi)| {
            let (lo_disp, hi_disp) = if is_hourly {
                (*lo, *hi)
            } else {
                (lo / 10_000.0, hi / 10_000.0)
            };
            lo_disp < x_min_disp
                || lo_disp > x_max_disp
                || hi_disp < x_min_disp
                || hi_disp > x_max_disp
        })
        .count();
    let clamp_pct: f64 = safe_pct(clamp_count as f64 / n as f64 * 100.0);

    let mut s = format!(
        "<p class=\"caption\">n={} / 平均レンジ幅 {} / レンジ &lt;{} {:.1}% (定額求人傾向) / レンジ &ge;{} {:.1}% (歩合・等級制傾向)",
        format_number(n as i64),
        avg_disp_text,
        narrow_label,
        narrow_pct,
        wide_label,
        wide_pct,
    );
    if clamp_count > 0 {
        s.push_str(&format!(
            " / 軸範囲 {} のため {} 件 ({:.1}%) が範囲外として端点に表示",
            axis_range_text,
            format_number(clamp_count as i64),
            clamp_pct
        ));
    }
    s.push_str("</p>\n");
    s
}

/// 共通 helper: 0除算結果 (NaN) や ±Inf を明示的に 0.0 に丸め、
/// 計算上 100% を超えてしまった値 (浮動小数誤差等) も [0.0, 100.0] にクランプする。
///
/// R2-P1-1 (ultrathink Round 2, 2026-05-28): `format!("{:.1}%", v)` が
/// "NaN%" / "inf%" を出力するのを防ぐ二重防衛。
#[inline]
pub(super) fn safe_pct(v: f64) -> f64 {
    if v.is_nan() || v.is_infinite() {
        0.0
    } else {
        v.clamp(0.0, 100.0)
    }
}

/// % 以外の数値 (平均値など) でも NaN/Inf を 0.0 に丸めるだけのヘルパ。
/// 上限クランプはしない (大きな値が正当な範囲)。
#[inline]
pub(super) fn safe_pct_like(v: f64) -> f64 {
    if v.is_nan() || v.is_infinite() {
        0.0
    } else {
        v
    }
}
