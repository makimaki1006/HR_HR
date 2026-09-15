/**
 * 段 2: 実ページの DOM から集めたクラスを、そのページに読み込まれている
 * 生 CSS と突き合わせる。
 *
 * ■ なぜ段 1 (tests/css_classes_exist.rs) だけでは足りないか
 *   段 1 はソースの文字列リテラルしか見られない。
 *     * `format!("{}", if x { "a" } else { "b" })` の条件組み立て
 *     * テンプレートと JS が付けるクラス
 *     * 「定義はどこかにあるが、このページには読み込まれていない」CSS
 *   このあたりが落ちる。実際に描画された DOM を読めば全部まとめて分かる。
 *
 * ■ なぜ CSS 側からクラス名を「抽出」しないか
 *   document.styleSheets を舐める方式は @media の入れ子や
 *   `.hover\:x:hover` のような擬似クラス付きセレクタで取りこぼす。
 *   それで 10 件ほど誤検出した実績がある。
 *   生テキストに対してトークンごとに検索するのが確実。
 *
 * ■ 使い方
 *   BASE_URL=http://127.0.0.1:8080 \
 *   E2E_EMAIL=... E2E_PASS=... \
 *   node scripts/audit_css_classes.js
 *
 * ■ 終了コード
 *   0 … 未定義クラスなし
 *   1 … 未定義クラスあり（または許可リストに不要な行が残っている）= 検査の結果
 *   2 … 検査が成立しなかった = 環境の問題
 *       設定漏れ・ログイン失敗・サーバに届かない・CSS が取れない・照合が壊れている
 *
 *   1 と 2 を分けているのは、CI が赤くなったときに
 *   「未定義クラスが増えた」のか「環境変数の入れ忘れ」なのかを
 *   終了コードだけで見分けるため。2 は検査の結論ではないので、
 *   「未定義 0 件」と同じ扱いにしてはいけない（見逃しになる）。
 *
 * ■ 検査が働いていることの確認（逆証明）
 *   AUDIT_CANARY=1 を付けると、CSS に絶対に無いクラスを 1 つ DOM の集計に
 *   混ぜ、それが「未定義」として報告されるかだけを見る。
 *   終了コード 0 = 検出できた（検査は働いている） / 1 = 見逃した（壊れている）。
 *   このモードでは実際の未定義クラスは表示するだけで、終了コードに含めない。
 */
const fs = require('fs');
const path = require('path');
// playwright はここでは読み込まない。照合部分だけを require して試せるように
// するため（scripts/audit_css_classes.selftest.js）。監査の入口で読む。

const BASE = process.env.BASE_URL || 'http://127.0.0.1:8080';
const EMAIL = process.env.E2E_EMAIL || '';
const PASS = process.env.E2E_PASS || '';
const ALLOWLIST = path.join(__dirname, '..', 'tests', 'css_class_allowlist.txt');
const GENERATED_MARKER =
  '# ---- ここから自動生成（着手前から在る分）。手で編集しない ----';

// 4 面 × 2 県。全国と小さい県で、データが薄いときにだけ出る枝も通る。
const VIEWS = (process.env.AUDIT_VIEWS || 'overview,titles,industry,people').split(',');
const PREFS = (process.env.AUDIT_PREFS || ',鳥取県').split(',');

const EXIT_OK = 0;
const EXIT_FINDINGS = 1;
const EXIT_CANNOT_RUN = 2;

/** 検査が成立しなかったときに投げる。終了コード 2 になる。 */
class CannotRun extends Error {
  constructor(msg, hint) {
    super(msg);
    this.name = 'CannotRun';
    this.hint = hint || '';
  }
}

/**
 * トークン名 -> 生 CSS を検索する正規表現。
 *
 * Tailwind は特殊文字を \ でエスケープして出力する:
 *   bg-blue-500/20   -> .bg-blue-500\/20
 *   hover:text-white -> .hover\:text-white:hover
 *   p-1.5            -> .p-1\.5
 * なので特殊文字の前に \ が「あってもなくても」拾う形にする。
 * 直後に [\w-] が来ないことも見る（bg-blue-5 が bg-blue-500 に誤マッチしない）。
 *
 * ■ 直後の `\` も拒む理由（2026-09-11 修正）
 *   `.bg-amber-500\/10` は bg-amber-500/10 の定義であって bg-amber-500 の
 *   定義ではない。`(?![\w-])` だけだと次の文字が `\` なので通ってしまい、
 *   bg-amber-500 / text-teal-300 など 20 種以上を「定義済み」と誤判定していた。
 *   一方 `.hover\:text-white:hover` の末尾の `:` は擬似クラスの始まりなので、
 *   エスケープされていない記号は終端として認める。
 */
