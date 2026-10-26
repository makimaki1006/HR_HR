import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { ComparisonRow } from '../../../generated/ComparisonRow';
import type { KeywordRow } from '../../../generated/KeywordRow';
import { GROUP_COLORS, keywordLayout, type KeywordBar, type KeywordGroup } from './keywordLayout';

interface Item {
  word: string;
  bars: readonly KeywordBar[];
}

/** 全体の語の件数 (上位 20 語)。棒の長さは件数。 */
export function AllKeywordChart({ rows }: { rows: readonly KeywordRow[] }) {
  const items: Item[] = rows.slice(0, 20).map((r) => ({
    word: r.word,
    bars: [{ group: '全体', value: r.count }],
  }));
  return (
    <ChartFrame
      caption="求人票キーワード調査（全体・上位20語）"
      empty="集計できるキーワードがありません"
      count={items.length}
    >
      {(size) => (
        <KeywordSvg
          title="全体の訴求語・求人数"
          label="全体の上位20語を含む求人数"
          mode="all"
          items={items}
          size={size}
        />
      )}
    </ChartFrame>
  );
}

/** 占有率の系列が無いときに伝える一文 (SP 版 CSV 以外)。 */
export const POPULARITY_ONLY_SP = '人気・超人気のタグがあるSP版CSVのときだけ表示されます';
/** 人気・超人気の件数がこの数未満のとき「参考値」と添える。 */
const SMALL_CLASS_N = 5;
const SMALL_N_NOTE = '件数が少ないため参考値';

/** 人気・超人気の系列に関する注記の文。 */
export function popularityNotes(popularAnyN: number): string[] {
  if (popularAnyN === 0) return [POPULARITY_ONLY_SP];
  if (popularAnyN < SMALL_CLASS_N) return [`人気・超人気（${String(popularAnyN)}件）は${SMALL_N_NOTE}です`];
  return [];
}

/** 先頭 N 件・全体・人気・超人気 (1 区分) の占有率 (同じ語・同じ 0〜100% の軸)。全体が欠測の語は棒を描かず — を出す。 */
export function ComparisonChart({
  rows,
  headN,
  allN,
  popularAnyN,
}: {
  rows: readonly ComparisonRow[];
  headN: number;
  allN: number;
  popularAnyN: number;
}) {
  const items: Item[] = rows.map((r) => {
    const bars: KeywordBar[] = [
      { group: '全体', value: r.all_share_pct },
      { group: '先頭', value: r.head_share_pct },
    ];
    if (popularAnyN > 0) bars.push({ group: '人気・超人気', value: r.popular_any_share_pct });
    return { word: r.word, bars };
  });
  const extra = popularAnyN > 0;
  const legend: { group: KeywordGroup; text: string }[] = extra
    ? [
        { group: '全体', text: `全体 ${String(allN)}件` },
        { group: '先頭', text: `先頭 ${String(headN)}件` },
      ]
    : [
        { group: '全体', text: '全体' },
        { group: '先頭', text: `先頭 ${String(headN)}件` },
      ];
  if (popularAnyN > 0) legend.push({ group: '人気・超人気', text: `人気・超人気 ${String(popularAnyN)}件` });
  return (
    <ChartFrame
      caption="訴求語の占有率比較（上位20語）"
      empty="比較できるキーワードデータがありません"
      count={items.length}
      notes={items.length > 0 ? popularityNotes(popularAnyN) : []}
    >
      {(size) => (
        <KeywordSvg
          title={`訴求語の占有率比較：先頭 ${String(headN)} 件 / 全体 ${String(allN)} 件`}
          label={`先頭 ${String(headN)} 件と全体 ${String(allN)} 件の占有率比較。先頭の上位20語。横軸0から100パーセント`}
          mode="comparison"
          legend={legend}
          items={items}
          size={size}
        />
      )}
    </ChartFrame>
  );
}

interface Size {
  w: number;
  h: number;
}

