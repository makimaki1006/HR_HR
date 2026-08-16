//! 勤務地パースの実データ監査用プローブ (一時的な検査。コミット対象ではない)
//!
//! 実行:
//!   cargo run --release --bin probe_location -- <出力ディレクトリ> <CSV...>
//!
//! 出力:
//!   - raw_locations.tsv : 実CSVを parse_csv_bytes に通して得た location_raw と
//!                         location_parsed (実際のパイプラインと同じ経路)
//!   - unique_parsed.tsv : ユニーク勤務地文字列 → 都道府県 / 市区町村 / method
//!   - synthetic.tsv     : 合成ケース (同名市区町村・郡部・表記揺れ等)

use rust_dashboard::handlers::survey::location_parser::parse_location;
use rust_dashboard::handlers::survey::upload::parse_csv_bytes;
use std::collections::BTreeMap;
use std::io::Write;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        anyhow::bail!("使い方: probe_location <出力ディレクトリ> <CSV...>");
    }
    let out_dir = std::path::PathBuf::from(&args[0]);
    std::fs::create_dir_all(&out_dir)?;

    let mut raw = std::fs::File::create(out_dir.join("raw_locations.tsv"))?;
    writeln!(
        raw,
        "file\trow_index\tlocation_raw\tprefecture\tmunicipality\tcity_type\tmethod\tconfidence"
    )?;

    // ユニーク勤務地文字列 → 出現件数
    let mut uniq: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_records = 0usize;

    for path in &args[1..] {
        let bytes = std::fs::read(path)?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let records = match parse_csv_bytes(&bytes, None) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[SKIP] {name}: {e}");
                continue;
            }
        };
        eprintln!("[OK] {name}: {} records", records.len());
        total_records += records.len();
        for rec in &records {
            let p = &rec.location_parsed;
            writeln!(
                raw,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                name,
                rec.row_index,
                esc(&rec.location_raw),
                p.prefecture.clone().unwrap_or_default(),
                p.municipality.clone().unwrap_or_default(),
                p.city_type.clone().unwrap_or_default(),
                p.method,
                p.confidence
            )?;
            *uniq.entry(rec.location_raw.clone()).or_default() += 1;
        }
    }
    eprintln!(
        "total records = {total_records}, unique locations = {}",
        uniq.len()
    );

    // ユニーク文字列を parse_location に直接通す (context_pref なし)
    let mut uf = std::fs::File::create(out_dir.join("unique_parsed.tsv"))?;
    writeln!(
        uf,
        "count\tinput\tprefecture\tmunicipality\tregion_block\tcity_type\tmethod\tconfidence"
    )?;
    for (text, count) in &uniq {
        let p = parse_location(text, None);
        writeln!(
            uf,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            count,
            esc(text),
            p.prefecture.clone().unwrap_or_default(),
            p.municipality.clone().unwrap_or_default(),
            p.region_block.clone().unwrap_or_default(),
            p.city_type.clone().unwrap_or_default(),
            p.method,
            p.confidence
        )?;
    }

    // ===== 合成ケース =====
    let cases: &[(&str, Option<&str>)] = &[
        // 政令指定都市の区
        ("神奈川県 川崎市 川崎区 東扇島", None),
        ("神奈川県川崎市川崎区東扇島", None),
        ("神奈川県 横浜市 港北区", None),
        ("大阪府 大阪市 淀川区", None),
        ("北海道 札幌市 白石区", None),
        ("福岡県 福岡市 博多区", None),
        ("神奈川県 川崎市 川崎駅", None),
        // 郡部
        ("群馬県 佐波郡 玉村町", None),
        ("群馬県佐波郡玉村町", None),
        ("北海道 河東郡 音更町", None),
        ("栃木県 芳賀郡 市貝町", None),
        ("沖縄県 中頭郡 北谷町", None),
        ("東京都 西多摩郡 檜原村", None),
        // 「市」で始まる市名
        ("千葉県 市川市", None),
        ("千葉県市川市", None),
        ("千葉県 市原市 五井", None),
        ("三重県 四日市市", None),
        ("山梨県 西八代郡 市川三郷町", None),
        ("鹿児島県 いちき串木野市", None),
        // 同名市区町村（都道府県文脈）
        ("東京都 府中市", None),
        ("広島県 府中市", None),
        ("北海道 伊達市", None),
        ("福島県 伊達市", None),
        ("府中市", None),
        ("伊達市", None),
        ("府中市", Some("広島県")),
        ("伊達市", Some("福島県")),
        // 表記揺れ
        ("東京都　新宿区", None), // 全角スペース
        ("東京都  新宿区", None), // 半角スペース2つ
        ("東京都新宿区西新宿2-8-1", None),
        ("東京都新宿区西新宿２−８−１", None), // 全角数字
        ("ﾄｳｷｮｳﾄ 新宿区", None),              // 半角カナ
        ("東京都 新宿区 ほか", None),
        ("東京都 新宿区、渋谷区", None),
        ("東京都 新宿区 / 渋谷区", None),
        ("東京都 新宿区 ・ 渋谷区", None),
        // 駅名・非住所
        ("新宿駅", None),
        ("新宿駅から徒歩5分", None),
        ("駅チカ", None),
        ("リモート", None),
        ("在宅", None),
        ("在宅勤務あり", None),
        ("全国各地", None),
        ("東京都 千代田区 (在宅勤務可)", None),
        // ambiguous 先行が住所を潰す疑いのあるケース
        ("愛知県 東海市", None),
        ("茨城県 那珂郡 東海村", None),
        ("東京都 東大和市", None),
        ("北海道 北広島市", None),
        ("大阪府 東大阪市", None),
        ("神奈川県 横浜市 都筑区", None),
        ("愛知県 名古屋市 東区", None),
        ("東京都 港区 (全国転勤あり)", None),
        ("静岡県 静岡市 葵区 (在宅勤務可)", None),
        // 共有区名
        ("大阪府 大阪市 北区", None),
        ("大阪府 堺市 北区", None),
        ("愛知県 名古屋市 中区", None),
        ("兵庫県 神戸市 中央区", None),
        ("北区", None),
        ("中央区", None),
        // 複数県表記
        ("東京都 / 神奈川県", None),
        ("埼玉県 さいたま市 大宮区", None),
    ];
    let mut sf = std::fs::File::create(out_dir.join("synthetic.tsv"))?;
    writeln!(
        sf,
        "input\tcontext\tprefecture\tmunicipality\tregion_block\tcity_type\tmethod\tconfidence"
    )?;
    for (text, ctx) in cases {
        let p = parse_location(text, *ctx);
        writeln!(
            sf,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            esc(text),
            ctx.unwrap_or(""),
            p.prefecture.clone().unwrap_or_default(),
            p.municipality.clone().unwrap_or_default(),
            p.region_block.clone().unwrap_or_default(),
            p.city_type.clone().unwrap_or_default(),
            p.method,
            p.confidence
        )?;
    }

    eprintln!("done -> {}", out_dir.display());
    Ok(())
}

fn esc(s: &str) -> String {
    s.replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "")
}
