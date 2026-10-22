import type { ApplicantViewer } from '../../generated/ApplicantViewer';
import type { ApplicantRow } from '../../generated/ApplicantRow';
import type { ApplicantQueue } from '../../generated/ApplicantQueue';
import type { ApplicantDetail } from '../../generated/ApplicantDetail';
import type { ApplicantReminders } from '../../generated/ApplicantReminders';
const first: ApplicantRow = { id: '90001', name: '架空 はるか', phone: '+815000000000', deal: 'サンプル物流株式会社', job: '検品スタッフ（架空求人）', date: '2026-10-18', stage: '朝連絡', stage_id: '1431574804', appointment: '2026-10-18T09:00:00+09:00', media: '架空求人サイト' };
const items: ApplicantRow[] = [
  first,
  { id: '90002', name: '架空 あおい', phone: '000-0000-0000', deal: 'サンプル工房株式会社', job: '組立スタッフ（架空求人）', date: '2026-10-17', stage: '応募受付', stage_id: '1343099551', appointment: '', media: '架空採用ページ' },
];
export const applicantQueueFixture: ApplicantQueue = {
  items, total: 2, next_cursor: null,
  pipelines: [{ id: 'fixture', label: '一次対応_BPO', stages: [{ id: '1431574804', label: '朝連絡' }, { id: '1343099551', label: '応募受付' }, { id: 'fixture-filter-grace', label: '足切り猶予期間' }, { id: 'fixture-filter-docs', label: '選考書類待ち' }, { id: 'fixture-filter-call', label: '約束あり連絡' }] }],
  media_options: [{ id: '架空求人サイト', label: '架空求人サイト' }, { id: '架空採用ページ', label: '架空採用ページ' }],
};
const outside: ApplicantRow = { ...first, id: '90003', name: '架空 そら', deal: 'サンプル商店株式会社', stage: '採用済み', stage_id: 'fixture-other-done', appointment: '', date: '2026-10-19' };
export const applicantAllQueueFixture: ApplicantQueue = {
  ...applicantQueueFixture, items: [outside, ...items], total: 3,
  pipelines: [...applicantQueueFixture.pipelines, { id: 'fixture-other', label: '採用管理（架空）', stages: [{ id: outside.stage_id, label: outside.stage }] }],
};
export function applicantDetailFixture(id: string, search = window.location.search): ApplicantDetail {
  const row = applicantAllQueueFixture.items.find(i => i.id === id) ?? first;
  return {
    values: { hs_pipeline: id === outside.id ? 'fixture-other' : 'fixture', hs_pipeline_stage: row.stage_id, renrakukakuninmokuteki: '10/18 午後の連絡を希望', meeruadoresu: 'applicant@example.invalid', denwabangou: row.phone, hoyuushikaku_hiaringu: 'none', shuugyoujoukyou: '旧項目: 在職中', nyuushakiboujiki: '旧項目: 相談' },
    writes_enabled: id !== outside.id && new URLSearchParams(search).get('writes') !== 'off',
    write_pipeline: id === outside.id ? null : { id: 'fixture', label: '一次対応_BPO', stages: [...(applicantQueueFixture.pipelines[0]?.stages.filter(s => !s.id.startsWith('fixture-filter-')) ?? []), ...['昼連絡','夕連絡','約束あり連絡','選考書類待ち','足切り猶予期間','一次対応実施済'].map((label, i) => ({id:`fixture-stage-${String(i)}`,label}))] },
    editable: [
      ...[['taioushubetsu','対応種別'],['ichijitaioujisshinaiyou','一次対応実施内容'],['ouboshahenobaitaimerusoufu','応募者への媒体メール送付'],['senkoushoruikaishuu','選考書類回収'],['ouboshajouhoukanrishito','応募者情報管理シート'],['bpotaishougairiyuu','BPO対象外理由']].map(([name, label]) => ({name:name ?? '',label:label ?? '',property_type:'enumeration',field_type:'select',options:[{value:'done',label:'確認済み',hidden:false},{value:'waiting',label:'確認待ち',hidden:false}],section:'結果'})),
      {name:'renrakukakuninmokuteki',label:'連絡・確認目的詳細',property_type:'string',field_type:'textarea',options:[],section:'結果'},
      {name:'yakusokukadennichiji',label:'約束架電日時',property_type:'datetime',field_type:'date',options:[],section:'結果'},
      ...[['ichijitaiou_baitai_message_nichiji','一次対応_媒体メッセージ送信日時'],['senkoushoruiteishutsukigenbi','選考書類提出期限日'],['ashigiriketteibi','足切り決定日']].map(([name,label]) => ({name:name ?? '',label:label ?? '',property_type:'date',field_type:'date',options:[],section:'結果'})),
      ...[['ichijitaiou_shuugyou_joukyou','就業状況'],['ichijitaiou_nyuusha_kibou_jiki','入社希望時期']].map(([name,label]) => ({name:name ?? '',label:label ?? '',property_type:'enumeration',field_type:'select',options:[{value:'consult',label:'相談したい',hidden:false}],section:'ヒアリング'})),
      {name:'hoyuushikaku_hiaringu',label:'保有資格',property_type:'enumeration',field_type:'select',options:[{value:'none',label:'資格なし',hidden:false}],section:'ヒアリング'},
      ...[['kanrengyoumukeiken','関連業務経験'],['genshokumaeshokukaranotenshokuriyuu','転職理由'],['oubodouki','応募動機'],['kinmujoukennoyuusenjuni','勤務条件の優先順位'],['oubonoheiganjoukyou','併願状況'],['kyuujinkensakujouken','求人検索条件'],['shigotosagashinohanikoutsuushudan','仕事探しの範囲'],['mensetsukanounichiji','面接可能日時'],['renrakukanouyoubijikantai','連絡可能曜日・時間帯'],['tokukijikou','特記事項'],['ichijitaiou_azukatta_shitsumon','預かった質問'],['ichijitaiou_jitai_riyuu','辞退理由'],['ichijitaiou_shokan_memo','所感メモ']].map(([name,label]) => ({name:name ?? '',label:label ?? '',property_type:'string',field_type:'textarea',options:[],section:'ヒアリング'})),
      ...[['meeruadoresu','メールアドレス'],['denwabangou','電話番号']].map(([name,label]) => ({name:name ?? '',label:label ?? '',property_type:'string',field_type:'text',options:[],section:'連絡先'})),
    ],
    listings: [{ id: '80001', title: row.job, deep_link: 'https://app.hubspot.com/contacts/0/record/0-420/80001/', jobcopy_url: 'https://www.example.com/app/job-copy/fictional-posting', current: false, fields: [
      { label: '求人状態', value: '公開中' }, { label: '媒体', value: '架空求人サイト' }, { label: '勤務地', value: '架空市サンプル町' }, { label: '給与形態', value: '時給' }, { label: '職種', value: '検品スタッフ' }, { label: '公開日', value: '2026/10/01' }, { label: '終了日', value: '2026/11/30' },
    ] }],
    deals: [{ id: '70001', title: row.deal, deep_link: 'https://app.hubspot.com/contacts/0/record/0-3/70001/', jobcopy_url: null, current: true, fields: [{ label: '取引ステージ', value: '契約中' }, { label: '取引担当者', value: '架空 担当' }, { label: '契約開始日', value: '2026/10/01' }] },
      { id: '70002', title: 'サンプル物流の以前の契約', deep_link: 'https://app.hubspot.com/contacts/0/record/0-3/70002/', jobcopy_url: null, current: false, fields: [{ label: '取引ステージ', value: '解約済み' }] }],
    related_notice: null,
    row, phone: row.phone, stop_reason: null, deep_link: `https://app.hubspot.com/contacts/0/record/0-421/${row.id}/`,
    jobs: [{ label: row.job, url: 'https://app.hubspot.com/contacts/0/record/0-420/80001/' }],
    sections: [
      { title: '応募者の情報', fields: [{ label: 'カナ', value: id === '90002' ? 'カクウ アオイ' : 'カクウ ハルカ' }, { label: '年齢', value: '28' }, { label: '性別', value: '回答しない' }, { label: 'メール', value: 'applicant@example.invalid' }] },
      { title: 'この求人の対応ルール', fields: [{ label: '初動対応', value: '応募のお礼を伝え、希望の勤務時間を確認してください。' }, { label: '資格保有確認', value: '必須資格はありません。' }] },
      { title: '応募先', fields: [{ label: '職種', value: '検品' }, { label: '勤務地', value: '架空市サンプル町' }, { label: '勤務時間', value: '9:00〜17:00' }] },
      { title: '応募の内容', fields: [{ label: '面接可能日時', value: '平日午後' }, { label: '質問への回答', value: '研修の内容について知りたいです。' }] },
      { title: 'ヒアリングと結果', fields: [{ label: '保有資格', value: '資格なし' }, { label: '以前の就業状況', value: '在職中' }, { label: '以前の入社希望時期', value: '相談' }, { label: '応募動機', value: '通勤しやすい職場を希望' }, { label: '一次対応実施内容', value: '勤務時間の確認待ち' }] },
    ],
    notes: [{ date: '2026-10-18T00:00:00Z', html: '<p>午後の連絡を希望しています。</p>' }],
    posting_url: Number(id) % 2 === 0 ? null : 'https://www.example.com/jobs/fictional-inspection',
    jobcopy_url: 'https://www.example.com/app/job-copy/fictional-posting',
    sheet_url: id === '90002' ? null : 'https://docs.google.com/spreadsheets/d/fictional-applicants-sheet/edit',
    sheet_embed: id !== '90002',
  };
}
/** 約束架電の通知の架空サンプル。1 件は 5 分前を過ぎ、もう 1 件はまだ先 (帯に出ない)。 */
export function applicantRemindersFixture(now = Date.now()): ApplicantReminders {
  const promised = { stage: '約束あり連絡', stage_id: '1431576654' };
  const rows = [
    { ...first, id: '90005', name: '架空 みさき', ...promised, appointment: new Date(now + 2 * 60_000).toISOString() },
    { ...first, id: '90004', name: '架空 みなと', ...promised, appointment: new Date(now + 20 * 60_000).toISOString() },
  ];
  return { items: rows, total: rows.length };
}
/** 開いている人の目印の架空サンプル。90002 を別の人が開き、90005 (約束架電) も別の人が開いている。 */
export const applicantPresenceFixture: ApplicantViewer[] = [
  { applicant: '90002', phone_key: '', name: 'kakuu_sato' },
  { applicant: '90005', phone_key: '', name: 'kakuu_suzuki' },
];
