//! ガイドタブハンドラー
//! docs/USER_GUIDE.md の内容をHTMLとして表示する

use axum::response::Html;

/// ガイドタブ: 取扱説明書をHTML形式で表示
pub async fn tab_guide() -> Html<String> {
    let html = build_guide_html();
    Html(html)
}

/// ガイドHTMLを構築（マークダウンパーサー不使用、直接HTML記述）
fn build_guide_html() -> String {
    format!(
        r##"<div class="space-y-4">

  <!-- タイトル -->
  <div class="stat-card">
    <h2 class="text-xl font-bold text-white mb-2">📖 ハローワーク求人分析ダッシュボード 取扱説明書</h2>
    <p class="text-slate-400 text-sm">最終更新: 2026年3月19日</p>
  </div>

  <!-- ⚠️ データの範囲に関する重要な注意（常に展開） -->
  <div class="stat-card border border-amber-600/50">
    <h3 class="text-lg font-bold text-amber-400 mb-3">⚠️ データの範囲に関する重要な注意</h3>
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-slate-700">
          <th class="text-left py-2 px-3 text-slate-300 w-1/4">注意事項</th>
          <th class="text-left py-2 px-3 text-slate-300">説明</th>
        </tr>
      </thead>
      <tbody class="text-slate-400">
        <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">対象データ</td><td class="py-2 px-3">ハローワークインターネットサービスに掲載された求人<strong class="text-amber-400">のみ</strong></td></tr>
        <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">含まれないもの</td><td class="py-2 px-3">Indeed、リクナビ、マイナビ等の民間求人サイトの求人</td></tr>
        <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">データの偏り</td><td class="py-2 px-3">中小企業・医療福祉・製造業が多い。IT・スタートアップ・外資系は少ない</td></tr>
        <tr><td class="py-2 px-3 font-semibold text-white">地域差</td><td class="py-2 px-3">地方はHW比率が高く実態に近い。都市部は民間サイト併用が多くHWだけでは全貌が見えない</td></tr>
      </tbody>
    </table>
    <p class="mt-3 text-amber-300 text-sm font-semibold">このダッシュボードの数値は「HW掲載求人の中での分析結果」であり、求人市場全体を表すものではありません。</p>

    <div class="mt-4 pt-3 border-t border-slate-700">
      <h4 class="text-sm font-bold text-slate-300 mb-2">外部統計データについて</h4>
      <p class="text-slate-400 text-sm mb-2">一部のグラフには「※外部統計データ」と記載されたセクションがあります。これらは公的統計から取得したデータで、HW求人データとは別のデータソースです。</p>
      <table class="w-full text-sm">
        <tbody class="text-slate-400">
          <tr class="border-b border-slate-800"><td class="py-1 px-3 font-semibold text-white w-1/3">出典末尾に「※外部統計データ」</td><td class="py-1 px-3">国の統計（e-Stat API）から取得。全産業・全チャネルを含む</td></tr>
          <tr><td class="py-1 px-3 font-semibold text-white">出典末尾に記載なし</td><td class="py-1 px-3">HW掲載求人データから算出</td></tr>
        </tbody>
      </table>
      <p class="mt-2 text-slate-400 text-sm">外部統計の有効求人倍率（厚労省公表値）とHW求人データから算出した指標は計算方法が異なります。直接比較する際はこの点にご注意ください。</p>
    </div>
  </div>

  <!-- セクション1: はじめに -->
  {section_about}

  <!-- セクション2: 基本操作 -->
  {section_basic}

  <!-- セクション3: タブ別ガイド -->
  {section_tabs}

  <!-- セクション4: 指標辞典 -->
  {section_metrics}

  <!-- セクション5: ユースケース集 -->
  {section_usecases}

  <!-- セクション6: 外部データ出典 -->
  {section_sources}

  <!-- セクション7: FAQ・用語集 -->
  {section_faq}

</div>"##,
        section_about = build_section_about(),
        section_basic = build_section_basic(),
        section_tabs = build_section_tabs(),
        section_metrics = build_section_metrics(),
        section_usecases = build_section_usecases(),
        section_sources = build_section_sources(),
        section_faq = build_section_faq(),
    )
}

