import { describe, expect, it } from 'vitest';
import { conditionChanges, managementColumns, manualColumns, manualLabel, manualRow } from './manualEditModel';
describe('手直しする求人票', () => {
  it('途中に保存形式の見出しがある本文も冒頭の給与を落とさない', () => {
    const body = '訪問看護師募集。時給1800円。\n勤務時間：9〜18時';
    expect(manualRow(body)['仕事内容']).toBe(body);
    expect(manualRow('勤務時間：9〜18時\n訪問看護師募集。時給1800円。', false)['仕事内容']).toBe('勤務時間：9〜18時\n訪問看護師募集。時給1800円。');
  });
  it('求人見出しの給与を変更・追加・削除した場合と保存済み案から戻した場合も確認する', () => {
    const current = manualRow('案件名：月給25万円の配送');
    expect(conditionChanges(current, current, { ...current, '案件名': '月給30万円の配送' })).toContain('案件名');
    const plain = { ...current, '案件名': '配送スタッフ' };
    expect(conditionChanges(current, current, plain)).toContain('案件名');
    expect(conditionChanges(plain, plain, current)).toContain('案件名');
    expect(conditionChanges(plain, current, plain)).toContain('案件名');
  });
  it('84項目の入力名を用意し、空の給与は未取得のまま保持する', () => {
    expect(manualColumns).toHaveLength(84);
    expect(new Set(manualColumns.map(manualLabel)).size).toBe(84);
    expect(manualColumns.map(manualLabel)).not.toContain('その他の案内');
    expect(manualColumns.map(manualLabel).join(' ')).not.toMatch(/id|基本給与|Indeed/);
    const row = manualRow('仕事の内容は配送です。');
    expect(row['仕事内容']).toBe('仕事の内容は配送です。');
    expect(row['基本給与 最小']).toBe('');
  });
  it.each(['基本給与 最小', '通勤経路', '勤務時間', '雇用形態', '自由項目1の内容'])('%sの変更を検知する', key => {
    const current = manualRow('仕事内容：配送\n給与形態：月給\n基本給与 最小：250000');
    expect(conditionChanges(current, current, { ...current, [key]: '変更後' })).toContain(key);
    expect(conditionChanges(current, current, current)).toHaveLength(0);
  });
  it('基にする案ですでに変わった条件も確認し、管理情報は入力対象にしない', () => {
    const current = manualRow('求人id：hidden\n仕事内容：配送\n基本給与 最小：250000');
    const base = { ...current, '基本給与 最小': '270000' };
    expect(conditionChanges(current, base, base)).toContain('基本給与 最小');
    expect(managementColumns).toEqual(['求人id', '店舗id', '職種id']);
  });
});
