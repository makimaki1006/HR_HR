# HR_HR CRM Frames (社内用 Chrome 拡張)

CRM 架電画面 (`https://hr-hw.onrender.com/app/crm`) 右側の「求人検索・リンク先」iframe で、
`X-Frame-Options` / CSP `frame-ancestors` によりフレーム表示を拒否するサイト (例: jp.indeed.com の「接続が拒否されました」) を表示できるようにする。

## 仕組み

- ブロックリスト方式。CRM タブの中のフレームでは、リストにあるドメイン以外すべてのサイトのフレーム拒否ヘッダを外す。
- CRM タブ = トップレベル URL が アプリのオリジン (固定: `https://hr-hw.onrender.com` / `http://localhost:9216` / `http://127.0.0.1:9216`) かつパスが `/app/crm` または `/app/crm/` 配下のタブ。
- `declarativeNetRequest` のセッションルールを 1 本だけ持つ。
  - `condition.tabIds` = 現在の CRM タブの ID、`resourceTypes: ["sub_frame"]`、`excludedRequestDomains` = ブロックリスト + アプリ自身のホスト名
  - `action` = レスポンスヘッダ `x-frame-options` と `content-security-policy` を削除
- iframe 内のリンク遷移 (Google の検索結果クリック等) は initiator が google.com になるため `initiatorDomains` には頼らず、`tabIds` で絞っている。
- タブの URL 変更・クローズ・作成、設定変更、Service Worker 起動のたびに、全タブを調べてルールを作り直す (イベントの取りこぼしに強い)。CRM タブが無ければルールも無い。
- 注意: ルールの反映は CRM 画面の表示後 (非同期)。画面表示と同時に読み込む iframe では、ごく稀に初回だけ拒否されうる。検索して開く通常の使い方では問題ない。
- コンテンツスクリプトはアプリのオリジンのみ。`document.documentElement.dataset.hrhrFrames` に拡張のバージョンを書く (アプリ側との契約はこれだけ)。

## 権限と理由

| 権限 | 理由 |
|---|---|
| `declarativeNetRequestWithHostAccess` | レスポンスヘッダの書き換え (`modifyHeaders`)。`declarativeNetRequest` より狭く、host 権限のあるサイトにだけ作用する |
| `host_permissions: <all_urls>` | 書き換え対象は任意の外部サイト (Indeed、求人媒体、企業サイト等) で、事前に列挙できないため。`modifyHeaders` は書き換え先 URL と initiator の host 権限が要る。**実際の作用範囲は `tabIds` + `sub_frame` + ブロックリストで限定**している。副次的に、タブの URL (CRM タブの判定) も読める |
| `storage` | ブロックリスト (`storage.sync`) と管理者ポリシー (`storage.managed`) の読み書き |

`tabs` / `webNavigation` 権限は使わない (`<all_urls>` の host 権限でタブの URL が読めるため不要)。リモートコード・外部通信・解析ツールは無い。拡張自身はネットワークを呼ばない。

## セキュリティ上のトレードオフ

- DNR はヘッダの値を編集できず削除のみなので、`content-security-policy` は丸ごと消える。**CRM タブ内でフレームに入れたサイトは、そのサイト自身の CSP (script-src 等) が無効**になる。他のタブ・通常の閲覧には影響しない。
- フレームされたサイトは `X-Frame-Options` の保護 (クリックジャッキング対策) を失う。ログイン・決済・認証系はブロックリストで除外している。リストに無いサイトへ CRM 内でログイン情報を入力しない運用とする。
- アプリ自身 (同一ホスト) は触らない。Zoom Smart Embed (`applications.zoom.us`) は元から埋め込みを許可しているので、CRM タブ内でヘッダを外されても動作は変わらない (Zoom は既定のブロックリストに入れていない)。
- ブロックリストの追加・削除は管理者が把握できるよう、運用では managed ポリシーを推奨。

## インストール

### 開発・試用 (パッケージ化されていない拡張)
1. `chrome://extensions` を開き「デベロッパー モード」を ON。
2. 「パッケージ化されていない拡張機能を読み込む」でこの `extensions/hrhr-crm-frames/` を選ぶ。

### パッケージ (zip) の作成
```
bash extensions/hrhr-crm-frames/pack.sh   # dist/hrhr-crm-frames-<version>.zip (dist/ はコミットしない)
```
`test/`・`package.json` は含まれない。配布と導入の手順は次節。

## ZIP での配布と導入

Chrome ウェブストアには載せず、ZIP を配って各自の Chrome に入れる運用。

### 導入
1. 受け取った `hrhr-crm-frames-<version>.zip` を、**消さない固定のフォルダ**に展開する (例: `C:\Users\<自分>\hrhr-crm-frames`)。展開後にフォルダを移動・削除すると拡張が無効になるので、置き場所は最初に決める。
2. Chrome で `chrome://extensions` を開き、右上の「デベロッパー モード」を ON にする。
3. 「パッケージ化されていない拡張機能を読み込む」を押し、展開したフォルダ (中に `manifest.json` がある階層) を選ぶ。
4. 確認: `https://hr-hw.onrender.com/app/crm` を開き、案件を選んで右の「求人検索・リンク先」でリンクを開くと、リンク操作行に「拡張機能: 有効」と出る。拡張アイコンのポップアップは「有効 (この CRM タブ)」になる。