/// セクション: はじめに
fn build_section_about() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">📌 はじめに ― このダッシュボードでわかること</summary>
      <div class="mt-3 text-slate-400 text-sm space-y-2">
        <p>ハローワークインターネットサービスに掲載された求人データを多角的に分析し、<strong class="text-white">地域の求人市場の「今」と「動き」</strong>を可視化するツールです。</p>
        <p>以下のような問いに答えることができます:</p>
        <ul class="list-disc list-inside space-y-1 ml-2">
          <li>この地域の求人市場はどんな状態か？</li>
          <li>どんな企業が求人を出しているか？</li>
          <li>求人条件の相場はどのくらいか？</li>
          <li>自社の求人条件は市場でどの位置にあるか？</li>
          <li>求人市場はどう変化しているか？（📈 トレンドタブで約20ヶ月の時系列推移を確認）</li>
        </ul>
      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: 基本操作
fn build_section_basic() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">🖱️ ログインと基本操作</summary>
      <div class="mt-3 text-slate-400 text-sm space-y-4">
        <div>
          <h4 class="text-white font-semibold mb-1">ログイン</h4>
          <ul class="list-disc list-inside ml-2">
            <li>メールアドレスとパスワードを入力してログイン</li>
            <li>許可ドメイン: @f-a-c.co.jp, @cyxen.co.jp</li>
          </ul>
        </div>
        <div>
          <h4 class="text-white font-semibold mb-2">フィルタ操作（画面上部）</h4>
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-slate-700">
                <th class="text-left py-2 px-3 text-slate-300">フィルタ</th>
                <th class="text-left py-2 px-3 text-slate-300">操作</th>
                <th class="text-left py-2 px-3 text-slate-300">効果</th>
              </tr>
            </thead>
            <tbody>
              <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white font-semibold">都道府県</td><td class="py-2 px-3">プルダウンで選択</td><td class="py-2 px-3">全タブのデータが選択した都道府県に切り替わる</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white font-semibold">市区町村</td><td class="py-2 px-3">都道府県選択後に表示される</td><td class="py-2 px-3">さらに絞り込み。「すべて」で都道府県全体</td></tr>
              <tr><td class="py-2 px-3 text-white font-semibold">産業</td><td class="py-2 px-3">ドロップダウンで選択</td><td class="py-2 px-3">産業分類でフィルタ</td></tr>
            </tbody>
          </table>
          <p class="mt-2 text-slate-300">「全国」を選択すると47都道府県の合計・平均値が表示されます。</p>
        </div>
      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: タブ別ガイド（9タブ）
