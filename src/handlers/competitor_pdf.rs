//! Fixed-page, offline PDF export. Uploaded data is kept only in a private temporary directory.
use std::{process::Stdio, sync::OnceLock, time::Duration};

use tokio::{process::Command, sync::Semaphore};

const LAYOUT: &str = r#"<style>
@page{size:A3 landscape;margin:8mm}
body.pdf-document{background:white;font-family:'Noto Sans CJK JP','Yu Gothic','Meiryo',sans-serif;overflow:visible;print-color-adjust:exact;-webkit-print-color-adjust:exact}
.pdf-document main{max-width:none;padding:0;margin:0}
.pdf-document .no-print{display:none!important}
.pdf-document .pdf-page{width:1510px;height:1045px;position:relative;break-after:page;page-break-after:always}
.pdf-document .pdf-page:last-child{break-after:auto;page-break-after:auto}
.pdf-document .pdf-content{width:1510px;display:flow-root}
.pdf-document .excel-dashboard{grid-template-columns:54% 46%;break-inside:auto}
.pdf-document .excel-dashboard.kw-tab{grid-template-columns:42% 58%}
.pdf-document .kw-tab .summary{columns:auto}
.pdf-document .kw-tab .summary td:not(:first-child){white-space:nowrap}
.pdf-document .charts{grid-template-columns:1fr 1fr;grid-template-rows:200px 200px minmax(520px,auto)}
.pdf-document .keyword-comparison{min-height:520px}
.pdf-document .keyword-comparison svg{min-height:460px}
.pdf-document .chart.wide{grid-column:1/-1}
.pdf-document .summary td,.pdf-document .summary th{overflow-wrap:anywhere}
.pdf-document .summary{padding:10px 12px;columns:2;column-gap:16px}
.pdf-document .summary>h1,.pdf-document .summary>.meta{column-span:all}
.pdf-document .summary td:not(:first-child){white-space:nowrap}
.pdf-document .summary>h2,.pdf-document .summary>.band-title{break-after:avoid}
.pdf-document .summary>table:not(.words):not(.word-comparison),.pdf-document .summary .kw-block{break-inside:avoid}
.pdf-document .summary table{font-size:12px;margin-bottom:8px}
.pdf-document .summary th,.pdf-document .summary td{padding:2px 4px;line-height:1.15}
.pdf-document .summary .meta td{font-size:14px;padding:4px}
.pdf-document .summary h1{font-size:22px;margin:4px 0 10px}
.pdf-document .summary h2{font-size:14px;margin:8px 0 5px}
.pdf-document .summary .note{font-size:10px;line-height:1.4}
.pdf-document .page-navy{margin-top:0;break-inside:auto}
/* Google 検索需要は縦に長いので 2 段組にして、ページ全体の縮小を小さくする。 */
.pdf-document #competitor-google{columns:3;column-gap:24px}
.pdf-document #competitor-google>*{break-inside:avoid}
.pdf-document #competitor-google>div:not(.page-head){break-inside:auto}
.pdf-document #competitor-google .trend-card{break-inside:avoid}
.pdf-document #competitor-google>p:has(+.trend-card),.pdf-document #competitor-google>p:has(+p+.trend-card){break-after:avoid}
.pdf-document #competitor-google>.block-title{break-after:avoid-column;break-after:avoid}
.pdf-document #competitor-google>h1,.pdf-document #competitor-google>.page-header,.pdf-document #competitor-google>header{column-span:all}
.pdf-document #panel-indeed-keywords{display:block;columns:3;column-gap:24px}
.pdf-document #panel-indeed-keywords>.block-title,.pdf-document #panel-indeed-keywords>.kw-caveats{column-span:all}
.pdf-document #panel-indeed-keywords>.kw-part{break-inside:avoid}
.pdf-document #panel-indeed-keywords .kw-part{min-width:0}
/* 印刷時の幅は Chromium が 900px 未満として扱い、画面用の @media(max-width:900px) が効いて 1 列になる。
   fitPages は 2 列の高さで縮小率を決めるので、印刷時に 1 列になるとページの下へ溢れる。 */
