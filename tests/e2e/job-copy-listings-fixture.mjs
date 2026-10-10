// compose_copy_body の実際の列順。値はすべて架空の求人条件です。
const fixtureHrhColumns = ["案件名", "仕事内容", "通勤経路", "最寄り駅", "キャッチコピー", "メリット", "仕事情報補足1のタイトル", "仕事情報補足2のタイトル", "仕事情報補足3のタイトル", "仕事情報補足4のタイトル", "仕事情報補足1の内容", "仕事情報補足2の内容", "仕事情報補足3の内容", "仕事情報補足4の内容", "雇用形態", "Indeed表示職種名", "応募資格", "給与形態", "基本給与 最小", "基本給与 最大", "タスクの所要時間", "タスクの単位", "平均稼働時間", "平均稼働日数", "固定残業代", "想定残業時間", "条件付き給与1 条件", "条件付き給与1 深夜帯", "条件付き給与1 最小給与", "条件付き給与1 最大給与", "条件付き給与2 条件", "条件付き給与2 深夜帯", "条件付き給与2 最小給与", "条件付き給与2 最大給与", "条件付き給与3 条件", "条件付き給与3 深夜帯", "条件付き給与3 最小給与", "条件付き給与3 最大給与", "給与補足", "試用・研修の有無", "試用・研修時の雇用条件", "試用・研修期の雇用形態", "試用・研修期の給与のタイプ", "試用・研修期の基本給与 最小", "試用・研修期の基本給与 最大", "試用・研修期のタスクの所要時間", "試用・研修期のタスクの単位", "試用・研修期の平均稼働時間", "試用・研修期の平均稼働日数", "試用・研修期の固定残業代", "試用・研修期の想定残業時間", "試用・研修の詳細情報", "勤務時間", "勤務時間帯", "自由項目1のタイトル", "自由項目2のタイトル", "自由項目3のタイトル", "自由項目4のタイトル", "自由項目1の内容", "自由項目2の内容", "自由項目3の内容", "自由項目4の内容", "受動喫煙対策", "受動喫煙についての補足情報", "応募方法", "応募後のプロセス", "採用予定人数"];
export const composeFixtureBody = (values) => fixtureHrhColumns.filter(key => values[key]).map(key => `${key}：${values[key].includes("\n") ? "\n" : ""}${values[key]}`).join("\n");
// Synthetic job listings only. No applicant/contact properties.
export const fixtureJobs = Array.from({ length: 40 }, (_, index) => {
  const id = String(index + 1);
  const hrh = index < 30;
  return { id, media: hrh ? 'hrh' : 'airwork', media_job_id: `${hrh ? 'HR' : 'AW'}-${id}`, account_id: hrh ? null : 'sample-account',
    title: `${hrh ? '配送ドライバー' : '看護スタッフ'}・${['大分', '沖縄', '東京'][index % 3]}${id}`, prefecture: ['大分県', '沖縄県', '東京都'][index % 3], municipality: ['大分市', '那覇市', '新宿区'][index % 3],
    category: hrh ? 'ドライバー' : '看護師', publication_status: index % 5 ? '公開中' : '公開終了', last_csv_detected_at: '2026-10-10T00:00:00Z', application_count: index === 39 ? null : 40 - index };
});
export function fixtureHistory(id) {
  const listing = fixtureJobs.find(row => row.id === id);
  if (!listing) throw new Error('synthetic listing not found');
  const hrh = listing.media === 'hrh';
  const body = (latest) => hrh ? composeFixtureBody({
    案件名: listing.title,
    仕事内容: '決まったルートで日用品を届けます。\n先輩と一緒にルートを覚え、無理なく仕事に慣れていけます。',
    通勤経路: '駅から徒歩10分', 最寄り駅: 'サンプル駅', キャッチコピー: '地域の暮らしを支える配送の仕事', メリット: '同乗研修があります',
    '仕事情報補足1のタイトル': '入社後の流れ', '仕事情報補足2のタイトル': '職場の雰囲気',
    '仕事情報補足1の内容': '先輩と2週間かけてルートを覚えます。', '仕事情報補足2の内容': '困ったときはチームで相談できます。',
    雇用形態: '正社員', Indeed表示職種名: 'ドライバー', 応募資格: '普通自動車免許', 給与形態: '月給',
    '基本給与 最小': latest ? '280000' : '250000', '基本給与 最大': '320000',
    平均稼働時間: '1日8時間', 平均稼働日数: '月20日', 固定残業代: 'なし', 想定残業時間: '月10時間',
    '条件付き給与1 条件': '夜間の配送を担当する場合', '条件付き給与1 深夜帯': '22:00〜翌5:00',
    '条件付き給与1 最小給与': '300000', '条件付き給与1 最大給与': '340000', 給与補足: '交通費支給。経験に応じて相談できます。',
    '試用・研修の有無': 'あり（2週間）', '試用・研修時の雇用条件': '給与が異なります', '試用・研修期の雇用形態': '正社員',
    '試用・研修期の給与のタイプ': '月給', '試用・研修期の基本給与 最小': '240000', '試用・研修期の基本給与 最大': '240000',
    '試用・研修の詳細情報': '研修中は先輩と一緒に配送します。', 勤務時間: '8:00〜17:00（休憩1時間）', 勤務時間帯: '日勤',
    '自由項目1のタイトル': '休日・休暇', '自由項目2のタイトル': '待遇', '自由項目1の内容': '週休2日。年間休日120日',
    '自由項目2の内容': '社会保険完備、制服貸与', '自由項目3の内容': '勤務地は応募時にご確認ください。',
    受動喫煙対策: '屋内禁煙', 応募方法: '仕事の内容を確認してから応募できます。', 応募後のプロセス: '面談で勤務条件を確認します。', 採用予定人数: '2名',
  }) : `落ち着いた環境で利用者の健康を支える看護のお仕事です。\n勤務に慣れるまでチームで支えます。\n時給${latest ? '1,800' : '1,600'}円で、勤務時間は9:00〜18:00です。\nシフトによる週休2日。看護師資格が必要です。`;
  return { listing, current_images: !hrh ? { observed_at: '2026-10-10T00:00:00Z', image_urls: ['https://example.invalid/job-copy/care-1.svg'] } : null, history_counts: hrh ? { hrh_kyuujinhyou_honbun: 2, hrh_kyuujinhyou_gazou: 2 } : { shigotonaiyou: 2 }, history_may_be_incomplete: false,
    versions: id === '39' ? [] : (id === '37' ? [true] : [false, true]).map((latest, index) => ({ written_at: `2026-10-0${index + 1}T00:00:00Z`, body: id === '40' ? '' : body(latest), image_urls: hrh ? [`https://example.invalid/job-copy/delivery-${index}.svg`] : null })) };
}
export const fixtureImage = (care) => `<svg xmlns="http://www.w3.org/2000/svg" width="800" height="360" viewBox="0 0 800 360"><rect width="800" height="360" fill="${care ? '#e3f2ef' : '#e7eef4'}"/><circle cx="660" cy="100" r="95" fill="${care ? '#bfded1' : '#b9d3e2'}"/><rect x="75" y="125" width="340" height="135" rx="20" fill="${care ? '#438572' : '#356882'}"/><path d="M415 155h110l75 65v40H415z" fill="#6496ad"/><circle cx="175" cy="265" r="30" fill="#243c4a"/><circle cx="490" cy="265" r="30" fill="#243c4a"/><text x="75" y="80" font-family="sans-serif" font-size="30" fill="#243c4a">${care ? 'CARE TEAM' : 'LOCAL DELIVERY'}</text></svg>`;