fn build_section_tabs() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">📑 タブ別ガイド（全9タブの読み方）</summary>
      <div class="mt-3 space-y-4">

        <!-- Tab 1: 地域概況 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">📊 Tab 1: 地域概況 ― 「この地域の求人市場はどんな状態か？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">セクション</th><th class="text-left py-2 px-3 text-slate-300">見るべきポイント</th><th class="text-left py-2 px-3 text-slate-300">判断基準</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">求人件数</td><td class="py-2 px-3">正社員/パートの比率</td><td class="py-2 px-3">パートが多い地域は非正規依存の可能性</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">平均給与</td><td class="py-2 px-3">全国平均との差</td><td class="py-2 px-3">全国平均より低い→採用難の一因かも</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">産業分布</td><td class="py-2 px-3">上位産業の偏り</td><td class="py-2 px-3">1産業が30%超→地域経済がその産業に依存</td></tr>
                <tr><td class="py-2 px-3">雇用形態分布</td><td class="py-2 px-3">正社員比率</td><td class="py-2 px-3">50%未満→非正規化が進んでいる地域</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-amber-400 text-xs">注意: ここに表示される数値はすべてHW掲載求人ベースです。</p>
          </div>
        </details>

        <!-- Tab 2: 企業分析 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">🏢 Tab 2: 企業分析 ― 「どんな企業が求人を出しているか？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">セクション</th><th class="text-left py-2 px-3 text-slate-300">見るべきポイント</th><th class="text-left py-2 px-3 text-slate-300">判断基準</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">企業規模分布</td><td class="py-2 px-3">従業員数別の求人件数</td><td class="py-2 px-3">小規模(10人未満)の求人が多い→個人事業主の多い地域</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">産業別求人密度</td><td class="py-2 px-3">産業ごとの求人の集中度</td><td class="py-2 px-3">HHIが高い→少数企業が市場を占有</td></tr>
                <tr><td class="py-2 px-3">法人あたり求人数</td><td class="py-2 px-3">1法人が出す平均求人数</td><td class="py-2 px-3">多い→採用に苦労している企業が多い</td></tr>
              </tbody>
            </table>
          </div>
        </details>

        <!-- Tab 3: 求人条件 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">💰 Tab 3: 求人条件 ― 「この地域の求人条件の相場はどのくらいか？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">セクション</th><th class="text-left py-2 px-3 text-slate-300">見るべきポイント</th><th class="text-left py-2 px-3 text-slate-300">判断基準</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">給与分布</td><td class="py-2 px-3">中央値と四分位範囲</td><td class="py-2 px-3">中央値が最低賃金×160hに近い→最低ライン</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">年間休日</td><td class="py-2 px-3">平均と分布</td><td class="py-2 px-3">105日未満→週休1日相当で労働環境が厳しい</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">賞与</td><td class="py-2 px-3">支給率と月数</td><td class="py-2 px-3">賞与なしが50%超→業界の慣習としてボーナスが少ない</td></tr>
                <tr><td class="py-2 px-3">雇用形態別比較</td><td class="py-2 px-3">正社員vsパートの条件差</td><td class="py-2 px-3">差が小さい→正社員のメリットが薄い</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-slate-500 text-xs">給与について: HW求人の給与は「基本給+手当」の月額表示が多いですが、時給表示のパート求人も含まれます。比較時は雇用形態フィルタを活用してください。</p>
          </div>
        </details>

        <!-- Tab 4: 採用動向 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">📋 Tab 4: 採用動向 ― 「企業はなぜ求人を出しているのか？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">セクション</th><th class="text-left py-2 px-3 text-slate-300">見るべきポイント</th><th class="text-left py-2 px-3 text-slate-300">判断基準</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">欠員補充率</td><td class="py-2 px-3">欠員理由の割合</td><td class="py-2 px-3">30%超→離職が多い可能性。定着支援の営業チャンス</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">増員率</td><td class="py-2 px-3">増員理由の割合</td><td class="py-2 px-3">高い→成長産業。ポジティブな採用</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">新設率</td><td class="py-2 px-3">新規事業所設立の割合</td><td class="py-2 px-3">高い→地域に新規参入が活発</td></tr>
                <tr><td class="py-2 px-3">募集理由の産業差</td><td class="py-2 px-3">産業ごとの理由分布</td><td class="py-2 px-3">介護・飲食で欠員が多い→構造的な人手不足</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-slate-500 text-xs">「未選択」について: 募集理由が「未選択」の求人が約23%あります。これらは「理由不明」として扱われます。</p>
          </div>
        </details>

        <!-- Tab 5: 求人地図 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">🗺️ Tab 5: 求人地図 ― 「求人はどこに集中しているか？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">操作</th><th class="text-left py-2 px-3 text-slate-300">説明</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">ピンの色</td><td class="py-2 px-3">給与水準や雇用形態で色分け</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">クリック</td><td class="py-2 px-3">求人の詳細情報を表示</td></tr>
                <tr><td class="py-2 px-3">ズーム</td><td class="py-2 px-3">市区町村レベルまで拡大可能</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-slate-500 text-xs">注意: 地図上のピンはジオコーディング（住所→緯度経度変換）の結果です。住所が不正確な求人はピンが表示されない場合があります。</p>
          </div>
        </details>

        <!-- Tab 6: 市場分析 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">📈 Tab 6: 市場分析 ― 「市場の構造と外部環境はどうなっているか？」</summary>
          <div class="mt-2 ml-4 space-y-3">
            <p class="text-slate-400 text-sm">6つのサブタブがあります。</p>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 1: 求人動向</h5>
              <table class="w-full text-sm">
                <thead><tr class="border-b border-slate-700"><th class="text-left py-1 px-2 text-slate-300">指標</th><th class="text-left py-1 px-2 text-slate-300">意味</th><th class="text-left py-1 px-2 text-slate-300">注意</th></tr></thead>
                <tbody class="text-slate-400">
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">欠員補充率</td><td class="py-1 px-2">求人理由が「欠員補充」の割合</td><td class="py-1 px-2">HW求人のみの値</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">業界分散度</td><td class="py-1 px-2">HHIの正規化値</td><td class="py-1 px-2">0.8以上=多様、0.5未満=集中リスク</td></tr>
                  <tr><td class="py-1 px-2">透明性スコア</td><td class="py-1 px-2">任意項目の記載率</td><td class="py-1 px-2">低い求人は情報を隠している可能性</td></tr>
                </tbody>
              </table>
            </div>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 2: 給与分析</h5>
              <table class="w-full text-sm">
                <thead><tr class="border-b border-slate-700"><th class="text-left py-1 px-2 text-slate-300">指標</th><th class="text-left py-1 px-2 text-slate-300">意味</th></tr></thead>
                <tbody class="text-slate-400">
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">給与競争力</td><td class="py-1 px-2">同地域・同産業内での相対順位</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">影の報酬</td><td class="py-1 px-2">基本給+賞与+残業代の推定年収</td></tr>
                  <tr><td class="py-1 px-2">最低賃金違反チェック</td><td class="py-1 px-2">時給換算で最低賃金を下回る求人の検出</td></tr>
                </tbody>
              </table>
            </div>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 3: テキスト分析</h5>
              <p class="text-slate-400 text-sm">求人票の文面から特徴語やテキスト品質を分析します。</p>
            </div>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 4: 市場構造</h5>
              <table class="w-full text-sm">
                <thead><tr class="border-b border-slate-700"><th class="text-left py-1 px-2 text-slate-300">指標</th><th class="text-left py-1 px-2 text-slate-300">意味</th><th class="text-left py-1 px-2 text-slate-300">注意</th></tr></thead>
                <tbody class="text-slate-400">
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">充足困難度スコア</td><td class="py-1 px-2">0-100で予測（高い=埋まりにくい）</td><td class="py-1 px-2">HW求人の特徴量から推定。実際の充足結果ではない</td></tr>
                  <tr><td class="py-1 px-2">グレード A/B/C/D</td><td class="py-1 px-2">A=容易〜D=困難</td><td class="py-1 px-2">予測モデルの結果であり確定的な判断ではない</td></tr>
                </tbody>
              </table>
            </div>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 5: 異常値・外部データ（最も情報が豊富）</h5>
              <table class="w-full text-sm">
                <thead><tr class="border-b border-slate-700"><th class="text-left py-1 px-2 text-slate-300">セクション</th><th class="text-left py-1 px-2 text-slate-300">データソース</th><th class="text-left py-1 px-2 text-slate-300">読み方</th></tr></thead>
                <tbody class="text-slate-400">
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">異常値検出</td><td class="py-1 px-2">HW求人</td><td class="py-1 px-2">給与・休日・従業員数で地域平均から大きく外れた求人の割合</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">最低賃金マスタ</td><td class="py-1 px-2">厚労省</td><td class="py-1 px-2">地域の最低賃金。求人給与との比較に</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">都道府県別外部指標</td><td class="py-1 px-2">複数の公的統計</td><td class="py-1 px-2">失業率・転職希望率・非正規率等。全産業データ</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">📈 有効求人倍率推移</td><td class="py-1 px-2">総務省 ※外部</td><td class="py-1 px-2">全産業の有効求人倍率の年度推移</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">📊 賃金推移</td><td class="py-1 px-2">総務省 ※外部</td><td class="py-1 px-2">全産業の現金給与月額の推移。男女別</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">👥 人口構成</td><td class="py-1 px-2">国勢調査 ※外部</td><td class="py-1 px-2">地域の人口・高齢化率・人口ピラミッド</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">🔄 人口動態</td><td class="py-1 px-2">SSDSE ※外部</td><td class="py-1 px-2">転入転出・昼夜間人口比</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">🏢 産業別事業所数</td><td class="py-1 px-2">経済センサス ※外部</td><td class="py-1 px-2">地域の全事業所数（HW未掲載含む）</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">📉 入職率・離職率</td><td class="py-1 px-2">雇用動向調査 ※外部</td><td class="py-1 px-2">医療・福祉産業の入職率と離職率。全チャネル</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">💰 消費支出</td><td class="py-1 px-2">家計調査 ※外部</td><td class="py-1 px-2">世帯の消費パターン。福利厚生設計の参考</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">🏗️ 事業所動態</td><td class="py-1 px-2">経済センサス ※外部</td><td class="py-1 px-2">開業率・廃業率の推移</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">🌡️ 気象特性</td><td class="py-1 px-2">気象統計 ※外部</td><td class="py-1 px-2">年平均気温・降雪日数等。通勤環境の参考</td></tr>
                  <tr class="border-b border-slate-800"><td class="py-1 px-2">🏥 介護需要推移</td><td class="py-1 px-2">社会統計 ※外部</td><td class="py-1 px-2">介護保険給付件数の推移。介護求人の先行指標</td></tr>
                  <tr><td class="py-1 px-2">🎯 地域ベンチマーク</td><td class="py-1 px-2">HW+外部統合</td><td class="py-1 px-2">12指標で地域を総合評価（0-100）</td></tr>
                </tbody>
              </table>
              <div class="mt-2 p-2 bg-amber-900/30 rounded text-amber-300 text-xs">
                <strong>外部データの読み方のコツ:</strong>
                <ul class="list-disc list-inside mt-1 space-y-1">
                  <li>「※外部統計データ」と書かれたセクションはHW求人とは別の公的統計です</li>
                  <li>外部統計は「全産業・全求人チャネル」を含むため、HW求人データの数値と直接比較できません</li>
                  <li>例: 外部統計の有効求人倍率が1.25でも、HW求人から算出した充足率とは一致しません</li>
                </ul>
              </div>
            </div>

            <div class="bg-slate-800/50 rounded p-3">
              <h5 class="text-cyan-300 font-semibold text-sm mb-2">サブタブ 6: 予測・推定</h5>
              <p class="text-slate-400 text-sm">充足困難度の予測モデル結果を表示します。</p>
            </div>
          </div>
        </details>

        <!-- Tab 7: 詳細検索 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">🔍 Tab 7: 詳細検索 ― 「条件に合う個別の求人を探したい」</summary>
          <div class="mt-2 ml-4">
            <p class="text-slate-400 text-sm">キーワードや条件で求人を検索し、個別の求人票を閲覧できます。</p>
          </div>
        </details>

        <!-- Tab 8: 市場診断 -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">🩺 Tab 8: 市場診断 ― 「自社の求人条件は市場でどのくらいの競争力があるか？」</summary>
          <div class="mt-2 ml-4">
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">操作</th><th class="text-left py-2 px-3 text-slate-300">説明</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3">月給・休日・賞与・雇用形態を入力</td><td class="py-2 px-3">自社の条件を入力</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3">「診断する」ボタン</td><td class="py-2 px-3">市場内でのポジションを診断</td></tr>
                <tr><td class="py-2 px-3">「リセット」ボタン</td><td class="py-2 px-3">フォームと結果をクリア（再診断時に使用）</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-slate-400 text-sm">レーダーチャートで5軸の相対位置を表示し、「市場平均より上/下」が一目でわかります。具体的な改善提案も表示されます。</p>
            <p class="mt-1 text-amber-400 text-xs">注意: 診断結果はHW掲載求人との比較です。民間サイトの求人条件との比較ではありません。</p>
          </div>
        </details>

        <!-- Tab 9: トレンド -->
        <details class="ml-2">
          <summary class="text-white font-semibold cursor-pointer hover:text-cyan-300">📈 Tab 9: トレンド ― 「求人市場はどう変化しているか？」</summary>
          <div class="mt-2 ml-4">
            <p class="text-slate-400 text-sm mb-2">HW過去データ（約20ヶ月分のスナップショット）を時系列で分析し、求人市場の変化を可視化します。</p>
            <table class="w-full text-sm">
              <thead><tr class="border-b border-slate-700"><th class="text-left py-2 px-3 text-slate-300">サブタブ</th><th class="text-left py-2 px-3 text-slate-300">内容</th></tr></thead>
              <tbody class="text-slate-400">
                <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">量の変化</td><td class="py-2 px-3">求人数・事業所数の推移、欠員補充率・増員率の変化</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">質の変化</td><td class="py-2 px-3">正社員給与（月額）・パート時給の推移、年間休日数の変化</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">構造の変化</td><td class="py-2 px-3">雇用形態別構成比の推移、平均掲載日数・長期掲載比率</td></tr>
                <tr class="border-b border-slate-800"><td class="py-2 px-3 font-semibold text-white">シグナル</td><td class="py-2 px-3">新規/継続/終了の推移、離脱率、充足困難度の変化</td></tr>
                <tr><td class="py-2 px-3 font-semibold text-white">外部比較</td><td class="py-2 px-3">有効求人倍率×HW求人数、賃金比較（厚労省月給vs HW正社員月給）、離職率比較、最低賃金×パート時給推移</td></tr>
              </tbody>
            </table>
            <p class="mt-2 text-slate-400 text-sm">都道府県フィルタに対応。市区町村フィルタは時系列データの集計単位上、非対応です。</p>
            <p class="mt-1 text-amber-400 text-xs">注意: このデータは過去のHW掲載求人のスナップショットです。リアルタイムの最新データではありません。</p>
          </div>
        </details>

      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: 指標辞典