function tokenPattern(tok) {
  const body = tok
    .split('')
    .map((ch) =>
      /[a-zA-Z0-9-]/.test(ch)
        ? ch
        : '\\\\?' + ch.replace(/[.*+?^${}()|[\]\\/]/g, '\\$&')
    )
    .join('');
  return new RegExp('\\.' + body + '(?![\\w-\\\\])');
}

/**
 * 許可リストを読む。
 *
 * 段 2 は DOM のクラスをソースのファイルに結び付けられないので、
 * `<クラス名> <ファイル>` 形式の行も**クラス名だけ**で効かせる。
 * ここは段 1 より緩い。段 2 の役目は「段 1 に見えないものを見つける」ことで、
 * 既知の債務を数え直すことではない。
 *
 * 手書き部分（ファイル指定の無い行）は段 2 が後始末を受け持つ。
 * 段 1 の視界外（テンプレート・JS 側）のクラスがそこに入るため、
 * 段 1 では「直ったかどうか」を判定できない。
 */
function loadAllowlist() {
  if (!fs.existsSync(ALLOWLIST)) return { all: new Set(), manual: new Set() };
  const text = fs.readFileSync(ALLOWLIST, 'utf8');
  const at = text.indexOf(GENERATED_MARKER);
  const head = at >= 0 ? text.slice(0, at) : text;
  const tail = at >= 0 ? text.slice(at + GENERATED_MARKER.length) : '';
  const parse = (s) =>
    s
      .split('\n')
      .map((l) => l.split('#')[0].trim())
      .filter(Boolean)
      .map((l) => l.split(/\s+/)[0]);
  const manual = new Set(parse(head));
  const all = new Set([...manual, ...parse(tail)]);
  return { all, manual };
}

async function login(p) {
  try {
    await p.goto(BASE + '/login', { waitUntil: 'domcontentloaded', timeout: 60000 });
  } catch (e) {
    throw new CannotRun(
      BASE + ' に届きません: ' + e.message,
      'BASE_URL を確認するか、アプリを起動してください。'
    );
  }
  await p.fill('input[name="email"]', EMAIL);
  await p.fill('input[name="password"]', PASS);
  await Promise.all([
    p.waitForNavigation().catch(() => {}),
    p.click('button[type="submit"]'),
  ]);
  // ログインできたか。失敗すると /login に留まり、以降の DOM が空になる。
  // そのまま進むと「未定義 0 件」で緑になってしまうので、ここで止める
  if (/\/login(\?|$)/.test(p.url())) {
    throw new CannotRun(
      'ログインできませんでした（/login に留まっています）。',
      'E2E_EMAIL / E2E_PASS の値を確認してください。'
    );
  }
}

// ブラウザを起こさずに照合そのものを試せるように出しておく。
// scripts/audit_css_classes.selftest.js がこれを使う。
module.exports = { tokenPattern, loadAllowlist };

// require された場合はここまで。監査は直接起動したときだけ走らせる
if (require.main !== module) return;

/**
 * playwright の置き場は環境で変わる。単体で入っていればそれを、
 * 無ければ @playwright/test が同じ `chromium` を出しているのでそちらを使う。
 */
function loadChromium() {
  for (const name of ['playwright', '@playwright/test']) {
    try {
      return require(name).chromium;
    } catch (e) {
      /* 次を試す */
    }
  }
  throw new CannotRun('playwright が見つかりません。', 'npm install を先に実行してください。');
}

