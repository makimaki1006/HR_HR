import type { TagUsage } from '../../../generated/TagUsage';
import { POPULARITY_ONLY_SP } from './KeywordChart';
import { bandsLayout, gapLayout, GAP_CENTER, rowTitle, shortWord, signed, VIEW_W } from './tagUsageLayout';

const BAR = '#006666';
const UP = '#c0504d';
const DOWN = '#4472c4';

/** 左のグラフ: 全体の割合の帯ごとに、語の数と各語の割合の棒を並べる。 */
export function TagBandsChart({ usage }: { usage: TagUsage }) {
  const empty = usage.n === 0 || usage.bands.every((b) => b.rows.length === 0);
  return (
    <figure className="cmp-chart cmp-tag-usage">
      <figcaption>求人票キーワード調査（全体の割合の帯別）</figcaption>
      {empty ? (
        <p className="cmp-note">集計できるキーワードがありません</p>
      ) : (
        <BandsSvg usage={usage} />
      )}
      {!empty && (
        <p className="cmp-note">
          全体 {usage.n} 件のうち、その語が付く求人の割合。件数1〜2の語（{usage.minor_word_count}語）は帯に入れていません。
        </p>
      )}
    </figure>
  );
}

function BandsSvg({ usage }: { usage: TagUsage }) {
  const l = bandsLayout(usage);
  return (
    <svg
      viewBox={`0 0 ${String(VIEW_W)} ${String(l.height)}`}
      role="img"
      aria-label="キーワードを全体の割合で3つの帯に分けた語と割合。横軸0から100パーセント"
    >
      <title>{`キーワードの割合の帯別（全体 ${String(usage.n)} 件）`}</title>
      {[0, 50, 100].map((t) => (
        <g key={t}>
          <path d={`M ${String(140 + 2.4 * t)} ${String(l.gridTop)} V ${String(l.gridBottom)}`} className="cmp-gridline" />
          <text x={140 + 2.4 * t} y={l.gridBottom + 14} textAnchor="middle" fontSize={12} className="cmp-label">
            {`${String(t)}%`}
          </text>
        </g>
      ))}
      {l.bands.map((b) => (
        <g key={b.band.key}>
          <text x={4} y={b.headY + 16} fontSize={13} fontWeight="bold" className="cmp-label">
            {`${b.band.label} ${String(b.band.rows.length)}語（${b.band.range}）`}
          </text>
          {b.emptyY !== null && (
            <text x={140} y={b.emptyY + 12} fontSize={12} className="cmp-label">
              該当する語はありません
            </text>
          )}
          {b.rows.map(({ row, y, width }) => (
            <g key={row.word}>
              <text x={135} y={y + 12} textAnchor="end" fontSize={12} className="cmp-label">
                {shortWord(row.word)}
                <title>{row.word}</title>
              </text>
              <rect x={140} y={y + 2} width={width} height={12} fill={BAR}>
                <title>{rowTitle(row)}</title>
              </rect>
              <text x={144 + width} y={y + 12} fontSize={11} className="cmp-label">
                {`${row.share_pct.toFixed(1)}%`}
              </text>
            </g>
          ))}
          {b.moreY !== null && (
            <text x={140} y={b.moreY + 10} fontSize={11} className="cmp-label">
              {`ほか ${String(b.more)} 語`}
            </text>
          )}
        </g>
      ))}
    </svg>
  );
}

/** 右のグラフ: 人気求人に多い語・少ない語 (差の棒、0 を中心に左右)。 */
export function TagGapChart({ usage }: { usage: TagUsage }) {
  return (
    <figure className="cmp-chart cmp-tag-usage">
      <figcaption>人気求人に多い語・少ない語</figcaption>
      {usage.popular_n === 0 ? (
        <p className="cmp-note">{POPULARITY_ONLY_SP}</p>
      ) : (
        <>
          <GapSvg usage={usage} />
          <p className="cmp-note">
            棒は人気求人での割合から全体の割合を引いた差（ポイント）。人気求人 {usage.popular_n} 件、全体 {usage.n}{' '}
            件。件数3以上の語が対象。
          </p>
        </>
      )}
    </figure>
  );
}

function GapSvg({ usage }: { usage: TagUsage }) {
  const l = gapLayout(usage);
  const per = 100 / l.max;
  return (
    <svg
      viewBox={`0 0 ${String(VIEW_W)} ${String(l.height)}`}
      role="img"
      aria-label="人気求人での割合から全体の割合を引いた差。人気求人に多い語と少ない語の上位10語"
    >
      <title>{`人気求人と全体の割合の差（人気求人 ${String(usage.popular_n)} 件 / 全体 ${String(usage.n)} 件）`}</title>
      {[
        [-l.max, `-${l.max.toFixed(0)}`],
        [0, '0'],
        [l.max, `+${l.max.toFixed(0)}`],
      ].map(([v, label]) => {
        const x = GAP_CENTER + Number(v) * per;
        return (
          <g key={String(label)}>
            <path d={`M ${String(x)} ${String(l.top)} V ${String(l.bottom)}`} className="cmp-gridline" />
            <text x={x} y={l.bottom + 14} textAnchor="middle" fontSize={12} className="cmp-label">
              {label}
            </text>
          </g>
        );
      })}
      {l.sections.map((s) => (
        <g key={s.title}>
          <text x={4} y={s.headY + 16} fontSize={13} fontWeight="bold" className="cmp-label">
            {`${s.title} ${String(s.count)}語`}
          </text>
          {s.emptyY !== null && (
            <text x={GAP_CENTER} y={s.emptyY + 12} textAnchor="middle" fontSize={12} className="cmp-label">
              該当する語はありません
            </text>
          )}
          {s.rows.map((r) => (
            <g key={r.row.word}>
              <text x={120} y={r.y + 12} textAnchor="end" fontSize={12} className="cmp-label">
                {shortWord(r.row.word)}
                <title>{r.row.word}</title>
              </text>
              <rect x={r.x} y={r.y + 2} width={r.width} height={12} fill={r.positive ? UP : DOWN}>
                <title>{rowTitle(r.row)}</title>
              </rect>
              <text x={r.textX} y={r.y + 12} textAnchor={r.anchor} fontSize={11} className="cmp-label">
                {signed(r.row.gap_pt ?? 0)}
              </text>
            </g>
          ))}
        </g>
      ))}
    </svg>
  );
}
