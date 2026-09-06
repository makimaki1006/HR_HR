/**
 * 月次の系列から「傾き」と「ばらつき」を分けて測る。
 *
 * ■ 直した理由（2026-08-21）
 *   それまでは「月ごとの変化率の中央値」をばらつきとして使い、
 *   最初と最後の変化がその何倍かで判定していた。これは筋が悪い。
 *   毎月きれいに 5% ずつ下がっている職種は、変化率の中央値が 5% になるので
 *   「ばらつきが大きい」と判定されてしまう。傾向がはっきりしているほど
 *   信用できない扱いになる、逆の結果になっていた。
 *
 * ■ 直したあとの考え方
 *   系列に直線を当てて、
 *     傾き   … 1 か月あたり何 % 動いているか
 *     ばらつき … その直線からどれだけ散らばっているか
 *   を別々に出す。傾きがばらつきに対して十分に大きければ「傾向がある」と見なす。
 *   一直線に下がっていればばらつきは小さくなるので、正しく「はっきりした傾向」になる。
 *
 *   対数をとってから直線を当てている。求人数のような正の量は
 *   「毎月 5% ずつ」のように掛け算で動くので、そのほうが実態に合う。
 *
 * ■ 言い過ぎないための注意
 *   月次データは前の月と無関係ではない（先月多ければ今月も多い）ので、
 *   統計の検定としては厳密ではない。p 値のような言い方はしない。
 *   「ばらつきに対して傾きが何倍か」という目安として扱う。
 */
'use strict';

/**
 * @param {Array<number|null>} values 月次の値（欠測は null）
 * @returns {null|{
 *   slopePct:number, totalPct:number, scatterPct:number, ratio:number,
 *   level:'はっきりした傾向'|'緩やかな傾向'|'傾向とは言えない',
 *   n:number, fitted:Array<number|null>
 * }}
 */
function fitTrend(values) {
  const pts = [];
  values.forEach((v, i) => {
    if (v != null && v > 0 && isFinite(v)) pts.push({ x: i, y: Math.log(v) });
  });
  if (pts.length < 4) return null;

  const n = pts.length;
  const mx = pts.reduce((a, p) => a + p.x, 0) / n;
  const my = pts.reduce((a, p) => a + p.y, 0) / n;
  let sxx = 0;
  let sxy = 0;
  for (const p of pts) { sxx += (p.x - mx) * (p.x - mx); sxy += (p.x - mx) * (p.y - my); }
  if (!sxx) return null;

  const slope = sxy / sxx;                 // 対数の傾き = 1 期あたりの増加率（対数）
  const intercept = my - slope * mx;

  // 直線からの散らばり
  let ss = 0;
  for (const p of pts) {
    const e = p.y - (intercept + slope * p.x);
    ss += e * e;
  }
  const dof = n - 2;
  const sigma = dof > 0 ? Math.sqrt(ss / dof) : 0;
  const seSlope = sigma / Math.sqrt(sxx);
  const ratio = seSlope ? Math.abs(slope) / seSlope : (slope ? Infinity : 0);

  const span = values.length - 1;

  // 直線から大きく外れた月を探す。1 点が跳ねただけで傾きが立つことがある。
  // 実例: 和歌山県のトラックドライバーは 2026-03 だけ跳ねており、
  // 「毎月 6.3% ずつ増えています」と書くと実態と食い違う（2026-08-31 に指摘された）。
  // 当てはまりが完璧なとき、散らばりは計算上のごみ（1e-16 程度）になる。
  // そのまま「散らばり何個分か」を測るとごみ同士の割り算になり、
  // 実装の最後の桁が違うだけで外れ月が出たり出なかったりする（2026-08-31 Rust 移植で発覚）。
  // 表示に出ない水準の散らばりでは、外れ月を判定しない。
  const SIGMA_EPS = 1e-9;
  const sigmaLog = sigma;
  const outliers = [];
  pts.forEach((pt) => {
    const resid = pt.y - (intercept + slope * pt.x);
    if (sigmaLog > SIGMA_EPS && Math.abs(resid) / sigmaLog >= 1.8) {
      outliers.push({ index: pt.x, resid, ratio: resid / sigmaLog });
    }
  });
  outliers.sort((a, b) => Math.abs(b.ratio) - Math.abs(a.ratio));

  // 月ごとの散らばりが、1 か月あたりの動きに比べて大きいかどうか。
  // 大きければ「毎月 X% ずつ」という言い方はできない（一本調子ではない）。
  const slopePctAbs = Math.abs((Math.exp(slope) - 1) * 100);
  const scatterPctVal = (Math.exp(sigma) - 1) * 100;
  const steady = slopePctAbs > 0 && scatterPctVal <= slopePctAbs * 2;

  const level = ratio >= 3 ? 'はっきりした傾向'
    : ratio >= 1.8 ? '緩やかな傾向' : '傾向とは言えない';

  return {
    steady,
    outliers,
    // 1 か月あたり何 %
    slopePct: (Math.exp(slope) - 1) * 100,
    // 期間全体で何 %（直線に沿った変化。端の月のブレに引きずられない）
    totalPct: (Math.exp(slope * span) - 1) * 100,
    // 直線からの散らばり（±何 %）
    scatterPct: (Math.exp(sigma) - 1) * 100,
    ratio,
    level,
    n,
    fitted: values.map((v, i) => (v == null ? null : Math.exp(intercept + slope * i))),
  };
}