(async () => {
  // 設定漏れは検査を始める前に弾く。ブラウザを起こしてから落ちると
  // ログの後ろの方に紛れて原因が読み取りにくい
  if (!EMAIL || !PASS) {
    throw new CannotRun(
      'E2E_EMAIL / E2E_PASS が未設定です。ログインできないと DOM を読めません。',
      '未設定のまま「未定義 0 件」にはできません（見逃しになります）。'
    );
  }

  const chromium = loadChromium();
  const { all: allow, manual } = loadAllowlist();
  // 手元は実 Chrome（プロファイル込みで見たいことがある）、CI は同梱 Chromium。
  // AUDIT_BROWSER_CHANNEL='' を渡すと同梱版になる
  const channel =
    process.env.AUDIT_BROWSER_CHANNEL === undefined
      ? 'chrome'
      : process.env.AUDIT_BROWSER_CHANNEL;
  const b = await chromium.launch(
    channel ? { channel, headless: true } : { headless: true }
  );
  const ctx = await b.newContext({ viewport: { width: 1280, height: 1000 } });
  const p = await ctx.newPage();

  await login(p);
  await p.goto(BASE + '/?tab=%2Ftab%2Findeed', { waitUntil: 'networkidle', timeout: 90000 });
  await p.waitForTimeout(3000);

  // 生 CSS を 1 本に。link で読んでいるものと、ページに埋め込まれた <style>
  const css = await p.evaluate(async () => {
    let t = '';
    for (const l of [...document.querySelectorAll('link[rel="stylesheet"]')]) {
      try {
        t += '\n' + (await (await fetch(l.href)).text());
      } catch (e) {
        /* 外部ホストは読めなくてよい */
      }
    }
    document.querySelectorAll('style').forEach((s) => {
      t += '\n' + s.textContent;
    });
    return t;
  });
  console.log('CSS 総量: ' + css.length + ' bytes');
  if (css.length < 10000) {
    await b.close();
    throw new CannotRun(
      'CSS がほとんど取れていません (' + css.length + ' bytes)。',
      '照合の土台が無いので中断します。ページが正しく描画されているか確認してください。'
    );
  }

  // 全面のクラストークンを集める
  const used = new Map();
  const collect = async (label) => {
    const toks = await p.evaluate(() => {
      const m = {};
      document.querySelectorAll('*').forEach((el) => {
        const cn = el.getAttribute('class');
        if (!cn) return;
        cn.trim()
          .split(/\s+/)
          .forEach((t) => {
            if (!t) return;
            if (!m[t]) m[t] = [];
            if (m[t].length < 2) {
              m[t].push(
                el.tagName.toLowerCase() +
                  '「' +
                  (el.textContent || '').trim().slice(0, 16) +
                  '」'
              );
            }
          });
      });
      return m;
    });
    Object.entries(toks).forEach(([t, where]) => {
      if (!used.has(t)) used.set(t, { where, views: new Set() });
      used.get(t).views.add(label);
    });
  };

  await collect('shell');
  for (const pref of PREFS) {
    for (const v of VIEWS) {
      const url =
        BASE + '/tab/indeed?view=' + v + (pref ? '&pref=' + encodeURIComponent(pref) : '');
      await p.goto(url, { waitUntil: 'networkidle', timeout: 90000 });
      await p.waitForTimeout(2000);
      await collect((pref || '全国') + '/' + v);
    }
  }
  await b.close();

  // ---- 照合が働いていることを先に確かめる（逆証明） ----------------------
  // 何も拾えなくなっても「未定義 0 件」で緑になってしまう。
  // 実在しないクラスが「無い」と、実在するクラスが「ある」と言えることを見る。
  const sane =
    !tokenPattern('text-zzz-999').test(css) && tokenPattern('text-slate-400').test(css);
  if (!sane) {
    throw new CannotRun(
      '照合が壊れています（実在しないクラスを「ある」と言うか、実在するクラスを「無い」と言っています）。',
      'scripts/audit_css_classes.selftest.js を実行して、どの条件で壊れているか確認してください。'
    );
  }

  // 1 クラスも集まっていないなら、ページが空だったということ。
  // 「未定義 0 件」と区別が付かないので検査不成立として止める
  if (used.size < 20) {
    throw new CannotRun(
      'DOM から集まったクラスが ' + used.size + ' 種しかありません。',
      'ページが描画されていない可能性があります。未定義 0 件とは区別が必要です。'
    );
  }

  // ---- 逆証明用のおとり --------------------------------------------------
  // DOM から集めた結果に、CSS に絶対に無いクラスを 1 つ混ぜる。
  // これが報告されなければ、検出の経路そのものが働いていない
  const CANARY = 'text-zzz-canary-999';
  const canaryMode = process.env.AUDIT_CANARY === '1';
  if (canaryMode) {
    used.set(CANARY, { where: ['(逆証明用のおとり)'], views: new Set(['canary']) });
  }

  // ---- 未定義クラス ------------------------------------------------------
  const missing = [];
  used.forEach((v, tok) => {
    if (!tokenPattern(tok).test(css)) missing.push({ tok, ...v });
  });
  const news = missing.filter((m) => !allow.has(m.tok));

  console.log('使用クラス ' + used.size + ' 種 / 未定義 ' + missing.length + ' 件');
  console.log('  うち許可リストに無いもの: ' + news.length + ' 件');
  console.log('');

  // 色系ファミリの在庫。置換先がその場で決まるように必ず出す
  const fams = new Set();
  (css.match(/\.(?:text|bg|border|from|to|via|ring|divide)-([a-z]+)-\d{2,3}/g) || []).forEach(
    (s) =>
      fams.add(
        s.replace(/^\.(?:text|bg|border|from|to|via|ring|divide)-/, '').replace(/-\d+$/, '')
      )
  );
  const famList = [...fams].sort();

  if (news.length) {
    const badFams = new Set();
    news.forEach((m) => {
      const mm = m.tok.match(
        /^(?:[a-z0-9-]+:)*(?:text|bg|border|from|to|via|ring|divide)-([a-z]+)-\d{2,3}/
      );
      if (mm && !fams.has(mm[1])) badFams.add(mm[1]);
    });
    if (badFams.size) {
      console.log('CSS に無い色名 : ' + [...badFams].sort().join(', '));
      console.log('使える色名     : ' + famList.join(', '));
      console.log('');
    }
    news
      .sort((a, c) => a.tok.localeCompare(c.tok))
      .forEach((m) => {
        console.log(
          '  ' +
            m.tok.padEnd(26) +
            ' [' +
            [...m.views].slice(0, 3).join(',') +
            ']  ' +
            m.where.join(' / ')
        );
      });
    console.log('');
  }

  // ---- 許可リストの手書き部分の後始末 ------------------------------------
  const stillMissing = new Set(missing.map((m) => m.tok));
  const stale = [...manual].filter((t) => !stillMissing.has(t));
  if (stale.length) {
    console.log('許可リストの手書き部分に不要な行が残っています（直ったのに消えていない）:');
    stale.forEach((t) => console.log('  ' + t));
    console.log('  -> tests/css_class_allowlist.txt の該当行を消してください。');
    console.log('');
  }

  // ---- おとりモードの判定 -------------------------------------------------
  // 実際の未定義クラスの有無ではなく、「おとりを検出できたか」だけを返す
  if (canaryMode) {
    const caught = news.some((m) => m.tok === CANARY);
    console.log(
      caught
        ? 'CANARY: 検出しました。検査は働いています。'
        : 'CANARY: 見逃しました。検出の経路が壊れています。'
    );
    process.exit(caught ? EXIT_OK : EXIT_FINDINGS);
  }

  if (!news.length && !stale.length) {
    console.log('未定義クラスなし。許可リストにも不要な行はありません。');
    console.log('CSS にある色名: ' + famList.join(', '));
    process.exit(EXIT_OK);
  }
  process.exit(EXIT_FINDINGS);
})().catch((e) => {
  // ここに来るのは「検査が成立しなかった」場合だけ。
  // 未定義クラスが見つかった場合は上で exit 1 しており、例外にはならない。
  // タイムアウトやセレクタ不一致など想定外のものも、結論を出せていない以上
  // 「未定義 0 件」ではないので 2 にまとめる
  if (e instanceof CannotRun) {
    console.error('検査を実行できませんでした: ' + e.message);
    if (e.hint) console.error('  ' + e.hint);
  } else {
    console.error('検査の途中で想定外のエラーが出ました（結論は出ていません）:');
    console.error(e);
  }
  console.error('  終了コード 2 = 環境の問題。1 = 未定義クラスあり、とは別物です。');
  process.exit(EXIT_CANNOT_RUN);
});