.pdf-document .trend-grid{grid-template-columns:1fr 1fr}
/* Indeed: 推移グラフ (2x2) を左、月別の表を右に置く。縦に長いままだと高さで縮んで右が空く。 */
.pdf-document #competitor-indeed{display:grid;grid-template-columns:minmax(0,1fr) 32%;column-gap:24px;align-items:start}
.pdf-document #competitor-indeed>*{grid-column:1/-1;min-width:0}
.pdf-document #competitor-indeed>.trend-grid{grid-column:1;margin:12px 0}
.pdf-document #competitor-indeed>.table-navy{grid-column:2;margin:12px 0}
.pdf-document .population-metrics{grid-template-columns:repeat(3,1fr)}
.pdf-document .population-grid{grid-template-columns:1.4fr 1fr}
/* 人口: 人口ピラミッドと年齢別の表を左、最低賃金・労働統計を右に置く (縦に長いままだと高さで縮んで右が空く)。 */
.pdf-document #panel-population>.page-navy:has(>.population-grid~:is(table.table-navy,.pdf-population-context)){display:grid;grid-template-columns:minmax(0,2.4fr) minmax(0,1fr);grid-template-rows:auto auto auto auto auto auto 1fr;column-gap:28px;align-items:start}
.pdf-document #panel-population>.page-navy:has(>.population-grid~:is(table.table-navy,.pdf-population-context))>*{grid-column:1/-1;min-width:0}
.pdf-document #panel-population>.page-navy:has(>.population-grid~:is(table.table-navy,.pdf-population-context))>.population-grid{grid-column:1;grid-row:span 3}
.pdf-document #panel-population>.page-navy:has(>.population-grid~:is(table.table-navy,.pdf-population-context))>.population-grid~*{grid-column:2}
.pdf-document .consultation-grid{display:block;columns:2;column-gap:24px}
.pdf-document .consultation-card{break-inside:avoid;margin-bottom:12px;padding:10px}
.pdf-document .consultation-card p{font-size:13px;line-height:1.5}
/* ユーザー指定(2026-10-28): 1ページ目は Excel の元のレイアウトで固定。変更しない。 */
/* PAGE1-FIXED-BEGIN */
.pdf-document #panel-excel .excel-dashboard{grid-template-columns:32% 68%;break-inside:auto}
.pdf-document #panel-excel .charts{grid-template-columns:1fr 1fr;grid-template-rows:220px 220px minmax(700px,auto)}
.pdf-document #panel-excel .keyword-comparison{min-height:700px}
.pdf-document #panel-excel .keyword-comparison svg{min-height:640px}
.pdf-document #panel-excel .summary{padding:10px 12px;columns:auto}
.pdf-document #panel-excel .summary td:not(:first-child){white-space:normal}
/* PAGE1-FIXED-END */
.pdf-document .table-navy{table-layout:fixed;overflow-wrap:anywhere}
.pdf-document .table-navy td,.pdf-document .table-navy th{overflow-wrap:anywhere}
/* PDF の2ページ目以降だけを補正。画面用の狭幅ルールで図が縦積みになるのを防ぐ。 */
.pdf-document #panel-keyword .charts{grid-template-columns:1fr 1fr}
.pdf-document #panel-keyword .summary table{font-size:14px}
.pdf-document #panel-keyword .summary th,.pdf-document #panel-keyword .summary td{padding:4px;line-height:1.3}
.pdf-document #competitor-google .table-navy td,.pdf-document #competitor-google .table-navy th{padding:3px;line-height:1.25}
.pdf-document #competitor-google .trend-card{padding:10px}
.pdf-document #competitor-google .block-title{margin-top:10px}
.pdf-document #competitor-google .pdf-heading-group{display:inline-block;width:100%;vertical-align:top;break-inside:avoid}
.pdf-document #competitor-google .trend-plot svg text{font-size:18px}
.pdf-document #competitor-google .trend-plot svg[viewBox^="0 0 900"] text{font-size:22px}
.pdf-document #competitor-google .trend-plot svg[viewBox="0 0 900 226"] text[text-anchor="middle"]{font-size:20px}
.pdf-document #competitor-google .trend-plot svg[viewBox="0 0 900 300"] text{font-size:14px}
.pdf-document #competitor-google .trend-plot svg[viewBox="0 0 900 300"] text[font-size="13"]{font-size:13px}
.pdf-document #competitor-google .trend-plot svg[viewBox="0 0 900 300"] text[fill='#8a6d1d']{transform:translateY(6px)}
.pdf-document #competitor-google:not(:has(svg,table)){columns:auto}
.pdf-document #panel-population .population-grid svg text{font-size:20px}
.pdf-document #panel-indeed .trend-plot svg text,.pdf-document #panel-indeed-keywords .trend-plot svg text{font-size:22px}
</style>"#;