fn build_section_metrics() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">📏 指標辞典</summary>
      <div class="mt-3 space-y-4">

        <div>
          <h4 class="text-white font-semibold mb-2">HW求人データから算出される指標</h4>
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-slate-700">
                <th class="text-left py-2 px-3 text-slate-300">指標名</th>
                <th class="text-left py-2 px-3 text-slate-300">定義</th>
                <th class="text-left py-2 px-3 text-slate-300">計算式</th>
                <th class="text-left py-2 px-3 text-slate-300">単位</th>
              </tr>
            </thead>
            <tbody class="text-slate-400">
              <tr class="border-b border-slate-800"><td class="py-2 px-3">欠員補充率</td><td class="py-2 px-3">求人理由が「欠員補充」の割合</td><td class="py-2 px-3">欠員求人数 / 全求人数 x 100</td><td class="py-2 px-3">%</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">増員率</td><td class="py-2 px-3">求人理由が「増員」の割合</td><td class="py-2 px-3">増員求人数 / 全求人数 x 100</td><td class="py-2 px-3">%</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">透明性スコア</td><td class="py-2 px-3">任意開示項目の記載率</td><td class="py-2 px-3">記載項目数 / 全任意項目数 x 100</td><td class="py-2 px-3">%</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">充足困難度</td><td class="py-2 px-3">求人条件から推定した充足の難しさ</td><td class="py-2 px-3">機械学習モデルによる予測（0-100）</td><td class="py-2 px-3">点</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">業界分散度</td><td class="py-2 px-3">HHIの正規化値</td><td class="py-2 px-3">1 - HHI/HHI_max（0-1）</td><td class="py-2 px-3">-</td></tr>
              <tr><td class="py-2 px-3">掲載期間</td><td class="py-2 px-3">受付日〜有効期限の日数</td><td class="py-2 px-3">有効期限 - 受付日</td><td class="py-2 px-3">日</td></tr>
            </tbody>
          </table>
        </div>

        <div>
          <h4 class="text-white font-semibold mb-2">外部統計データの指標</h4>
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-slate-700">
                <th class="text-left py-2 px-3 text-slate-300">指標名</th>
                <th class="text-left py-2 px-3 text-slate-300">出典</th>
                <th class="text-left py-2 px-3 text-slate-300">定義</th>
                <th class="text-left py-2 px-3 text-slate-300">注意</th>
              </tr>
            </thead>
            <tbody class="text-slate-400">
              <tr class="border-b border-slate-800"><td class="py-2 px-3">有効求人倍率</td><td class="py-2 px-3">社会・人口統計体系（総務省）</td><td class="py-2 px-3">有効求人数 / 有効求職者数</td><td class="py-2 px-3 text-amber-400">全チャネル・全産業の値</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">完全失業率</td><td class="py-2 px-3">労働力調査（総務省）</td><td class="py-2 px-3">失業者 / 労働力人口 x 100</td><td class="py-2 px-3">国勢調査ベース（5年周期）</td></tr>
              <tr class="border-b border-slate-800"><td class="py-2 px-3">入職率/離職率</td><td class="py-2 px-3">雇用動向調査（厚労省）</td><td class="py-2 px-3">入職者/離職者 / 常用労働者 x 100</td><td class="py-2 px-3">医療・福祉産業のみ表示</td></tr>
              <tr><td class="py-2 px-3">開業率/廃業率</td><td class="py-2 px-3">経済センサス（総務省）</td><td class="py-2 px-3">新設/廃業事業所 / (存続+廃業) x 100</td><td class="py-2 px-3">調査間隔（3-5年）の累計値</td></tr>
            </tbody>
          </table>
        </div>

      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: ユースケース集
