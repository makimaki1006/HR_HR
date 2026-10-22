// 顧客ごとに、顧客の担当者へ出す機能を選ぶ。社内の人（コンサルタント）には全部出す。
// 項目ごとの見せる・書ける、と「項目の設定」の許可は、シートごとの設定（項目とプルダウン）で決める。
import { currentCustomer, type DataState } from './data';
import type { Customer, CustomerFeature } from './types';

export const FEATURES: { key: CustomerFeature; label: string; note: string }[] = [
  { key: 'metrics', label: '数字', note: '面接移行率・採用率などの画面' },
  { key: 'history', label: '記録', note: 'だれが・いつ・何を変えたかの一覧' },
  { key: 'today', label: '今日の面接', note: '一覧の上の「今日の面接・明日・新着・未対応」' },
  { key: 'columnPicker', label: '列の表示', note: '一覧に出す列を顧客が選ぶ' },
  { key: 'export', label: '書き出し', note: '一覧を CSV で書き出す' },
  { key: 'timeline', label: '応募者ごとの変更の記録', note: '応募者の詳細の下の時系列' },
];

export function customerShows(c: Customer | undefined, key: CustomerFeature): boolean {
  return !(c?.hiddenFeatures ?? []).includes(key);
}

/** いま見ている人にこの機能を出すか。社内の人は常に出す。 */
export function canSee(d: DataState, key: CustomerFeature): boolean {
  return d.viewer.kind === 'staff' || customerShows(currentCustomer(d), key);
}
