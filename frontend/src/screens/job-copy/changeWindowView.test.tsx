// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { JobTimeline } from './JobTimeline';
import { HrhPerformance } from './HrhPerformance';
import { jobs } from './data';
import type { EChartProps } from '../../components/EChart';

const charts = vi.hoisted(() => ({ latest: null as EChartProps | null }));
vi.mock('../../components/EChart', () => ({ EChart: (props: EChartProps) => { if (props.testId === 'jt-applications') charts.latest = props; return <div />; } }));
afterEach(cleanup);
function required<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error('Missing synthetic fixture value');
  return value;
}
const base = required(jobs[0]);
const job = { ...base, versions: base.versions.filter(version => version.kind === 'published').map((version, i) => { const copy = { ...version }; delete copy.publishedUntil; return { ...copy, publishedFrom: `${required(['2026-09-01', '2026-09-16', '2026-09-25'][i])}T09:00:00+09:00` }; }),
  hrhPerformance: { ...required(base.hrhPerformance), rows: [{ period_start: '2026-09-14', period_end: '2026-09-20', impressions: 1234, clicks: 50, cost_yen: 5000, applications: 4 }] },
  overallApplications: { ...required(base.overallApplications), byDate: { '2026-09-02': 5, '2026-09-16': 7, '2026-09-24': 2, '2026-09-25': 100 } },
};
function choose(index = 1) {
  const lane = screen.getByRole('group', { name: '本文' });
  fireEvent.click(required(within(lane).getAllByRole('button')[index]));
}
it('shows actual days, rates, exact dates and updates chart bands and 14/28 day controls', () => {
  render(<JobTimeline job={job} marketMode="demo" showDummyBilling={false} />);
  choose();
  const panel = screen.getByRole('region', { name: '変えた前後の応募' });
  expect(within(panel).getByRole('button', { name: '7日' }).getAttribute('aria-pressed')).toBe('true');
  expect(panel.textContent).toContain('前 7日 0件');
  expect(panel.textContent).toContain('後 7日 7件');
  expect(panel.textContent).not.toContain('1日あたり');
  expect(panel.textContent).toContain('変えた日をまたぐ週');
  const option = required(charts.latest).option as { series: { markArea: { data: { xAxis: number }[][] } }[] };
  expect(required(option.series[0]).markArea.data.map(pair => pair.map(point => point.xAxis))).toEqual([
    [Date.parse('2026-09-09T00:00:00Z'), Date.parse('2026-09-16T00:00:00Z')],
    [Date.parse('2026-09-16T00:00:00Z'), Date.parse('2026-09-23T00:00:00Z')],
  ]);
  fireEvent.click(within(panel).getByRole('button', { name: '14日' }));
  expect(panel.textContent).toContain('前 14日 5件');
  expect(panel.textContent).toContain('後 9日分（14日に届かず） 9件');
  expect(panel.textContent).toContain('1日あたり 0.4件');
  expect(panel.textContent).toContain('1日あたり 1.0件');
  fireEvent.click(within(panel).getByRole('button', { name: '28日' }));
  expect(panel.textContent).toContain('前 28日 5件');
  choose(0);
  expect(screen.queryByRole('region', { name: '変えた前後の応募' })).toBeNull();
});
it('hides the section when daily data is unavailable or the selected acquisition did not change', () => {
  const withoutDates: typeof base = { ...job }; delete withoutDates.overallApplications;
  const { rerender } = render(<JobTimeline job={withoutDates} marketMode="demo" />);
  choose();
  expect(screen.queryByRole('region', { name: '変えた前後の応募' })).toBeNull();
  rerender(<JobTimeline job={{ ...job, versions: [required(job.versions[0]), { ...required(job.versions[0]), id: 'same', publishedFrom: '2026-09-16' }] }} marketMode="demo" />);
  choose();
  expect(screen.queryByRole('region', { name: '変えた前後の応募' })).toBeNull();
});
it('marks a weekly performance row and retains the entire weekly numbers', () => {
  render(<HrhPerformance job={job} />);
  const row = required(screen.getByRole('cell', { name: '1,234' }).closest('tr'));
  expect(row.textContent).toContain('変えた日をまたぐ週');
  expect(row.textContent).toContain('5,000円');
  expect(within(row).getByRole('cell', { name: '4' })).toBeTruthy();
});
it('keeps image-only changes and skips an unchanged acquisition when finding neighbors', () => {
  const first = { ...required(job.versions[0]), images: [], imageReferences: [] };
  const imageChange = { ...first, id: 'image-change', label: '画像変更', publishedFrom: '2026-09-16', imageReferences: [{ slot: 1, referenceHash: 'fictional-image' }] };
  const unchanged = { ...imageChange, id: 'same-image', label: '同じ画像', publishedFrom: '2026-09-18' };
  const nextChange = { ...required(job.versions[2]), images: [], imageReferences: [] };
  render(<JobTimeline job={{ ...job, versions: [first, imageChange, unchanged, nextChange] }} marketMode="demo" />);
  const imageLane = screen.getByRole('group', { name: '画像' });
  fireEvent.click(required(within(imageLane).getAllByRole('button')[1]));
  expect(screen.getByRole('region', { name: '変えた前後の応募' }).textContent).toContain('後 7日 7件');
  fireEvent.click(required(within(imageLane).getAllByRole('button')[2]));
  expect(screen.queryByRole('region', { name: '変えた前後の応募' })).toBeNull();
});
it('shows known unpublished days and shortens the before side at the previous actual change', () => {
  const versions = job.versions.map((version, index) => index === 1 ? { ...version, publishedUntil: '2026-09-22' } : version);
  render(<JobTimeline job={{ ...job, versions }} marketMode="demo" />);
  choose(2);
  const panel = screen.getByRole('region', { name: '変えた前後の応募' });
  expect(panel.textContent).toContain('前 7日 2件');
  expect(panel.textContent).toContain('後 7日 100件');
  fireEvent.click(within(panel).getByRole('button', { name: '14日' }));
  expect(panel.textContent).toContain('前 9日分（14日に届かず） 9件');
  expect(panel.textContent).toContain('後 11日分（14日に届かず） 100件');
  expect(panel.textContent).toContain('掲載していない日 3日');
});
