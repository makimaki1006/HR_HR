import type { PhoneHearing } from '../../generated/PhoneHearing';

/** Fictional phone records; never derived from production. */
export const demoPhoneHearing: PhoneHearing = {
  total_applicants: 28, truncated: false,
  items: [
    { applicant: '8'.repeat(64), application_date: '2026-09-24', media: '求人検索サイト', multi_listing: false,
      fields: [{ label: '転職理由', text: '通勤に片道1時間かかるため' }, { label: '勤務条件の優先順位', text: '通勤時間、休日、給与の順で希望' }, { label: '入社希望時期', text: '翌月から' }],
      notes: [{ time: '2026-09-24T04:30:00Z', text: '【架電】土日休みと、見学できる日程を確認したい。' }] },
    { applicant: '4'.repeat(64), application_date: '2026-09-16', media: '求人検索サイト', multi_listing: true,
      fields: [{ label: '保有資格', text: '普通自動車免許' }, { label: '応募動機', text: '配送の仕事をしてみたい' }],
      notes: [{ time: '2026-09-16T01:00:00Z', text: '【折り返し】未経験からの研修について質問あり。' }] },
    { applicant: '1'.repeat(64), application_date: '2026-09-02', media: null, multi_listing: false,
      fields: [{ label: '就業状況', text: '在職中' }, { label: '併願状況', text: 'ほかに1件検討中' }], notes: [] },
  ],
};
