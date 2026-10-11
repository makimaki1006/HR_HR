// @vitest-environment happy-dom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ColumnComparison, compareColumns, savedColumns } from './ColumnComparison';
afterEach(cleanup);
const before = '案件名：配送スタッフ\n自由項目2の内容：雇用保険 労災保険\n夜勤手当3,500円/回\n勤務時間：8時〜17時';
describe('全文で検知して列で特定する', () => {
  it('次の行の条件追加を検知し、その列の複数行値を丸ごと示す', () => {
    const after = before.replace('夜勤手当3,500円/回', '夜勤手当3,500円/回\n（月4回まで）');
    const result = compareColumns(before, after);
    expect(result.status).toBe('changed');
    expect(result.changes.map(item => item.key)).toEqual(['自由項目2の内容']);
    expect(result.changes[0]?.after).toBe('雇用保険 労災保険\n夜勤手当3,500円/回\n（月4回まで）');
    render(<ColumnComparison result={result} />);
    expect(screen.getByRole('table').textContent).toContain('（月4回まで）');
    expect(screen.getByRole('table').textContent).not.toContain('配送スタッフ');
    expect(screen.getByRole('table').textContent).not.toContain('8時〜17時');
    expect(document.querySelector('ins')?.textContent).toContain('（月4回まで）');
  });
  it.each([' ', '　'])('本文内のスペース「%s」一文字も検知する', space => {
    const result = compareColumns(before, before.replace('配送スタッフ', `配送${space}スタッフ`));
    expect(result.status).toBe('changed');
    expect(result.changes.map(item => item.key)).toEqual(['案件名']);
  });
  it('両端の空白・改行コードだけは揃え、内部の空白を保持する', () => {
    expect(compareColumns(before, `\u3000\r\n${before.replaceAll('\n', '\r\n')}\r\n `).status).toBe('unchanged');
    expect(compareColumns('仕事内容：配送 スタッフ', '仕事内容：配送　スタッフ').status).toBe('changed');
  });
  it('列定義にない見出しとその複数行本文をその他として捨てない', () => {
    const left = `${before}\n独自の待遇：家賃補助20,000円/月\n祝い金あり`;
    const right = left.replace('20,000', '10,000');
    const result = compareColumns(left, right);
    expect(result.changes).toEqual([expect.objectContaining({ key: 'その他', before: '独自の待遇：家賃補助20,000円/月\n祝い金あり', after: '独自の待遇：家賃補助10,000円/月\n祝い金あり' })]);
    render(<ColumnComparison result={result} />); expect(screen.getByRole('table').textContent).toContain('その他');
  });
  it('片方しかない列を追加・削除として表示し、空欄と未取得を区別する', () => {
    const result = compareColumns('案件名：配送\n勤務時間：8時', '案件名：配送\n応募資格：');
    expect(result.changes).toMatchObject([{ key: '勤務時間', kind: '削除', before: '8時' }, { key: '応募資格', kind: '追加', after: '' }]);
    render(<ColumnComparison result={result} />); expect(screen.getByRole('table').textContent).toContain('（空欄）');
    expect(screen.getByRole('table').textContent).toContain('未取得');
  });
  it('案の列値は保存された行データを使い、値の中の列名や改行を分割しない', () => {
    const a = { '仕事内容': '配送します\n勤務時間：8時', '勤務時間': '9時〜17時' };
    const b = { ...a, '仕事内容': `${a.仕事内容}\n（月4回まで）` };
    const body = (row: Record<string, string>) => Object.entries(row).map(([key, value]) => `${key}：${value}`).join('\n');
    const result = compareColumns(body(a), body(b), a, b);
    expect(result.changes).toEqual([expect.objectContaining({ key: '仕事内容', before: a.仕事内容, after: b.仕事内容 })]);
  });
  it('今の版でも複数行の値の中の未知の見出しを分断しない', () => {
    const value = '雇用保険\n手当：夜勤3,500円/回';
    const current = `自由項目2の内容：${value}\n勤務時間：9時〜17時`;
    const row = { '自由項目2の内容': `${value}\n（月4回まで）`, '勤務時間': '9時〜17時' };
    const next = `自由項目2の内容：${row['自由項目2の内容']}\n勤務時間：9時〜17時`;
    expect(compareColumns(current, next, undefined, row).changes).toEqual([expect.objectContaining({ key: '自由項目2の内容', before: value, after: row['自由項目2の内容'] })]);
  });
  it('今の版の複数行値に別の列名が含まれても実際の列の境界を使う', () => {
    const current = '仕事内容：配送します\n勤務時間：8時の例\n勤務時間：9時〜17時';
    const row = { '仕事内容': '配送します\n勤務時間：8時の例\n（月4回まで）', '勤務時間': '9時〜17時' };
    const next = `仕事内容：${row.仕事内容}\n勤務時間：9時〜17時`;
    expect(compareColumns(current, next, undefined, row).changes).toEqual([expect.objectContaining({ key: '仕事内容', before: '配送します\n勤務時間：8時の例', after: row.仕事内容 })]);
  });
  it('途中の未対応見出しもその他に残し、変更のない前後の列を出さない', () => {
    const row = { '仕事内容': '配送します', '勤務時間': '9時〜17時' };
    const current = '仕事内容：配送します\n独自の待遇：祝い金20,000円\n勤務時間：9時〜17時';
    const next = '仕事内容：配送します\n勤務時間：9時〜17時';
    expect(compareColumns(current, next, undefined, row).changes).toEqual([expect.objectContaining({ key: 'その他', before: '独自の待遇：祝い金20,000円', after: undefined, kind: '削除' })]);
  });
  it('管理用の識別子と機械的な給与の列名を画面に出さない', () => {
    const result = compareColumns('求人id：private-old\n試用・研修期の基本給与 最小：250000', '求人id：private-new\n試用・研修期の基本給与 最小：270000');
    render(<ColumnComparison result={result} />);
    expect(screen.getByRole('table').textContent).not.toMatch(/private-|求人id|基本給与/);
    expect(screen.getByRole('table').textContent).toContain('研修中の給与の下限');
    expect(screen.getByRole('table').textContent).toContain('270000');
  });
  it('取得済みの空本文と未取得を区別し、全列の削除も検知する', () => {
    const result = compareColumns('案件名：配送\n仕事内容：配達', '');
    expect(result.status).toBe('changed');
    expect(result.changes.map(item => [item.key, item.kind])).toEqual([['案件名', '削除'], ['仕事内容', '削除']]);
    expect(compareColumns('', '').status).toBe('unchanged');
    expect(compareColumns('案件名：配送', null).status).toBe('unavailable');
  });
  it('列の順序だけの違いも全文で検知してその他に示す', () => {
    expect(compareColumns('案件名：配送\n仕事内容：配達', '仕事内容：配達\n案件名：配送').changes.map(item => item.key)).toEqual(['その他']);
    expect(savedColumns('仕事内容：1行目\n2行目\n3行目').get('仕事内容')).toBe('1行目\n2行目\n3行目');
  });
});
