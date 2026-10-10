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
  const body = (latest) => hrh ? `案件名：${listing.title}\nキャッチコピー：地域の暮らしを支える配送の仕事\nIndeed表示職種名：ドライバー\n仕事内容：決まったルートで日用品を届けます。\n先輩と一緒にルートを覚え、無理なく仕事に慣れていけます。\n給与：月給${latest ? '28' : '25'}万円〜32万円\n給与詳細：交通費支給。経験に応じて相談できます。\n勤務時間：8:00〜17:00（休憩1時間）\n休日：週休2日。年間休日120日\n応募資格：普通自動車免許\n勤務地：${listing.prefecture}${listing.municipality}\n福利厚生：社会保険完備、制服貸与\n応募方法：仕事の内容を確認してから応募できます。`
    : `仕事内容：落ち着いた環境で利用者の健康を支える看護のお仕事です。\n勤務に慣れるまでチームで支えます。\n給与：時給${latest ? '1800' : '1600'}円〜2000円\n勤務時間：9:00〜18:00\n休日：シフトによる週休2日\n応募資格：看護師資格\n勤務地：${listing.prefecture}${listing.municipality}`;
  return { listing, current_images: !hrh ? { observed_at: '2026-10-10T00:00:00Z', image_urls: ['https://example.invalid/job-copy/care-1.svg'] } : null, history_counts: hrh ? { hrh_kyuujinhyou_honbun: 2, hrh_kyuujinhyou_gazou: 2 } : { shigotonaiyou: 2 }, history_may_be_incomplete: false,
    versions: id === '39' ? [] : (id === '37' ? [true] : [false, true]).map((latest, index) => ({ written_at: `2026-10-0${index + 1}T00:00:00Z`, body: id === '40' ? '' : body(latest), image_urls: hrh ? [`https://example.invalid/job-copy/delivery-${index}.svg`] : null })) };
}
export const fixtureImage = (care) => `<svg xmlns="http://www.w3.org/2000/svg" width="800" height="360" viewBox="0 0 800 360"><rect width="800" height="360" fill="${care ? '#e3f2ef' : '#e7eef4'}"/><circle cx="660" cy="100" r="95" fill="${care ? '#bfded1' : '#b9d3e2'}"/><rect x="75" y="125" width="340" height="135" rx="20" fill="${care ? '#438572' : '#356882'}"/><path d="M415 155h110l75 65v40H415z" fill="#6496ad"/><circle cx="175" cy="265" r="30" fill="#243c4a"/><circle cx="490" cy="265" r="30" fill="#243c4a"/><text x="75" y="80" font-family="sans-serif" font-size="30" fill="#243c4a">${care ? 'CARE TEAM' : 'LOCAL DELIVERY'}</text></svg>`;
