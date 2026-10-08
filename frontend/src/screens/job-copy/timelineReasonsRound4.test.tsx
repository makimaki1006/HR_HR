// @vitest-environment happy-dom
/**
 * Review round 4 of the application reasons: a 未取得 選択済み count never shows as 0件, the lane
 * does not say 「記録はありません」 when the records were only left out, and a section whose texts
 * were cut by the read limit does not say there is nothing to classify.
 */
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { jobs } from './data';
import type { JobCopyRecord } from './data';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import { JobTimeline } from './JobTimeline';
import { ApplicantReasons } from './ApplicantReasons';
import { inferCategories } from './reasonCategories';

vi.mock('../../api/client', () => ({ apiGet: vi.fn() }));
vi.mock('../../components/EChart', () => ({ EChart: () => <div>グラフ</div> }));
afterEach(() => { cleanup(); });

function demo(): JobCopyRecord {
  const job = jobs.find(item => item.id === 'demo-job-001');
  if (!job) throw new Error('Missing demo job');
  return job;
}
function demoReasons(): ApplicantReasonCollection {
  const reasons = demo().applicantReasons;
  if (!reasons) throw new Error('Missing demo reasons');
  return reasons;
}
const LEGACY = ['oubodouki', 'ouboriyuu_baitaikisai', 'ouboriyuu_hiaringu'];
/** The shape of a stored file written before the categories were read. */
function legacy(): ApplicantReasonCollection {
  const reasons = demoReasons();
  return { ...reasons, selections: null, items: reasons.items.filter(item => LEGACY.includes(item.sourceProperty)).map(item => ({ ...item, applicant: null })),
    sourceCounts: Object.fromEntries(Object.entries(reasons.sourceCounts).filter(([property]) => LEGACY.includes(property))) };
}

describe('a stored file without category selections', () => {
  it('shows 選択済み as 未取得 on the 応募理由 tab, with no 0件 選択済み count or column', () => {
    render(<ApplicantReasons job={{ ...demo(), applicantReasons: legacy() }} />);
    const summary = screen.getByRole('region', { name: '応募理由の分類' });
    expect(summary.textContent).toContain('n=7（記述7件） · 選択済み 未取得 · キーワードで推定6件 · 分類できない1件');
    expect(summary.textContent).not.toContain('選択済み0件');
    const table = within(screen.getByRole('region', { name: '応募理由の分類の件数' })).getByRole('table');
    expect(within(table).getAllByRole('columnheader').map(cell => cell.textContent)).toEqual(['分類', '件数（キーワードで推定）', 'nに対する割合']);
    expect(within(table).getAllByRole('row')[1]?.textContent).toBe('給与2件29%');
  });

  it('shows 選択済み 未取得 in every period of the timeline table', () => {
    render(<JobTimeline job={{ ...demo(), applicantReasons: legacy() }} marketMode="demo" />);
    const table = within(screen.getByRole('region', { name: '期間ごとの応募理由の数値' })).getByRole('table');
    const basis = [...table.querySelectorAll('td.jt-reason-basis')].map(cell => cell.textContent);
    expect(basis.slice(0, 2)).toEqual(['選択済み 未取得推定2件分類できない0件', '選択済み 未取得推定3件分類できない1件']);
    expect(table.textContent).not.toContain('選択済み0件');
  });
});

describe('the 応募理由 lane when every reason was left out', () => {
  it('says the records were left out, not that there are none', () => {
    const keys = Array.from({ length: 9 }, (_, index) => String(index + 1).repeat(64));
    render(<JobTimeline job={{ ...demo(), applicantReasons: { ...demoReasons(), multiListingApplicants: keys } }} marketMode="demo" />);
    const lane = screen.getByRole('group', { name: '応募理由' });
    expect(lane.querySelector('.jt-empty')?.textContent).not.toBe('応募理由の記録はありません');
    expect(lane.textContent).toContain('この求人だけに関連する、分類か文のある応募理由の記録はありません（ほかの求人にも関連する9件を除く）');
  });
});

describe('texts cut by the read limit', () => {
  it('says the transfer section could not read its texts, not that there is nothing to classify', () => {
    const reasons = demoReasons();
    expect(reasons.sourceCounts.genshokumaeshokukaranotenshokuriyuu?.nonblank).toBe(1);
    const truncated = { ...reasons, truncated: true, items: reasons.items.filter(item => item.sourceProperty !== 'genshokumaeshokukaranotenshokuriyuu') };
    render(<ApplicantReasons job={{ ...demo(), applicantReasons: truncated }} />);
    const transfer = screen.getByRole('region', { name: '今の仕事・前の仕事から転職する理由の分類' });
    expect(transfer.textContent).toContain('n=0（応募0件）');
    expect(transfer.textContent).toContain('読み込めた記述がありません（記入あり 1件 は読み込める上限を超えたため読み込んでいません。0件という意味ではありません）。');
    expect(transfer.textContent).not.toContain('分類できる記録はありません');
    expect(document.body.textContent).toContain('「選択済み」はすべての応募で数えていますが、「キーワードで推定」「分類できない」の数と記述の一覧は、読み込んだ記述だけで数えています');
  });
});

describe('the keyword dictionary after normalizing the keywords once', () => {
  it.each([
    ['時給が高い', ['給与']],
    ['家から近い', ['勤務地']],
    ['介護の仕事がしたい', ['職種興味']],
    ['大手で安心', ['会社規模']],
    ['ＳＮＳで見た', []],
  ])('%s', (text, expected) => {
    expect(inferCategories(text)).toEqual(expected);
  });
});
