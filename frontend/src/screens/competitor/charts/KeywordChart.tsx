import { useEffect, useRef, useState, type ReactNode } from 'react';
import type { ComparisonRow } from '../../../generated/ComparisonRow';
import type { KeywordRow } from '../../../generated/KeywordRow';
import { keywordLayout, type KeywordBar } from './keywordLayout';

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

/** 先頭 N 件と全体の占有率 (同じ語・同じ 0〜100% の軸)。全体が欠測の語は棒を描かず — を出す。 */
export function ComparisonChart({
  rows,
  headN,
  allN,
}: {
  rows: readonly ComparisonRow[];
  headN: number;
  allN: number;
}) {
  const items: Item[] = rows.map((r) => ({
    word: r.word,
    bars: [
      { group: '全体', value: r.all_share_pct },
      { group: '先頭', value: r.head_share_pct },
    ],
  }));
  return (
    <ChartFrame
      caption="訴求語の占有率比較（上位20語）"
      empty="比較できるキーワードデータがありません"
      count={items.length}
    >
      {(size) => (
        <KeywordSvg
          title={`訴求語の占有率比較：先頭 ${String(headN)} 件 / 全体 ${String(allN)} 件`}
          label={`先頭 ${String(headN)} 件と全体 ${String(allN)} 件の占有率比較。先頭の上位20語。横軸0から100パーセント`}
          mode="comparison"
          headN={headN}
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
  children,
}: {
  caption: string;
  empty: string;
  count: number;
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
    </figure>
  );
}

function KeywordSvg({
  title,
  label,
  mode,
  headN = 0,
  items,
  size,
}: {
  title: string;
  label: string;
  mode: 'all' | 'comparison';
  headN?: number;
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
      {mode === 'comparison' && (
        <>
          <rect x={l.left} y={8} width={12} height={8} fill="#006666" />
          <text x={l.left + 17} y={17} fontSize={12} className="cmp-label">
            全体
          </text>
          <rect x={l.left + 83} y={8} width={12} height={8} fill="#4472c4" />
          <text x={l.left + 100} y={17} fontSize={12} className="cmp-label">
            {`先頭 ${String(headN)}件`}
          </text>
        </>
      )}
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
                  fontSize={mode === 'comparison' ? 11 : 13}
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
