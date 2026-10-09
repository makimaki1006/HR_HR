# HR_HR CRM Frames (社内用 Chrome 拡張)

CRM 架電画面 (`https://hr-hw.onrender.com/app/crm`) 右側の「求人検索・リンク先」iframe で、
`X-Frame-Options` / CSP `frame-ancestors` によりフレーム表示を拒否するサイト (例: jp.indeed.com の「接続が拒否されました」) を表示できるようにする。

## 仕組み

- ブロックリスト方式。CRM タブの中のフレームでは、リストにあるドメイン以外すべてのサイトのフレーム拒否ヘッダを外す。
- CRM タブ = トップレベル URL が アプリのオリジン (既定: `https://hr-hw.onrender.com` / `http://localhost:9216` / `http://127.0.0.1:9216`) かつパスが `/app/crm` または `/app/crm/` 配下のタブ。
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
| `scripting` | 管理者が `appOrigins` で既定外のオリジンを指定した場合だけ、そのオリジンにコンテンツスクリプトを動的登録する |

`tabs` / `webNavigation` 権限は使わない (`<all_urls>` の host 権限でタブの URL が読めるため不要)。リモートコード・外部通信・解析ツールは無い。拡張自身はネットワークを呼ばない。

## セキュリティ上のトレードオフ

- DNR はヘッダの値を編集できず削除のみなので、`content-security-policy` は丸ごと消える。**CRM タブ内でフレームに入れたサイトは、そのサイト自身の CSP (script-src 等) が無効**になる。他のタブ・通常の閲覧には影響しない。
- フレームされたサイトは `X-Frame-Options` の保護 (クリックジャッキング対策) を失う。ログイン・決済・認証系はブロックリストで除外している。リストに無いサイトへ CRM 内でログイン情報を入力しない運用とする。
- アプリ自身 (同一ホスト) と Zoom Smart Embed (`zoom.us`) は触らない。
- ブロックリストの追加・削除は管理者が把握できるよう、運用では managed ポリシーを推奨。

## インストール

### 開発・試用 (パッケージ化されていない拡張)
1. `chrome://extensions` を開き「デベロッパー モード」を ON。
2. 「パッケージ化されていない拡張機能を読み込む」でこの `extensions/hrhr-crm-frames/` を選ぶ。

### パッケージ (zip)
```
cd extensions/hrhr-crm-frames
zip -r ../hrhr-crm-frames.zip manifest.json managed_schema.json background.js content.js popup.* options.* lib README.md
```
(`test/` と `package.json` は含めない。)

### Google Workspace 強制インストール
1. Chrome ウェブストアの「限定公開」(非公開リスト、ドメイン内限定) で zip を公開し、拡張 ID を得る。または自社ホストの `update.xml` + `.crx` を使う。
2. 管理コンソール → デバイス → Chrome → アプリと拡張機能 → ユーザーとブラウザ → 対象の組織部門を選択。
3. 追加 → 拡張機能 ID を追加 (セルフホストなら「URL から追加」) → インストールポリシーを「強制インストール」。
4. (任意) 同画面の「ポリシー」にスキーマ (`managed_schema.json`) に沿った JSON を貼る。例:
   ```json
   { "blocklist": { "Value": ["accounts.google.com", "zoom.us", "hubspot.com"] } }
   ```
   `appOrigins` も同様。

## ブロックリストの管理

- 既定: `accounts.google.com`, `accounts.youtube.com`, `login.microsoftonline.com`, `login.live.com`, `okta.com`, `auth0.com`, `appleid.apple.com`, `hubspot.com`, `hubspot.jp`, `hubapi.com`, `paypal.com`, `stripe.com`, `zoom.us` (サブドメインを含む)。アプリのオリジンのホストは常に除外される。
- **sync**: 拡張のオプション画面で編集 (1 行 1 ドメイン、`#` はコメント、URL・パス・ポート・ワイルドカードは不可。不正な行があると保存されず、行番号付きのエラーが出る)。利用者ごとの Chrome プロファイルに同期される。
- **managed (管理者ポリシー)**: `blocklist` / `appOrigins` が設定されていれば sync より優先。オプション画面は読み取り専用になる。

## 動作確認

1. 拡張を入れて `https://hr-hw.onrender.com/app/crm` を開く。拡張のポップアップに「有効 (この CRM タブ)」。
2. DevTools で `document.documentElement.dataset.hrhrFrames` がバージョン (`1.0.0`) を返す。
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