const FIT: &str = r#"<script>
document.body.classList.add('pdf-document');
for(const panel of document.querySelectorAll('[role="tabpanel"]')){
  panel.hidden=false;
  panel.classList.add('pdf-content');
  const page=document.createElement('section');page.className='pdf-page';
  panel.before(page);page.append(panel);
}
// 検索語の分析 (data-pdf-page) は Indeed の 1 ページに収まらないので、直後の別ページにする。
const kw=document.querySelector('[data-pdf-page="indeed-keywords"]');
if(kw){
  const host=kw.closest('.pdf-page');
  const page=document.createElement('section');page.className='pdf-page';
  kw.id='panel-indeed-keywords';kw.classList.add('pdf-content');
  page.append(kw);host.after(page);
}
// 印刷だけで見出し・注記・直後の表/図をひとまとまりにする。画面用HTMLは変えない。
const google=document.querySelector('#competitor-google');
if(google){
  for(const heading of [...google.querySelectorAll('.block-title')]){
    const group=document.createElement('div');group.className='pdf-heading-group';
    heading.before(group);group.append(heading);
    while(group.nextElementSibling){
      const next=group.nextElementSibling;
      if(!next.matches('p,table,figure'))break;
      group.append(next);
      if(next.matches('table,figure'))break;
    }
  }
}
// 構成比ありの人口も、人数だけの経路と同じ既存の図・表の並びにそろえる。
const population=document.querySelector('#panel-population>.page-navy');
if(population?.querySelector(':scope > .population-metrics')){
  const figure=population.querySelector(':scope > figure.trend-card');
  const note=figure?.nextElementSibling;
  const table=note?.nextElementSibling;
  if(figure&&note?.matches('p.note')&&table?.matches('table.table-navy')){
    const grid=document.createElement('div');grid.className='population-grid';
    const plot=document.createElement('div');const values=document.createElement('div');
    figure.before(grid);plot.append(figure,note);values.append(table);grid.append(plot,values);
    if(grid.nextElementSibling){
      const context=document.createElement('div');context.className='pdf-population-context';
      grid.after(context);
      while(context.nextElementSibling)context.append(context.nextElementSibling);
    }
  }
}
function fitPages(){
  // 文字を大きくした横軸は、最新月を残して近接する月だけ間引く。
  for(const svg of document.querySelectorAll('#panel-indeed svg,#panel-indeed-keywords svg')){
    let nextLeft=Infinity;
    for(const label of [...svg.querySelectorAll('text[y="268"]')].reverse()){
      label.style.visibility='';
      const box=label.getBBox();
      if(box.x+box.width+8>nextLeft)label.style.visibility='hidden';
      else nextLeft=box.x;
    }
  }
  for(const panel of document.querySelectorAll('.pdf-content')){
    panel.style.zoom='';panel.style.width='';
    let scale=Math.min(1,1508/Math.max(1510,panel.scrollWidth),1040/Math.max(1,panel.scrollHeight));
    // 1 ページ目 (#panel-excel) は元のレイアウトで固定するので拡大しない。
    // 縦が余り (内容がページの半分以上ある) ときは、レイアウト幅を狭めて拡大し、紙面を使い切る。
    // 拡大しても幅・高さがページに収まる最大の倍率だけを採る。収まらなければ 1 のまま。
    if(panel.id!=='panel-excel'&&scale>=0.99&&panel.scrollHeight>=470){
      for(let z=1.4;z>1.02;z-=0.05){
        const wd=Math.floor(1508/z);
        panel.style.width=wd+'px';panel.style.zoom='1';
        if(panel.scrollWidth<=wd+1&&panel.scrollHeight*z<=1030){scale=z;break;}
        panel.style.width='';
      }
    }
    panel.style.zoom=String(scale);
    // Zoom participates in pagination; transforms would clip the unscaled layout at a page break.
    for(let i=0;i<4;i++){
      const box=panel.getBoundingClientRect();
      const correction=Math.min(1,1508/box.width,1040/box.height);
      if(correction>=1)break;
      scale*=correction;panel.style.zoom=String(scale);
    }
    panel.dataset.pdfScale=String(scale);
  }
  document.documentElement.dataset.pdfReady='true';
}
fitPages();
document.fonts.ready.then(fitPages);
window.addEventListener('beforeprint',fitPages);
</script>"#;

pub(super) fn document(html: &str) -> String {
    // The report contains only escaped text, local styles and inline SVG. Prevent network access.
    let html = html.replace("<head>", "<head><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:\">");
    let html = html.replace("</head>", &format!("{LAYOUT}</head>"));
    let html = html.replace(include_str!("../../static/js/competitor-tabs.js"), "");
    html.replace("</body>", &format!("{FIT}</body>"))
}