/**
 * 画面に出す言い方。読むのは採用の担当者や営業で、統計の人ではない。
 * 「傾き」「ばらつき」「有意」といった言葉は使わず、起きていることをそのまま書く。
 */
function trendLabel(f) {
  if (!f) return 'データ不足';
  const up = f.slopePct > 0;
  if (f.level === '傾向とは言えない') return '月ごとにばらつく';
  // 全体としては動いているが、月ごとの振れが大きく一本調子ではない場合。
  // 「緩やかに増加」と書くと、毎月少しずつ増えていると誤解される。
  if (!f.steady) return up ? '振れながら増えた' : '振れながら減った';
  if (f.level === '緩やかな傾向') return up ? '緩やかに増加' : '緩やかに減少';
  return up ? '増え続けている' : '減り続けている';
}

/**
 * 一文での説明。数字は残しつつ、専門用語を避ける。
 *
 * ■ 直した点（2026-08-31）
 *   以前は振れの大きさに関わらず「月ごとの上下は X% ほどなので、増え続けていると
 *   見てよさそうです」と書いていた。上下が 44% あっても同じ文が出ており、
 *   振れの大きさを結論の根拠にしてしまっていた。逆である。
 *   振れが大きいときは「一本調子ではない」と書き、跳ねた月があればその月を示す。
 */
function describeTrend(f, what, months) {
  const name = what || 'この数値';
  if (!f) return `${name}は、比べられるだけの月数がありません。`;
  const up = f.slopePct > 0;
  const per = Math.abs(f.slopePct).toFixed(1);
  const tot = Math.abs(f.totalPct).toFixed(0);
  const sc = f.scatterPct.toFixed(0);

  if (f.level === '傾向とは言えない') {
    return `${name}は月ごとに ${sc}% ほど上下していて、`
      + '増えているとも減っているとも言えません。';
  }

  // 跳ねた月があれば名指しする
  let spike = '';
  if (f.outliers && f.outliers.length && months) {
    const o = f.outliers[0];
    const m = months[o.index];
    if (m) spike = `${m} が大きく${o.resid > 0 ? '跳ねて' : '落ち込んで'}います。`;
  } else if (f.outliers && f.outliers.length) {
    spike = '途中に大きく外れた月があります。';
  }

  if (!f.steady) {
    // 一本調子ではない。「毎月 X% ずつ」とは書かない。
    // 上下の大きさは「大きい」と決めつけず、毎月の動きと比べて示す。
    return `${name}はこの期間で ${tot}% ${up ? '増えました' : '減りました'}が、`
      + `月ごとの上下（±${sc}%）が毎月の動き（約 ${per}%）より大きく、`
      + `毎月少しずつ${up ? '増えた' : '減った'}わけではありません。`
      + (spike ? ` ${spike}` : '');
  }

  const strength = f.level === '緩やかな傾向' ? '緩やかに、' : '';
  return `${name}は${strength}毎月およそ ${per}% ずつ${up ? '増えて' : '減って'}います`
    + `（この期間で ${tot}% ${up ? '増加' : '減少'}）。`
    + `月ごとの上下は ${sc}% ほどで、${up ? '増え' : '減り'}方はおおむね一定です。`;
}

/**
 * 一覧の 1 行に収まる短い言い方。describeTrend と結論が食い違わないこと。
 *
 * ■ なぜ関数にしたか（2026-08-31）
 *   一覧側に別の文章生成（shortSay）を置いていたため、詳細を開くと
 *   「振れが大きい」と書いてあるのに、一覧では「毎月およそ 1.7% ずつ」と
 *   書いてある状態になっていた。表示していた 1,174 行のうち 1,138 行が該当。
 *   文章を作る場所を 1 つにまとめ、テストで固定する。
 *
 * words = { up: '集まりやすくなって', down: '集まりにくくなって' }
 */
function shortTrend(f, words) {
  if (!f) return '比べられるだけの月数がありません';
  const sc = f.scatterPct.toFixed(0);
  if (f.level === '傾向とは言えない') {
    return `月ごとに ${sc}% ほど上下するだけで、向きは定まりません`;
  }
  const up = f.slopePct > 0;
  const dir = up ? words.up : words.down;
  const per = Math.abs(f.slopePct).toFixed(1);
  const tot = Math.abs(f.totalPct).toFixed(0);
  if (!f.steady) {
    return `期間全体では ${tot}% ${dir}いますが、`
      + `月ごとの上下（±${sc}%）が毎月の動き（約 ${per}%）より大きく、一本調子ではありません`;
  }
  const strength = f.level === '緩やかな傾向' ? '緩やかに' : '';
  return `${strength}${dir}います（毎月およそ ${per}% ずつ）`;
}

/** 仕組みの説明。知りたい人だけが読む場所に置く用。 */
function explainMethod() {
  return '毎月の数値に直線を当てはめ、「1 か月あたり何 % 動いているか」と'
    + '「その直線からどれだけ散らばっているか」を分けて計算しています。'
    + '散らばりに対して動きが十分大きいときだけ「増え続けている／減り続けている」と表示します。'
    + 'この方法だと、求人数が少ない職種でも、動きが一定であれば拾えます。'
    + '月ごとの上下が大きいときは「毎月○% ずつ」とは書かず、'
    + '一本調子ではないことと、大きく外れた月を示します。'
    + 'なお、これは過去の動きを整理したものです。この先も同じように続くとは限りません。';
}

module.exports = { fitTrend, describeTrend, shortTrend, trendLabel, explainMethod };