fn build_section_usecases() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">🎯 ユースケース集</summary>
      <div class="mt-3 space-y-4">

        <div class="bg-slate-800/50 rounded p-3">
          <h4 class="text-white font-semibold mb-2">ケース1: 「営業先を見つけたい」（人材紹介会社向け）</h4>
          <ol class="list-decimal list-inside text-slate-400 text-sm space-y-1">
            <li><strong class="text-white">📊 地域概況</strong> で対象地域を選択し、求人件数の多い産業を確認</li>
            <li><strong class="text-white">📋 採用動向</strong> で欠員補充率が高い産業を特定 → 「人が辞めて困っている」企業が多い</li>
            <li><strong class="text-white">📈 市場分析 → 異常値・外部</strong> で入職率・離職率を確認 → HWデータの欠員率と外部統計の離職率が両方高い産業は構造的な人手不足</li>
            <li><strong class="text-white">📈 トレンド → 外部比較</strong> で有効求人倍率とHW求人数の連動を確認 → 求人倍率が上がっている地域は営業チャンス</li>
            <li><strong class="text-white">💰 求人条件</strong> で給与相場を把握 → 「相場より低い給与で出している企業」にコンサルティング提案</li>
          </ol>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <h4 class="text-white font-semibold mb-2">ケース2: 「地域の雇用政策を立案したい」（自治体向け）</h4>
          <ol class="list-decimal list-inside text-slate-400 text-sm space-y-1">
            <li><strong class="text-white">📊 地域概況</strong> で正社員比率・平均給与を全国と比較</li>
            <li><strong class="text-white">📈 市場分析 → 異常値・外部</strong> で：有効求人倍率推移、事業所動態、人口ピラミッド、介護需要推移を確認</li>
            <li><strong class="text-white">📈 トレンド</strong> で求人数・給与水準・雇用形態構成の時系列変化を確認 → 「給与は上昇傾向か」「正社員比率は変化しているか」を把握</li>
            <li><strong class="text-white">🩺 市場診断</strong> で地域の平均的な求人条件を入力し、市場での位置を確認</li>
          </ol>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <h4 class="text-white font-semibold mb-2">ケース3: 「自社の求人条件が適正か知りたい」（事業主向け）</h4>
          <ol class="list-decimal list-inside text-slate-400 text-sm space-y-1">
            <li><strong class="text-white">🩺 市場診断</strong> に自社の月給・休日・賞与を入力</li>
            <li>レーダーチャートで市場平均との比較を確認</li>
            <li>改善提案に従って条件を調整</li>
            <li><strong class="text-white">💰 求人条件</strong> で同業他社の条件分布と比較</li>
          </ol>
        </div>

      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: 外部データ出典