/** 描画した枠の実寸を測り、幅に合わせて作り直す (旧 competitor-keywords.js の ResizeObserver)。 */
function ChartFrame({
  caption,
  empty,
  count,
  notes = [],
  children,
}: {
  caption: string;
  empty: string;
  count: number;
  notes?: readonly string[];
  children: (size: Size) => ReactNode;
}) {
  const frame = useRef<HTMLElement>(null);
  const [size, setSize] = useState<Size>({ w: 440, h: 715 });
  useEffect(() => {
    const svg = frame.current?.querySelector('svg');
    if (!svg || typeof ResizeObserver === 'undefined') return undefined;
    const measure = (): void => {
      const box = svg.getBoundingClientRect();
      // 非表示のタブでは 0。前回の寸法を保つ。
      if (box.width >= 1 && box.height >= 1) {
        setSize((prev) => (prev.w === box.width && prev.h === box.height ? prev : { w: box.width, h: box.height }));
      }
    };
    const observer = new ResizeObserver(measure);
    observer.observe(svg);
    measure();
    return () => {
      observer.disconnect();
    };
  }, [count]);
  return (
    <figure className="cmp-chart cmp-keyword-chart" ref={frame}>
      <figcaption>{caption}</figcaption>
      {count === 0 ? <p className="cmp-note">{empty}</p> : children(size)}
      {notes.map((n) => (
        <p key={n} className="cmp-note">
          {n}
        </p>
      ))}
    </figure>
  );
}

function KeywordSvg({
  title,
  label,
  mode,
  legend = [],
  items,
  size,
}: {
  title: string;
  label: string;
  mode: 'all' | 'comparison';
  legend?: readonly { group: KeywordGroup; text: string }[];
  items: readonly Item[];
  size: Size;
}) {
  const l = keywordLayout({ mode, items, ...size });
  return (
    <svg
      viewBox={`0 0 ${String(l.w)} ${String(l.h)}`}
      role="img"
      aria-label={label}
      data-series={mode === 'all' ? 'keyword-all' : 'keyword-head'}
      style={l.h > size.h ? { minHeight: l.h } : undefined}
    >
      <title>{title}</title>
      {l.ticks.map((t, i) => (
        <g key={i}>
          <path d={`M${String(t.x)} ${String(l.top)} V${String(l.h - 30)}`} className="cmp-gridline" />
          <text x={t.x} y={l.h - 8} textAnchor="middle" fontSize={12} className="cmp-axis">
            {t.label}
          </text>
        </g>
      ))}
      {mode === 'comparison' &&
        legend.map((g, i) => {
          const x = l.top > 42 ? l.left + (i % 2) * 105 : l.left + i * 83;
          const y = l.top > 42 ? 8 + Math.floor(i / 2) * 16 : 8;
          return (
            <g key={g.group}>
              <rect x={x} y={y} width={12} height={8} fill={GROUP_COLORS[g.group]} />
              <text x={x + 17} y={y + 9} fontSize={12} className="cmp-label">
                {g.text}
              </text>
            </g>
          );
        })}
      {l.rows.map((r, i) => (
        <g key={i}>
          <text x={l.left - 10} y={r.cy + 4} textAnchor="end" fontSize={13} className="cmp-label">
            {r.label}
            <title>{r.word}</title>
          </text>
          {r.bars.map((b) =>
            b.value === null ? (
              <text key={b.group} x={l.left + 4} y={b.y + 9} fontSize={11} className="cmp-label">
                —
              </text>
            ) : (
              <g key={b.group}>
                <rect
                  x={l.left}
                  y={b.y}
                  width={b.width}
                  height={b.height}
                  rx={2}
                  fill={b.color}
                  data-word={r.word}
                  data-group={b.group}
                  data-value={b.value}
                >
                  <title>{`${r.word} / ${b.group}: ${b.text}`}</title>
                </rect>
                <text
                  x={l.left + b.width + 4}
                  y={b.y + b.height}
                  fontSize={l.labelSize}
                  className="cmp-label"
                >
                  {b.text}
                </text>
              </g>
            ),
          )}
        </g>
      ))}
    </svg>
  );
}
