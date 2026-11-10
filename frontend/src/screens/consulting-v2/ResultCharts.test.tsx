// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { BoxPlot, whiskers } from './ResultCharts';

const stats = (min: number, q1: number, median: number, q3: number, max: number) => ({ min, q1, median, q3, max, mean: median, n: 40 });

afterEach(cleanup);

describe('BoxPlot のひげ', () => {
  it('ひげは箱の幅の1.5倍までで止め、それより内側なら最小・最大で止める', () => {
    expect(whiskers(stats(0, 2, 4, 10, 491))).toEqual({ low: 0, high: 22 });
    expect(whiskers(stats(5, 9, 12, 19, 27))).toEqual({ low: 5, high: 27 });
    expect(whiskers(stats(0, 0, 0, 0, 30))).toEqual({ low: 0, high: 0 });
  });

  it('外れた大きい値があっても目盛りはひげの端までにし、右端に最大値を書く', () => {
    render(<BoxPlot title="架空の散らばり" rows={[{ label: '継続0回', stats: stats(0, 2, 4, 10, 491) }, { label: '継続1回', stats: stats(0, 1, 3, 6, 12) }]} />);
    const chart = screen.getByRole('img', { name: '架空の散らばり' });
    const texts = [...chart.querySelectorAll('text')].map((t) => t.textContent);
    expect(texts).toContain('22.0'); // 目盛りの右端は初回のひげの端 10 + 1.5×8
    expect(texts).not.toContain('491.0');
    expect(texts).toContain('最大 491');
    expect(texts.filter((t) => t.startsWith('最大'))).toHaveLength(1); // 継続1 は最大 12 がひげの内側
    expect(Number(chart.querySelector('rect')?.getAttribute('width'))).toBeCloseTo(8 / 22 * 450); // 箱が潰れない
  });
});