### 起動時の警告について
デベロッパー モードの拡張は、Chrome の起動時に「デベロッパー モードの拡張機能を無効にしてください」という警告が出ることがある。**「キャンセル」を押せば拡張は有効のまま**。「無効にする」を押した場合は、`chrome://extensions` で拡張のスイッチを ON に戻す。

### 更新
1. 新しい ZIP を、**今の拡張と同じフォルダの中身に上書き展開**する (フォルダ自体は変えない)。
2. `chrome://extensions` で本拡張の「再読み込み」(丸い矢印) ボタンを押す。
3. バージョン (`1.0.1` 等) が上がったことを確認する。

### 削除
`chrome://extensions` で本拡張の「削除」を押し、展開したフォルダも削除する。設定したブロックリストは Chrome の同期データに残る場合がある (影響はない)。

### 問い合わせ
導入できない・動かない場合は s_fujimaki@f-a-c.co.jp まで。

### Google Workspace 強制インストール
1. (ZIP 配布とは別の将来案) 限定公開のストア掲載、または自社ホストの `update.xml` + `.crx` で拡張 ID を得る。
2. 管理コンソール → デバイス → Chrome → アプリと拡張機能 → ユーザーとブラウザ → 対象の組織部門を選択。
3. 追加 → 拡張機能 ID を追加 (セルフホストなら「URL から追加」) → インストールポリシーを「強制インストール」。
4. (任意) 同画面の「ポリシー」にスキーマ (`managed_schema.json`) に沿った JSON を貼る。例:
   ```json
   { "blocklist": { "Value": ["accounts.google.com", "login.microsoftonline.com", "stripe.com"] } }
   ```
   (対象オリジンは固定。管理者ポリシーで変更する項目は `blocklist` のみ。)

## 運用方針

- この拡張は意図的に最小限にしてある。更新するたびに ZIP を再配布して各自が入れ直す必要があるため、**変更は稀**にする。
- 新しい機能は拡張ではなく HR_HR のアプリ側に入れる。拡張は「CRM 画面の枠でフレーム拒否ヘッダを外す」ことだけを担当する。
- 既定のブロックリスト (サインイン・認証ページ、決済) は安全側の初期値として拡張に組み込んである。通常は編集しなくてよい。
- HubSpot と Zoom は既定のブロックリストに入れていない (CRM の枠の中で普通に使える)。HubSpot を枠の中で開くとき、Chrome のサードパーティ Cookie 制限により枠内ではログイン状態が引き継がれず、ログイン画面が繰り返される場合がある。その場合は新しいタブで開く。

## ブロックリストの管理

- 既定: `accounts.google.com`, `accounts.youtube.com`, `login.microsoftonline.com`, `login.live.com`, `okta.com`, `auth0.com`, `appleid.apple.com`, `paypal.com`, `stripe.com` (サブドメインを含む。サインイン・決済のみ)。アプリのオリジンのホストは常に除外される。
- **sync**: 拡張のオプション画面で編集 (1 行 1 ドメイン、`#` はコメント、URL・パス・ポート・ワイルドカードは不可。不正な行があると保存されず、行番号付きのエラーが出る)。利用者ごとの Chrome プロファイルに同期される。
- **managed (管理者ポリシー)**: `blocklist` が設定されていれば sync より優先。オプション画面は読み取り専用になる。

## 動作確認

1. 拡張を入れて `https://hr-hw.onrender.com/app/crm` を開く。拡張のポップアップに「有効 (この CRM タブ)」。
2. DevTools で `document.documentElement.dataset.hrhrFrames` がバージョン (`1.0.1`) を返す。
3. 右側 iframe で jp.indeed.com 等を開く → 拒否されず表示される。
4. 同じサイトを別タブの普通のページで開いても挙動は変わらない (拡張は何もしない)。ポップアップは「対象外」。
5. `chrome://extensions` → 本拡張の「Service Worker」 → コンソールで
   `chrome.declarativeNetRequest.getSessionRules()` が CRM タブを開いている間だけ 1 件返る。
6. 自動テスト (リポジトリ直下で `npm ci` 済み):
   ```
   cd extensions/hrhr-crm-frames
   npm test            # 純粋ロジックの単体テスト (node --test, 依存なし)
   npm run test:e2e    # 実 Chromium (新ヘッドレス) で拡張を読み込む E2E。127.0.0.1:9216 と :9301 を使う
   ```

## アンインストール

`chrome://extensions` で削除。強制インストールの場合は管理コンソールのポリシーから外す。セッションルールはブラウザ終了・拡張削除で消え、残らない。
