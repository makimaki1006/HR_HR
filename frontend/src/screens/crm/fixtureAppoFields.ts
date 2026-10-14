// 架空サンプル用: 「必須項目が多いステージ」と「担当者を選ぶ型の項目」の確認用データ (実在の項目・人は含まない)。
// ステージを移す画面の見出し分け・2 列・必須の残り数・担当者の select を、バックエンド無しで確かめるためのもの。
import type { CrmCatalogProperty } from '../../generated/CrmCatalogProperty';

const p = (name: string, label: string, property_type: string, field_type: string, options: { value: string; label: string }[] = [], owner_ref = false): CrmCatalogProperty =>
  ({ name, label, property_type, field_type, options: options.map(o => ({ ...o, display_order: 0, hidden: false })), owner_ref });

/** 担当者を選ぶ型の項目。「アポ獲得者」は本人が最初から入る (writeModel の APPOINTER_FIELDS) */
const APPOINTER = p('bpo_26', 'アポ獲得者', 'enumeration', 'select', [], true);

/** HubSpot のプロパティのグループにあたる見出しごとの項目 (架空) */
export const FIXTURE_APPO_GROUPS: { name: string; label: string; properties: CrmCatalogProperty[] }[] = [
  { name: 'fx_appo_info', label: 'アポ情報(架空)', properties: [
    p('fx_appo_company', 'アポ先の会社名', 'string', 'text'), p('fx_appo_contact', 'アポ先のご担当者名', 'string', 'text'),
    p('fx_appo_dept', 'ご担当者の部署', 'string', 'text'), p('fx_appo_title', 'ご担当者の役職', 'string', 'text'),
    p('fx_appo_phone', 'アポ先の電話番号', 'string', 'phonenumber'), p('fx_appo_date', 'アポ日', 'date', 'date'),
    p('fx_appo_time', 'アポ時間帯', 'string', 'text'), p('fx_appo_place', 'アポの場所', 'string', 'text'), APPOINTER,
  ] },
  { name: 'fx_hearing', label: 'ヒアリング内容(架空)', properties: [
    p('fx_hear_headcount', '採用予定人数', 'number', 'number'), p('fx_hear_budget', '採用予算(万円)', 'number', 'number'),
    p('fx_hear_period', '採用時期', 'string', 'text'),
    p('fx_hear_need', '採用の緊急度', 'enumeration', 'select', [{ value: 'high', label: '高い' }, { value: 'mid', label: '普通' }, { value: 'low', label: '低い' }]),
    p('fx_hear_media', '利用中の媒体', 'string', 'text'), p('fx_hear_decider', '決裁者', 'string', 'text'),
    p('fx_hear_issue', '採用の課題', 'string', 'textarea'), p('fx_hear_next', '次回の連絡日', 'date', 'date'),
  ] },
  { name: 'fx_contract', label: '契約・連絡先(架空)', properties: [
    p('fx_contract_type', '想定する契約の種類', 'enumeration', 'select', [{ value: 'a', label: 'プランA' }, { value: 'b', label: 'プランB' }]),
    p('fx_contract_mail', '連絡用メール', 'string', 'text'), p('fx_contract_fee', '契約料の目安(万円)', 'number', 'number'),
    p('fx_contract_memo', '引き継ぎメモ', 'string', 'textarea'),
  ] },
];

/** 「アポ日確定(多項目・確認用)」の必須項目 (21 項目) */
export const FIXTURE_APPO_REQUIRED: string[] = FIXTURE_APPO_GROUPS.flatMap(g => g.properties.map(x => x.name));
