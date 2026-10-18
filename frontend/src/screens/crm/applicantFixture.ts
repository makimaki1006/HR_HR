import type { ApplicantRow } from '../../generated/ApplicantRow';
import type { ApplicantQueue } from '../../generated/ApplicantQueue';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
const first: ApplicantRow = { id: '90001', name: '架空 はるか', deal: 'サンプル物流株式会社', job: '検品スタッフ（架空求人）', date: '2026-10-18', stage: '朝連絡', stage_id: '1431574804', appointment: '2026-10-18T09:00:00+09:00', media: '架空求人サイト' };
const items: ApplicantRow[] = [
  first,
  { id: '90002', name: '架空 あおい', deal: 'サンプル工房株式会社', job: '組立スタッフ（架空求人）', date: '2026-10-17', stage: '応募受付', stage_id: '1343099551', appointment: '', media: '架空採用ページ' },
];
export const applicantQueueFixture: ApplicantQueue = {
  items, total: 2, next_cursor: null,
  pipelines: [{ id: 'fixture', label: '一次対応_BPO', stages: [{ id: '1431574804', label: '朝連絡' }, { id: '1343099551', label: '応募受付' }] }],
  media_options: [{ id: '架空求人サイト', label: '架空求人サイト' }, { id: '架空採用ページ', label: '架空採用ページ' }],
};
export function applicantDetailFixture(id: string): ApplicantDetail {
  const row = items.find(i => i.id === id) ?? first;
  return {
    row, phone: '000-0000-0000', stop_reason: null, deep_link: `https://app.hubspot.com/contacts/0/record/0-421/${row.id}/`,
    jobs: [{ label: row.job, url: 'https://app.hubspot.com/contacts/0/record/0-420/80001/' }],
    sections: [
      { title: '応募者の情報', fields: [{ label: 'カナ', value: id === '90002' ? 'カクウ アオイ' : 'カクウ ハルカ' }, { label: '年齢', value: '28' }, { label: '性別', value: '回答しない' }, { label: 'メール', value: 'applicant@example.invalid' }] },
      { title: 'この求人の対応ルール', fields: [{ label: '初動対応', value: '応募のお礼を伝え、希望の勤務時間を確認してください。' }, { label: '資格保有確認', value: '必須資格はありません。' }] },
      { title: '応募先', fields: [{ label: '職種', value: '検品' }, { label: '勤務地', value: '架空市サンプル町' }, { label: '勤務時間', value: '9:00〜17:00' }] },
      { title: '応募の内容', fields: [{ label: '面接可能日時', value: '平日午後' }, { label: '質問への回答', value: '研修の内容について知りたいです。' }] },
      { title: 'ヒアリングと結果', fields: [{ label: '保有資格', value: '資格なし' }, { label: '応募動機', value: '通勤しやすい職場を希望' }, { label: '一次対応実施内容', value: '勤務時間の確認待ち' }] },
    ],
    notes: [{ date: '2026-10-18T00:00:00Z', html: '<p>午後の連絡を希望しています。</p>' }],
    sheet_url: id === '90002' ? null : 'https://docs.google.com/spreadsheets/d/fictional-applicants-sheet/edit',
    sheet_embed: id !== '90002',
  };
}
