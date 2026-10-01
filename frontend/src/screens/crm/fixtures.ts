import type { Activity, CrmRecord, RecordProperty } from './model';

const property = (name: string, label: string, value: string | null): RecordProperty => ({ name, label, value });
const call: Activity = {
  id: 'sample-call-1', type: 'call', title: '採用計画について初回ヒアリング',
  body: '物流拠点での採用について情報収集中。午前中は会議が多いため、次回は午後の連絡を希望。担当者への取り次ぎ方法を確認した。',
  occurredAt: '2026-09-30T05:30:00Z', owner: '佐々木 葵', outcome: '接続済み',
};
const note: Activity = {
  id: 'sample-note-1', type: 'note', title: '次回連絡に向けた申し送り',
  body: '採用人数と開始時期を確認する。前回の説明を繰り返す前に、検討状況を伺う。',
  occurredAt: '2026-09-30T05:40:00Z', owner: '佐々木 葵', outcome: null,
};
const task: Activity = {
  id: 'sample-task-1', type: 'task', title: '採用時期を確認するフォローコール',
  body: 'サンプル期限: 2026年10月2日 14:00。採用担当者に検討状況を確認する。',
  occurredAt: '2026-09-30T05:45:00Z', owner: '佐々木 葵', outcome: '未完了',
};

/** Entirely fictional records. No real customer IDs, emails or dialable phone numbers. */
export const SAMPLE_RECORDS: CrmRecord[] = [
  {
    id: 'sample-contact-1', objectType: 'contacts', name: '高橋 美咲',
    subtitle: '採用担当 / サンプル物流株式会社', owner: '佐々木 葵', status: 'コンタクト済み',
    updatedAt: '2026-09-30T05:45:00Z', deepLink: null,
    properties: [property('email', 'Eメール', 'misaki.takahashi@example.com'),
      property('phone', '電話番号', 'サンプル（発信不可）'), property('jobtitle', '役職', '採用担当'),
      property('hubspot_owner_id', 'コンタクト担当者', '佐々木 葵'),
      property('hs_lead_status', 'リードステータス', 'コンタクト済み'),
      property('lifecyclestage', 'ライフサイクルステージ', 'リード'),
      property('notes_last_contacted', '最終コンタクト', '2026/09/30 14:30')],
    activities: [task, note, call, {
      id: 'sample-email-1', type: 'email', title: 'サービス概要のご案内',
      body: '採用支援サービスの概要資料をご案内しました。',
      occurredAt: '2026-09-29T07:00:00Z', owner: '山本 健', outcome: null,
    }],
    associations: [{ objectType: 'companies', id: 'sample-company-1', label: '主たる会社' },
      { objectType: 'deals', id: 'sample-deal-1', label: null }],
  },
  {
    id: 'sample-contact-2', objectType: 'contacts', name: '森 大輔',
    subtitle: '総務部長 / サンプル製作所株式会社', owner: '山本 健', status: '未コンタクト',
    updatedAt: '2026-09-29T01:00:00Z', deepLink: null,
    properties: [property('email', 'Eメール', 'daisuke.mori@example.com'),
      property('phone', '電話番号', null), property('jobtitle', '役職', '総務部長'),
      property('hubspot_owner_id', 'コンタクト担当者', '山本 健'),
      property('hs_lead_status', 'リードステータス', '未コンタクト'),
      property('lifecyclestage', 'ライフサイクルステージ', 'リード')], activities: [],
    associations: [{ objectType: 'companies', id: 'sample-company-2', label: '主たる会社' }],
  },
  {
    id: 'sample-contact-3', objectType: 'contacts', name: '小林 直子',
    subtitle: '拠点責任者 / サンプル物流株式会社', owner: '佐々木 葵', status: '検討中',
    updatedAt: '2026-09-28T06:00:00Z', deepLink: null,
    properties: [property('email', 'Eメール', 'naoko.kobayashi@example.com'),
      property('phone', '電話番号', 'サンプル（発信不可）'), property('jobtitle', '役職', '拠点責任者'),
      property('hubspot_owner_id', 'コンタクト担当者', '佐々木 葵'),
      property('hs_lead_status', 'リードステータス', '検討中')],
    activities: [{ id: 'sample-meeting-1', type: 'meeting', title: '採用課題の情報交換',
      body: '現場の採用要件と勤務条件について確認。次回は本社の採用担当者を交えて相談する。',
      occurredAt: '2026-09-28T06:00:00Z', owner: '佐々木 葵', outcome: '実施済み' }],
    associations: [{ objectType: 'companies', id: 'sample-company-1', label: '主たる会社' }],
  },
  {
    id: 'sample-company-1', objectType: 'companies', name: 'サンプル物流株式会社',
    subtitle: 'sample-logistics.example.com', owner: '佐々木 葵', status: 'リード',
    updatedAt: '2026-09-30T05:45:00Z', deepLink: null,
    properties: [property('domain', '会社ドメイン名', 'sample-logistics.example.com'),
      property('phone', '電話番号', 'サンプル（発信不可）'), property('industry', '業種', '運輸・物流'),
      property('city', '市区町村', '大分市'), property('numberofemployees', '従業員数', '120'),
      property('hubspot_owner_id', '会社担当者', '佐々木 葵'), property('lifecyclestage', 'ライフサイクルステージ', 'リード')],
    activities: [note, call], associations: [
      { objectType: 'contacts', id: 'sample-contact-1', label: null },
      { objectType: 'contacts', id: 'sample-contact-3', label: null },
      { objectType: 'deals', id: 'sample-deal-1', label: null }],
  },
  {
    id: 'sample-company-2', objectType: 'companies', name: 'サンプル製作所株式会社',
    subtitle: 'sample-manufacturing.example.com', owner: '山本 健', status: 'リード',
    updatedAt: '2026-09-29T01:00:00Z', deepLink: null,
    properties: [property('domain', '会社ドメイン名', 'sample-manufacturing.example.com'),
      property('phone', '電話番号', null), property('industry', '業種', '製造'),
      property('city', '市区町村', '別府市'), property('hubspot_owner_id', '会社担当者', '山本 健')],
    activities: [], associations: [{ objectType: 'contacts', id: 'sample-contact-2', label: null }],
  },
  {
    id: 'sample-deal-1', objectType: 'deals', name: 'サンプル物流 — 採用支援',
    subtitle: '新規営業 / 課題ヒアリング', owner: '佐々木 葵', status: '課題ヒアリング',
    updatedAt: '2026-09-30T05:40:00Z', deepLink: null,
    properties: [property('hubspot_owner_id', '取引担当者', '佐々木 葵'),
      property('pipeline', 'パイプライン', '新規営業'), property('dealstage', '取引ステージ', '課題ヒアリング'),
      property('amount', '金額', '¥300,000'), property('closedate', 'クローズ日', '2026/11/30')],
    activities: [note, call], associations: [
      { objectType: 'companies', id: 'sample-company-1', label: null },
      { objectType: 'contacts', id: 'sample-contact-1', label: null }],
  },
];
