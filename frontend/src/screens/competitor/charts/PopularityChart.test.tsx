// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { PopularityChart, popularityDomain, popularityPosition, type PopularityChartData } from './PopularityChart';
afterEach(cleanup);
const data = (values: (number | null)[]): PopularityChartData => ({ unit: '%', series: ['人気・超人気の割合'], rows: values.map((value, i) => ({ label: ['夜勤', 'シフト制', '固定・不明'][i] ?? String(i), values: [{ value, n: [16, 41, 265][i] ?? 20, popular: [11, 20, 74][i] ?? 0 }] })) });
describe('人気の比較の尺度', () => {
  it('勤務形態の表示値と棒の長さの順序を一致させ、分子と母数を区別する', () => {
    const { container } = render(<PopularityChart data={data([69, 49, 28])} />);
    const widths = [...container.querySelectorAll<HTMLElement>('.cmp-pop-bar')].map(b => parseFloat(b.style.width));
    [69, 49, 28].forEach((value, i) => { expect(widths[i]).toBeCloseTo(value); });
    expect(screen.getByText('人気・超人気 11／16件')).toBeTruthy();
    expect(screen.getByText('人気・超人気 74／265件')).toBeTruthy();
  });
  it('小差を拡大すると点にし、非0始点と絶対差を明示する', () => {
    const { container } = render(<PopularityChart data={data([32, 33, 34])} />);
    fireEvent.click(screen.getByRole('button', { name: '差を拡大' }));
    expect(container.querySelectorAll('.cmp-pop-bar')).toHaveLength(0);
    expect(container.querySelectorAll('.cmp-pop-dot')).toHaveLength(3);
    expect(screen.getByText(/0始点とは限りません/)).toBeTruthy();
    expect(screen.getByText(/最大差 2ポイント/)).toBeTruthy();
    const range = popularityDomain(data([32, 33, 34]), true);
    expect(popularityPosition(34, range)).toBeGreaterThan(popularityPosition(33, range));
    fireEvent.click(screen.getByRole('button', { name: '全体を見る' }));
    expect(container.querySelectorAll('.cmp-pop-bar')).toHaveLength(3);
  });
  it('全0・同値100・全欠測でも有限な範囲を保つ', () => {
    for (const values of [[0, 0], [100, 100], [null, null]]) for (const zoom of [false, true]) {
      const range = popularityDomain(data(values), zoom);
      expect(range.every(Number.isFinite)).toBe(true); expect(range[1]).toBeGreaterThan(range[0]);
    }
    render(<PopularityChart data={data([0, 0])} />); expect(screen.getByText(/表示値は同じ/)).toBeTruthy();
  });
  it('欠測はゼロの棒に置換しない', () => {
    const { container } = render(<PopularityChart data={data([null, 0, 28])} />);
    expect(container.querySelectorAll('.cmp-pop-bar')).toHaveLength(2); expect(screen.getByText('—')).toBeTruthy();
  });
  it('表示精度より小さい差を拡大しても同じ値は同じ位置にする', () => {
    const d = { ...data([0.01, 0.04]), unit: 'pt' as const };
    const { container } = render(<PopularityChart data={d} />);
    fireEvent.click(screen.getByRole('button', { name: '差を拡大' }));
    const dots = [...container.querySelectorAll<HTMLElement>('.cmp-pop-dot')];
    expect(dots[0]?.style.left).toBe(dots[1]?.style.left);
    expect(dots.map(d => d.dataset.value)).toEqual(['0', '0']);
    expect(screen.getByText(/表示値は同じ/)).toBeTruthy();
    expect(popularityDomain(d, true)).toEqual([-0.1, 0.1]);
  });
  it('ポイント差は通常表示でも0を含み、小さな差に合わせた範囲にする', () => {
    const d = { ...data([-1, 2]), unit: 'pt' as const }; expect(popularityDomain(d, false)).toEqual([-2.2, 2.2]);
  });
});
