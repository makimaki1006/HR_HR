// @vitest-environment happy-dom
import { cleanup, render } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { makeReport } from '../fixtures';
import { TagBandsChart, TagGapChart } from './TagUsageCharts';
import { bandsLayout, gapAxisMax, gapLayout, signed } from './tagUsageLayout';

afterEach(cleanup);

const usage = () => makeReport().excel.tag_usage;

describe('tagUsage layout', () => {
  it('符号つき・小数1桁で、0.05 未満は 0.0', () => {
    expect([signed(18.3), signed(-12.4), signed(0.01)]).toEqual(['+18.3', '-12.4', '0.0']);
  });
  it('軸の端は最大の差を 5 の倍数に切り上げ (最小 10)', () => {
    expect(gapAxisMax(usage())).toBe(20);
  });
  it('正の差は中心から右、負の差は左へ伸びる', () => {
    const l = gapLayout(usage());
    const up = l.sections[0]?.rows[0];
    const down = l.sections[1]?.rows[0];
    if (!up || !down) throw new Error('rows');
    expect(up.x).toBe(285);
    expect(up.width).toBeCloseTo(91.5, 5);
    expect(down.x).toBeCloseTo(285 - 62, 5);
    expect(down.anchor).toBe('end');
  });
  it('帯の棒は割合 × 2.4', () => {
    expect(bandsLayout(usage()).bands[0]?.rows[0]?.width).toBeCloseTo(136.08, 5);
  });
});

describe('TagBandsChart / TagGapChart', () => {
  it('帯の見出し・割合・ホバーの説明を出す', () => {
    const { container } = render(<TagBandsChart usage={usage()} />);
    expect(container.textContent).toContain('ほとんどの求人が付けている 1語（全体の50%以上）');
    expect(container.textContent).toContain('56.7%');
    const titles = Array.from(container.querySelectorAll('rect title')).map((t) => t.textContent);
    expect(titles[0]).toBe('交通費支給: 700件・56.7% / 人気求人 30件・75.0% / 差 +18.3');
  });
  it('人気求人が 0 件のときは SP 版の案内だけ', () => {
    const u = { ...usage(), popular_n: 0 };
    const { container } = render(<TagGapChart usage={u} />);
    expect(container.textContent).toContain('人気・超人気の表示があるSP版CSVのときだけ表示されます');
    expect(container.querySelector('svg')).toBeNull();
  });
  it('差の値を符号つきで出す', () => {
    const { container } = render(<TagGapChart usage={usage()} />);
    expect(container.textContent).toContain('+18.3');
    expect(container.textContent).toContain('-12.4');
  });
});