fn build_section_sources() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">📚 外部統計データ 出典一覧</summary>
      <div class="mt-3">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-slate-700">
              <th class="text-left py-2 px-3 text-slate-300">データ</th>
              <th class="text-left py-2 px-3 text-slate-300">出典</th>
              <th class="text-left py-2 px-3 text-slate-300">更新頻度</th>
              <th class="text-left py-2 px-3 text-slate-300">取得方法</th>
            </tr>
          </thead>
          <tbody class="text-slate-400">
            <tr class="border-b border-slate-800"><td class="py-2 px-3">人口・高齢化率</td><td class="py-2 px-3">SSDSE-A / 国勢調査（総務省）</td><td class="py-2 px-3">5年（国勢調査）</td><td class="py-2 px-3">Excel手動DL</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">転入転出</td><td class="py-2 px-3">住民基本台帳（総務省）</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">SSDSE-A</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">外国人住民</td><td class="py-2 px-3">国勢調査（総務省）</td><td class="py-2 px-3">5年</td><td class="py-2 px-3">SSDSE-A</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">昼間人口</td><td class="py-2 px-3">国勢調査 従業地・通学地集計</td><td class="py-2 px-3">5年</td><td class="py-2 px-3">SSDSE-A</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">人口ピラミッド</td><td class="py-2 px-3">国勢調査 年齢各歳別</td><td class="py-2 px-3">5年</td><td class="py-2 px-3">国勢調査×SSDSE按分推計</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">都道府県統計</td><td class="py-2 px-3">労働力調査 他</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">SSDSE-A</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">有効求人倍率</td><td class="py-2 px-3">社会・人口統計体系（総務省）</td><td class="py-2 px-3">年度次</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">賃金・労働時間</td><td class="py-2 px-3">社会・人口統計体系（総務省）</td><td class="py-2 px-3">年度次</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">産業別事業所数</td><td class="py-2 px-3">経済センサス-活動調査（総務省）</td><td class="py-2 px-3">5年</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">入職率・離職率</td><td class="py-2 px-3">雇用動向調査（厚労省）</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">消費支出</td><td class="py-2 px-3">家計調査（総務省）</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">開業率・廃業率</td><td class="py-2 px-3">経済センサス（総務省）</td><td class="py-2 px-3">3-5年</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3">気象データ</td><td class="py-2 px-3">社会・人口統計体系（総務省）</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">e-Stat API</td></tr>
            <tr><td class="py-2 px-3">介護需要</td><td class="py-2 px-3">社会・人口統計体系（総務省）</td><td class="py-2 px-3">年次</td><td class="py-2 px-3">e-Stat API</td></tr>
          </tbody>
        </table>
      </div>
    </details>
  </div>"#.to_string()
}

