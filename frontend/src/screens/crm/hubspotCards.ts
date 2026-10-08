/**
 * HubSpot の取引レコード画面の左サイドバーにあるカード (架電で使う 2 枚) を、「プロパティ」パネルの既定と
 * 「HubSpotのカードから選ぶ」に使う。
 *
 * - 出典: HubSpot の取引 (deal) レコード画面の左サイドバー。2026-10-08 に画面で確認して写した (読み取りのみ)。
 *   カードの構成 (どのプロパティをどの順に出すか) は HubSpot の公開 API で読めないため、ここに書き写している
 * - 架電で使うのは「リスト情報」(最初に開いておく) と「BPOアポ情報」(閉じておく)。
 *   ほかのカード (この取引の概要・クラウドサイン必須項目・事前アンケート回答・請求管理・キャンセル管理・FS商談管理・納品管理) は架電では使わないので入れない
 * - 中身は hubspotCards.json (Rust のテスト src/crm/workspace_tests.rs も同じファイルを読み、案件の読み取りの URL の長さを確かめる)。
 *   HubSpot でカードを変えたら JSON を直す (label は HubSpot の表示名、name は内部名。並びはカードの並び)
 * - 画面に出す表示名は、HubSpot の項目一覧 (GET /api/crm/property-catalog) の表示名を優先し、無ければ label を使う
 */
import cards from './hubspotCards.json';

export interface HubSpotCardItem {
  /** HubSpot のカードに出ている表示名 */
  label: string;
  /** 取引プロパティの内部名 (画面には出さない) */
  name: string;
}

export interface HubSpotCard {
  id: string;
  /** カードの見出し (HubSpot と同じ) */
  title: string;
  /** パネルを開いたときに広げておくか */
  expanded: boolean;
  items: HubSpotCardItem[];
}

/** 写した日 (YYYY-MM-DD) */
export const HUBSPOT_CARDS_CHECKED_AT: string = cards.checked_at;

/** 架電で使うカード (HubSpot の並び) */
export const HUBSPOT_CARDS: readonly HubSpotCard[] = cards.cards;

/** カードの項目の内部名 (重複なし、カードの並び。2 枚に同じ項目があれば最初の位置) */
export function cardPropertyNames(list: readonly HubSpotCard[] = HUBSPOT_CARDS): string[] {
  const out: string[] = [];
  for (const c of list) for (const it of c.items) if (!out.includes(it.name)) out.push(it.name);
  return out;
}
