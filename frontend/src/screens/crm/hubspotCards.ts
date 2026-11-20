/**
 * HubSpot の取引レコードページ「bpo用」(BPO_リクロジ のチームに割り当て) のカードを、「プロパティ」パネルの既定と
 * 「HubSpotのカードから選ぶ」に使う。
 *
 * - 出典: 左サイドバーの 5 枚 (タスク設定・検索用・管理情報・リスト情報・架電後情報) と、中央の「BPO用」タブの
 *   プロパティのカード 4 枚 (メモ・架電記録・タスク設定・リスト情報 🚨編集不可。見出しに「BPO用｜」を付けた)。
 *   2026-11-16 にレイアウトの編集画面を保存せずに開いて写した (読み取りのみ)。カードの構成は HubSpot の公開 API で読めない
 * - 2026-11-16 の BPO の会議「FS用から BPO用のプロパティに変更」で、以前の 3 枚 (リスト情報・BPOアポ情報・架電結果) から置き換えた
 * - 同じ項目が 2 枚に出るのは HubSpot と同じ
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

/** bpo用 のカード (HubSpot の並び) */
export const HUBSPOT_CARDS: readonly HubSpotCard[] = cards.cards;

/** カードの項目の内部名 (重複なし、カードの並び。2 枚に同じ項目があれば最初の位置) */
export function cardPropertyNames(list: readonly HubSpotCard[] = HUBSPOT_CARDS): string[] {
  const out: string[] = [];
  for (const c of list) for (const it of c.items) if (!out.includes(it.name)) out.push(it.name);
  return out;
}