/// セクション: FAQ・用語集
fn build_section_faq() -> String {
    r#"<div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">❓ よくある質問（FAQ）</summary>
      <div class="mt-3 space-y-4 text-sm">

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: 有効求人倍率が厚労省の公表値と違うのですが？</p>
          <p class="text-slate-400 mt-1">A: ダッシュボードの「📈 有効求人倍率推移」は厚労省の公式統計（社会・人口統計体系経由）をそのまま表示しています。一方、他のタブの指標（充足率等）はHW掲載求人から独自に算出しています。計算方法と対象範囲が異なるため、数値は一致しません。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: 外部統計データはいつ更新されますか？</p>
          <p class="text-slate-400 mt-1">A: 外部統計データは主に年次更新です。SSDSE-Aは年1回更新、国勢調査は5年に1回です。ダッシュボード上のデータ更新時期は管理者にお問い合わせください。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: パートと正社員を分けて分析できますか？</p>
          <p class="text-slate-400 mt-1">A: はい。多くのタブでは雇用形態別（正社員/パート/その他）に分けて表示しています。外部統計データのセクションは全雇用形態の合計値で表示されます。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: 特定の企業の求人を探すにはどうすればよいですか？</p>
          <p class="text-slate-400 mt-1">A: 🔍 詳細検索タブで企業名をキーワード検索してください。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: データは最新ですか？</p>
          <p class="text-slate-400 mt-1">A: HW求人データはスクレイピング実行時点のスナップショットです。最終更新日はログイン後の画面に表示されます。外部統計データの各出典の年次はセクション末尾の出典に記載されています。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: トレンドタブは市区町村単位で見られますか？</p>
          <p class="text-slate-400 mt-1">A: いいえ。トレンド分析は都道府県単位のみ対応です。時系列データの集計構造上、市区町村レベルでの集計は行われていません。都道府県を選択してご利用ください。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: トレンドタブのデータはいつの期間ですか？</p>
          <p class="text-slate-400 mt-1">A: 約20ヶ月分のHW求人スナップショットを基に集計しています。各月次のスクレイピング結果を蓄積したもので、リアルタイムの最新データではありません。</p>
        </div>

        <div class="bg-slate-800/50 rounded p-3">
          <p class="text-white font-semibold">Q: 外部比較タブの外部データとHWデータで時間の粒度が違うのはなぜですか？</p>
          <p class="text-slate-400 mt-1">A: HWデータは月次スナップショット、外部統計は年度データです。外部統計は年度内で同じ値がステップ表示されます。これは元データの公表頻度の違いによるものです。</p>
        </div>

      </div>
    </details>
  </div>

  <div class="stat-card">
    <details>
      <summary class="text-lg font-bold text-cyan-400 cursor-pointer hover:text-cyan-300">📝 用語集</summary>
      <div class="mt-3">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-slate-700">
              <th class="text-left py-2 px-3 text-slate-300">用語</th>
              <th class="text-left py-2 px-3 text-slate-300">意味</th>
            </tr>
          </thead>
          <tbody class="text-slate-400">
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">HW</td><td class="py-2 px-3">ハローワーク（公共職業安定所）</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">有効求人</td><td class="py-2 px-3">掲載中で有効期限内の求人</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">欠員補充</td><td class="py-2 px-3">退職者の後任として募集する求人</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">増員</td><td class="py-2 px-3">事業拡大のために新たに募集する求人</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">HHI</td><td class="py-2 px-3">ハーフィンダール・ハーシュマン指数（市場集中度の指標）</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">SSDSE</td><td class="py-2 px-3">教育用標準データセット（統計センター提供）</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">e-Stat</td><td class="py-2 px-3">政府統計の総合窓口（API経由でデータ取得）</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">Turso</td><td class="py-2 px-3">クラウドデータベース（外部統計データの格納先）</td></tr>
            <tr class="border-b border-slate-800"><td class="py-2 px-3 text-white">スナップショット</td><td class="py-2 px-3">ある時点での全有効求人の一括取得データ</td></tr>
            <tr><td class="py-2 px-3 text-white">複合キー</td><td class="py-2 px-3">事業所番号+職業分類+雇用形態で「同じ募集」を追跡する識別子</td></tr>
          </tbody>
        </table>
      </div>
    </details>
  </div>"#.to_string()
}