fn browser() -> std::ffi::OsString {
    if let Some(path) = std::env::var_os("PDF_CHROMIUM_PATH") {
        return path;
    }
    #[cfg(windows)]
    {
        return r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe".into();
    }
    // Playwright の executablePath は PATH を引かないので、名前だけ ("chromium") では起動できない
    // (2026-10-06 本番で "executable doesn't exist at chromium")。Debian の chromium パッケージの絶対パス。
    #[cfg(not(windows))]
    DEFAULT_LINUX_CHROMIUM.into()
}

#[cfg(not(windows))]
const DEFAULT_LINUX_CHROMIUM: &str = "/usr/bin/chromium";

pub(super) async fn generate(html: &str) -> Result<Vec<u8>, String> {
    static LIMIT: OnceLock<Semaphore> = OnceLock::new();
    let _permit = tokio::time::timeout(
        Duration::from_secs(60),
        LIMIT.get_or_init(|| Semaphore::new(1)).acquire(),
    )
    .await
    .map_err(|_| "PDF作成が混み合っています。少し時間をおいて再度お試しください。")?
    .map_err(|_| "PDF作成を開始できませんでした。")?;
    let dir = tempfile::tempdir().map_err(|_| "PDFの作成準備に失敗しました。")?;
    let input = dir.path().join("report.html");
    let output = dir.path().join("report.pdf");
    let diagnostic_path = dir.path().join("chromium.log");
    let diagnostic_file =
        std::fs::File::create(&diagnostic_path).map_err(|_| "PDFの作成準備に失敗しました。")?;
    tokio::fs::write(&input, document(html))
        .await
        .map_err(|_| "PDFの作成準備に失敗しました。")?;
    let url = reqwest::Url::from_file_path(&input).map_err(|_| "PDFの作成準備に失敗しました。")?;
    let helper = dir.path().join("render.cjs");
    tokio::fs::write(&helper, include_str!("../../scripts/pdf/render.cjs"))
        .await
        .map_err(|_| "PDFの作成準備に失敗しました。")?;
    let module = std::env::var_os("PDF_PLAYWRIGHT_MODULE").unwrap_or_else(|| {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("node_modules/playwright-core")
            .into_os_string()
    });
    let mut command =
        Command::new(std::env::var_os("PDF_NODE_PATH").unwrap_or_else(|| "node".into()));
    command
        .arg(helper)
        .arg(browser())
        .arg(url.as_str())
        .arg(&output)
        .arg(module);
    command
        .stdout(Stdio::null())
        .stderr(Stdio::from(diagnostic_file))
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|err| {
        tracing::error!(error = %err, "Cannot start competitor PDF renderer");
        "PDFを作成できませんでした。時間をおいて再度お試しください。"
    })?;
    // Complete files can be returned while the helper finishes closing its browser.
    let result = tokio::time::timeout(Duration::from_secs(45), async {
        loop {
            if let Ok(bytes) = tokio::fs::read(&output).await {
                if bytes.starts_with(b"%PDF-")
                    && bytes[bytes.len().saturating_sub(32)..]
                        .windows(5)
                        .any(|s| s == b"%%EOF")
                {
                    return Ok(bytes);
                }
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|_| "PDF作成の状態を確認できませんでした。")?
            {
                tracing::warn!(
                    ?status,
                    "Competitor PDF renderer exited without a complete PDF"
                );
                return Err("PDFを作成できませんでした。再度お試しください。");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    let _ = child.kill().await;
    if !matches!(&result, Ok(Ok(_))) {
        let diagnostic = tokio::fs::read_to_string(&diagnostic_path)
            .await
            .unwrap_or_default();
        tracing::warn!(diagnostic = %diagnostic.chars().take(2000).collect::<String>(), "Competitor PDF Chromium diagnostic");
    }
    result
        .map_err(|_| "PDFの作成に時間がかかっています。再度お試しください。".to_string())?
        .map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    /// ユーザー指定(2026-10-28): 1ページ目は Excel の元のレイアウトで固定。変更しない。
    #[test]
    fn page_one_layout_is_pinned_to_the_original() {
        let begin = super::LAYOUT.find("PAGE1-FIXED-BEGIN").expect("begin");
        let end = super::LAYOUT.find("PAGE1-FIXED-END").expect("end");
        let page1 = &super::LAYOUT[begin..end];
        assert!(page1.contains("grid-template-columns:32% 68%"));
        assert!(page1.contains("grid-template-rows:220px 220px minmax(700px,auto)"));
        assert!(!page1.contains("54%") && !page1.contains("46%"));
        assert!(!page1.contains("columns:2") && !page1.contains("column-gap"));
        assert!(super::FIT.contains("panel.id!=='panel-excel'&&scale>=0.99"));
    }

    /// Playwright は executablePath を PATH から探さない。名前だけの既定値では本番で起動できなかった (2026-10-06)。
    #[cfg(not(windows))]
    #[test]
    fn default_linux_chromium_is_an_absolute_path() {
        assert!(std::path::Path::new(super::DEFAULT_LINUX_CHROMIUM).is_absolute());
    }

    /// 本番イメージは PDF_CHROMIUM_PATH を絶対パスで渡し、ビルド時にその実体を起動して確かめる。
    #[test]
    fn dockerfile_sets_an_absolute_chromium_path_and_checks_it() {
        let docker = include_str!("../../Dockerfile");
        let line = docker
            .lines()
            .find(|l| l.trim_start().starts_with("ENV PDF_CHROMIUM_PATH="))
            .expect("Dockerfile must set PDF_CHROMIUM_PATH");
        let path = line.trim().trim_start_matches("ENV PDF_CHROMIUM_PATH=");
        assert!(
            path.starts_with('/'),
            "PDF_CHROMIUM_PATH must be absolute: {path}"
        );
        assert!(docker.contains("\"$PDF_CHROMIUM_PATH\" --version"));
        #[cfg(not(windows))]
        assert_eq!(path, super::DEFAULT_LINUX_CHROMIUM);
    }

    #[tokio::test]
    #[ignore = "Requires a Chromium/Edge executable; set COMPETITOR_PDF_TEST_HTML to the source report"]
    async fn export_fixed_pdf_from_real_report() {
        let input = std::env::var("COMPETITOR_PDF_TEST_HTML").expect("source HTML path");
        let html = std::fs::read_to_string(&input).unwrap();
        let pdf = super::generate(&html).await.unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.len() > 10_000);
        std::fs::write(std::path::Path::new(&input).with_extension("pdf"), pdf).unwrap();
        std::fs::write(
            std::path::Path::new(&input).with_extension("fixed.html"),
            super::document(&html),
        )
        .unwrap();

        // Exercise realistic row counts without making paid Google API calls.
        use crate::handlers::survey::{
            aggregator::aggregate_records_with_mode,
            upload::{parse_csv_bytes_with_hints, UserSourceHint, WageMode},
        };
        use serde_json::json;
        let records = parse_csv_bytes_with_hints("タイトル,会社名,勤務地,給与,雇用形態\n施設長,A社,大阪府大阪市,月給 25万円 ~ 30万円,正社員\n".as_bytes(), Some("大阪府"), UserSourceHint::Indeed).unwrap();
        let agg = aggregate_records_with_mode(&records, WageMode::Monthly);
        let indeed = json!({"status":"ok","region":"大阪府","rows":(1..=24).map(|i|json!({"month":format!("検証月{i}"),"job":100,"ctk":250,"emp":20,"spp":2.5})).collect::<Vec<_>>()});
        let google = json!({"status":"ok","keyword":"検証用データ・施設長 求人","region":"大阪府","demand":{"status":"ok","keywords":[{"keyword":"施設長 求人","avg_monthly":320,"competition":"HIGH","monthly_12m":(1..=12).map(|i|json!({"month":format!("検証月{i}"),"search_volume":390})).collect::<Vec<_>>()}]},"suggestions":{"status":"ok","suggestions":(1..=20).map(|i|json!({"keyword":format!("施設長 転職 検証語{i}"),"avg_monthly":170})).collect::<Vec<_>>()}});
        let population = json!({"status":"ok","region":"大阪府（検証用データ）","bands":(0..18).map(|i|json!({"age_group":format!("{}〜{}歳",i*5,i*5+4),"male_count":1234,"female_count":2345})).collect::<Vec<_>>(),"minimum_wage":1231,"labor":{"fiscal_year":2024,"unemployment_rate":2.5}});
        let rich = crate::handlers::survey::report_html::render_competitor_report(
            &agg,
            45,
            "検証用データ",
            &indeed,
            &google,
            &population,
        );
        let rich_pdf = super::generate(&rich).await.unwrap();
        std::fs::write(
            std::path::Path::new(&input).with_extension("rich.pdf"),
            rich_pdf,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&input).with_extension("rich.fixed.html"),
            super::document(&rich),
        )
        .unwrap();
    }
}
