import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { KpiCard } from './KpiCard';

describe('KpiCard', () => {
  it('renders value, unit and n with digit grouping', () => {
    const html = renderToStaticMarkup(
      <KpiCard label="求人数" value={12345} unit="件" n={1234} />,
    );
    expect(html).toContain('<h3 class="hw-kpi-label">求人数</h3>');
    expect(html).toContain('<span>12,345</span><span class="hw-kpi-unit">件</span>');
    expect(html).toContain('>n=1,234<');
  });

  it('shows データなし (not 0) for a null value', () => {
    const html = renderToStaticMarkup(<KpiCard label="平均月給" value={null} unit="円" n={0} />);
    expect(html).toContain('データなし');
    expect(html).not.toContain('hw-kpi-unit');
    expect(html).toContain('>n=0<');
  });

  it('treats NaN / Infinity as データなし', () => {
    for (const v of [Number.NaN, Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY]) {
      const html = renderToStaticMarkup(<KpiCard label="x" value={v} unit="円" n={3} />);
      expect(html).toContain('データなし');
      expect(html).not.toContain('NaN');
      expect(html).not.toContain('∞');
      expect(html).not.toContain('hw-kpi-unit');
    }
  });

  it('keeps a real 0 distinct from null', () => {
    const html = renderToStaticMarkup(<KpiCard label="欠員" value={0} unit="件" n={10} />);
    expect(html).not.toContain('データなし');
    expect(html).toContain('<span>0</span>');
  });

  it('shows -0 as 0, never "-0"', () => {
    const html = renderToStaticMarkup(<KpiCard label="x" value={-0} unit="件" n={1} />);
    expect(html).toContain('<span>0</span>');
    expect(html).not.toContain('-0');
  });

  it('shows a tiny negative that rounds to zero as 0, never "-0"', () => {
    const html = renderToStaticMarkup(<KpiCard label="x" value={-0.0001} unit="件" n={1} />);
    expect(html).toContain('<span>0</span>');
    expect(html).not.toContain('-0');
  });

  it('keeps a real negative value', () => {
    const html = renderToStaticMarkup(<KpiCard label="x" value={-12} unit="pt" n={1} />);
    expect(html).toContain('<span>-12</span>');
  });

  it('treats a non-finite n like an unknown n (n=不明, no "n=NaN")', () => {
    for (const n of [Number.NaN, Number.POSITIVE_INFINITY]) {
      const html = renderToStaticMarkup(<KpiCard label="x" value={1} unit="%" n={n} />);
      expect(html).toContain('>n=不明<');
      expect(html).not.toContain('NaN');
      expect(html).not.toContain('∞');
    }
  });

  it('shows n=0 when n is -0', () => {
    const html = renderToStaticMarkup(<KpiCard label="x" value={1} unit="%" n={-0} />);
    expect(html).toContain('>n=0<');
  });

  it('shows n=不明 when n is null', () => {
    const html = renderToStaticMarkup(<KpiCard label="x" value={1} unit="%" n={null} />);
    expect(html).toContain('>n=不明<');
  });

  it('uses a custom formatter and renders the note', () => {
    const html = renderToStaticMarkup(
      <KpiCard
        label="充足率"
        value={0.4567}
        unit="%"
        n={300}
        format={(v) => (v * 100).toFixed(1)}
        note={<em>参考値</em>}
      />,
    );
    expect(html).toContain('<span>45.7</span>');
    expect(html).toContain('<div class="hw-kpi-note"><em>参考値</em></div>');
  });

  it('display (pre-formatted string) wins over format(value), unit still appended', () => {
    const html = renderToStaticMarkup(
      <KpiCard label="平均月給" value={250000} display="25.0万" unit="円" n={10} />,
    );
    expect(html).toContain('<span>25.0万</span><span class="hw-kpi-unit">円</span>');
    expect(html).not.toContain('250,000');
  });

  it('display is shown even when value is null; emptyText replaces データなし', () => {
    const shown = renderToStaticMarkup(<KpiCard label="x" value={null} display="約10件" unit="" n={null} />);
    expect(shown).toContain('約10件');
    expect(shown).not.toContain('データなし');
    const dash = renderToStaticMarkup(<KpiCard label="x" value={null} unit="円" n={null} emptyText="-" />);
    expect(dash).toContain('<span class="hw-kpi-empty">-</span>');
    expect(dash).not.toContain('データなし');
    expect(dash).toContain('>n=不明<');
  });
});
