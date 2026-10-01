// Pure renderer for the guide data (GET /api/guide). No fetching here, so tests can feed fixtures.
// Class names are semantic (.guide__*); guide.css maps them to the old Tailwind look.
import type { ReactNode } from 'react';
import type { GuideBlock } from '../../generated/GuideBlock';
import type { GuideResponse } from '../../generated/GuideResponse';
import type { GuideSpan } from '../../generated/GuideSpan';

const cls = (base: string, mod: string): string => `${base} ${base}--${mod.replaceAll('_', '-')}`;

function Spans({ spans }: { spans: GuideSpan[] }): ReactNode {
  return spans.map((s, i) => {
    if (s.style === 'plain') return s.text;
    if (s.style === 'strong_white') {
      return (
        <strong key={i} className="guide__strong--white">
          {s.text}
        </strong>
      );
    }
    return <strong key={i}>{s.text}</strong>;
  });
}

function Blocks({ blocks }: { blocks: GuideBlock[] }): ReactNode {
  return blocks.map((b, i) => <Block key={i} block={b} />);
}

function Block({ block }: { block: GuideBlock }): ReactNode {
  switch (block.kind) {
    case 'card':
      return (
        <div className="guide__card">
          <Blocks blocks={block.blocks} />
        </div>
      );
    case 'group':
      return (
        <div className={cls('guide__group', block.style)}>
          <Blocks blocks={block.blocks} />
        </div>
      );
    case 'details':
      return (
        <details className={cls('guide__details', block.style)}>
          <summary className={cls('guide__summary', block.style)}>{block.summary}</summary>
          {block.style === 'shots' ? (
            <div className="guide__shots">
              <Blocks blocks={block.blocks} />
            </div>
          ) : (
            <Blocks blocks={block.blocks} />
          )}
        </details>
      );
    case 'heading': {
      const c = cls('guide__heading', block.style);
      if (block.style === 'title') return <h2 className={c}>{block.text}</h2>;
      if (block.style === 'section') return <h3 className={c}>{block.text}</h3>;
      if (block.style === 'panel') return <h5 className={c}>{block.text}</h5>;
      return <h4 className={c}>{block.text}</h4>;
    }
    case 'para':
      if (block.style === 'bare') return <Spans spans={block.spans} />;
      return (
        <p className={cls('guide__para', block.style)}>
          <Spans spans={block.spans} />
        </p>
      );
    case 'list': {
      const items = block.items.map((it, i) => (
        <li key={i}>
          <Spans spans={it} />
        </li>
      ));
      const c = cls('guide__list', block.style);
      return block.style === 'numbered' ? <ol className={c}>{items}</ol> : <ul className={c}>{items}</ul>;
    }
    case 'table': {
      const last = block.rows.length - 1;
      return (
        <table className={`guide__table${block.dense ? ' guide__table--dense' : ''}`}>
          <thead>
            <tr>
              {block.headers.map((h, i) => (
                <th key={i} className={`guide__th${block.wide_first_col && i === 0 ? ' guide__th--half' : ''}`}>
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {block.rows.map((row, ri) => (
              <tr key={ri} className={ri < last ? 'guide__tr--sep' : ''}>
                {row.map((c, ci) => (
                  <td key={ci} className={cls('guide__td', c.style)}>
                    <Spans spans={c.spans} />
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
    case 'image':
      return <img className="guide__img" src={block.src} alt={block.alt} loading="lazy" />;
  }
}

export function GuideView({ data }: { data: GuideResponse }) {
  return (
    <div className="guide__root" data-testid="guide-root">
      <Blocks blocks={data.blocks} />
    </div>
  );
}
