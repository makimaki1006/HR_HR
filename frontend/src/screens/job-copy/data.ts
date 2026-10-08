import type { CopyImage } from './images';
import type { ApplicantDimension, ApplicantDistribution } from './applicantCompositionModel';
import type { ApplicantReasonCollection } from './applicantReasonsModel';
import type { HrhPerformanceCollection } from './hrhPerformanceModel';
import type { JointDemographics } from './reverseSearchModel';
import type { MarketData } from './marketChartModel';
/** Fictional MOC fixtures. No HubSpot IDs, real employers, or applicant data. */
export interface CopyVersion {
  id: string;
  label: string;
  observedAt: string;
  publishedFrom?: string;
  publishedUntil?: string;
  certainty: 'confirmed' | 'estimated' | 'unknown';
  kind: 'published' | 'received' | 'ai_draft';
  source: string;
  body: string;
  applications: { confirmed: number; estimated: number; unknown: number } | null;
  images?: CopyImage[];
  imageReferences?: { referenceHash: string; slot: number }[];
  historicalImageBytesAvailable?: boolean;
  note: string;
  distributions?: Partial<Record<ApplicantDimension, ApplicantDistribution>>;
  observationDates?: string[];
  attributesFetchedAt?: string;
  observedPublicationStatus?: string;
}

export interface JobCopyRecord {
  id: string;
  hubspotId?: string;
  dataSource?: 'hubspot';
  hubspotUrl?: string;
  attributionUnknown?: number;
  overallApplications?: {
    total: number; missingDate: number; fetchedAt: string; distributions: Partial<Record<ApplicantDimension, ApplicantDistribution>>; byDate?: Record<string, number>;
    /**
     * Applications that HubSpot also links to another job, by application date (included in byDate
     * and total). Absent when the source did not check. They are never put into a version's period.
     */
    multiListing?: { byDate: Record<string, number>; missingDate: number };
  };
  applicantReasons?: ApplicantReasonCollection | undefined;
  hrhPerformance?: HrhPerformanceCollection | undefined;
  jointDemographics?: JointDemographics | undefined;
  title: string;
  company: string;
  media: string;
  mediaJobId: string;
  /**
   * The account the media job ID belongs to: the HRハッカー shop ID (id_shop_hrhakkaa) or the
   * Airワーク account login ID (airwork_account_login_id). Billing rows are matched on media +
   * this + mediaJobId, never on the job ID alone. Absent when not acquired.
   */
  accountId?: string;
  location: string;
  versions: CopyVersion[];
}

const driverOriginal = "キャッチコピー：いつもの道で、地域の暮らしを支える配送の仕事。\n\n仕事内容\n大分市周辺の店舗へ日用品を届けます。出発前に伝票と荷物を照合し、商品の状態を確認してから積み込みます。店舗では納品先の担当者と数量を確認して引き渡します。\n\n配送先ごとの受け入れ時間や荷下ろしの場所は、先輩と一緒に覚えます。帰社後は未配達品の確認、伝票整理、車内の片付けを行い、翌日の準備につなげます。\n\n仕事の進め方\n朝礼でルートと注意事項を共有します。遅れや荷物の破損に気づいた場合は事務所へ連絡し、安全確認を優先して対応します。配送記録は帰社時に担当者と照合します。\n\n職場紹介\n配車担当とドライバーが同じ拠点で働く配送センターです。道路状況や納品先の変更を共有し、互いの配送を支えています。\n\n入社後の流れ\n初日は車両点検と安全ルールを確認します。2週間の同乗研修で積み込みと配送を経験し、担当ルートを覚えます。\n\n応募後の流れ\n応募内容を確認後、担当者から面談日程をご連絡します。仕事内容と勤務条件を説明し、入社日の希望を伺います。" + '\n\n募集条件\n' + '職種：配送ドライバー\n仕事内容：地域の店舗へ日用品を配送します。1日8件程度を担当します。\n給与：月給250,000円〜280,000円\n勤務時間：8:00〜17:00（休憩60分）\n休日：週休2日、日曜休み\n応募条件：普通自動車免許。配送経験は必須ではありません。\n待遇：交通費支給、同乗研修2週間。';
const driverRevision = "キャッチコピー：土日は自分の時間に。地域の暮らしを支える配送の仕事。\n\n仕事内容\n大分市周辺の店舗へ日用品を届けます。出発前に伝票と荷物を照合し、商品の状態を確認してから積み込みます。店舗では納品先の担当者と数量を確認して引き渡します。\n\n配送件数を見直し、店舗での確認や荷下ろしに余裕を持てるルートを組んでいます。帰社後は未配達品の確認、伝票整理、車内の片付けを行い、翌日の準備につなげます。\n\n仕事の進め方\n朝礼でルートと注意事項を共有します。遅れや荷物の破損に気づいた場合は事務所へ連絡し、安全確認を優先して対応します。配送記録は帰社時に担当者と照合します。\n\n職場紹介\n配車担当とドライバーが同じ拠点で働く配送センターです。道路状況や納品先の変更を共有し、互いの配送を支えています。\n\n入社後の流れ\n初日は車両点検と安全ルールを確認します。4週間の同乗研修で積み込みと配送を経験し、習熟度を確認してルートを引き継ぎます。\n\n応募後の流れ\n応募内容を確認後、担当者から面談日程をご連絡します。仕事内容と勤務条件を説明し、入社日の希望を伺います。" + '\n\n募集条件\n' + '職種：配送ドライバー\n仕事内容：地域の店舗へ日用品を配送します。1日6件程度を担当します。\n給与：月給270,000円〜300,000円\n勤務時間：7:00〜16:00（休憩60分）\n休日：週休2日、土日休み\n応募条件：普通自動車免許。配送経験は必須ではありません。\n待遇：交通費支給、同乗研修4週間。';

export const jobs: JobCopyRecord[] = [
  {
    id: 'demo-job-001', title: '地域配送ドライバー', company: 'デモ運輸A',
    media: 'HRハッカー', mediaJobId: 'DEMO-HRH-001', accountId: 'DEMO-SHOP-01', location: '大分県大分市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 3,
    overallApplications: { total: 28, missingDate: 10, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-02': 1, '2026-09-04': 2, '2026-09-07': 1, '2026-09-10': 2, '2026-09-13': 1, '2026-09-16': 3, '2026-09-18': 2, '2026-09-21': 2, '2026-09-24': 1, '2026-09-27': 1, '2026-09-30': 1, '2026-10-02': 1 } },
    // HRハッカーの期間別実績（架空の課金例）
    hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: 'DEMO-HRH-001', captured_at: '2026-10-05T00:00:00Z', rows: [
      { period_start: '2026-09-01', period_end: '2026-09-14', impressions: 4200, clicks: 160, cost_yen: 30000, applications: 6 },
      { period_start: '2026-09-15', period_end: '2026-09-30', impressions: 5100, clicks: 210, cost_yen: 45000, applications: 9 },
      { period_start: '2026-10-01', period_end: '2026-10-05', impressions: 1300, clicks: 52, cost_yen: 12000, applications: 2 },
    ] },
    versions: [
      {
        id: 'demo-001-v1', label: '初回掲載', observedAt: '2026-09-01T09:00:00+09:00',
        publishedFrom: '2026-09-01T09:00:00+09:00', publishedUntil: '2026-09-15T10:00:00+09:00',
        certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ・初回取得）', body: driverOriginal,
        applications: { confirmed: 12, estimated: 0, unknown: 0 },
        note: '媒体更新日時と応募日時が揃った架空例。応募数はデモ用の固定値です。',
      },
      {
        id: 'demo-001-v2', label: '給与・勤務条件変更', observedAt: '2026-09-15T10:30:00+09:00',
        publishedFrom: '2026-09-15T10:00:00+09:00', publishedUntil: '2026-09-25T12:00:00+09:00',
        certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ・更新取得）', body: driverRevision,
        applications: { confirmed: 9, estimated: 0, unknown: 2 },
        note: '日時が不明な応募2件は、この版に応募日で結びついた件数に含めません。全件架空です。',
      },
      {
        id: 'demo-001-v3', label: '初回文面への復帰', observedAt: '2026-09-25T12:30:00+09:00',
        publishedFrom: '2026-09-25T12:00:00+09:00', certainty: 'confirmed', kind: 'published',
        source: '求人CSV（デモ・元の本文に戻した版）', body: driverOriginal,
        applications: { confirmed: 4, estimated: 0, unknown: 1 },
        note: '初回版と同じ本文へ戻った例。本文を再利用しても掲載期間は別です。応募数は架空です。',
      },
      {
        id: 'demo-001-draft', label: 'ヒアリングからのAI案', observedAt: '2026-10-02T15:00:00+09:00',
        certainty: 'unknown', kind: 'ai_draft', source: 'ヒアリング文字起こし（架空・AI案の例）',
        body: "キャッチコピー：初めての配送も、隣の先輩と一歩ずつ。（未掲載の架空AI案）\n\n仕事内容\n大分市周辺の店舗へ洗剤や生活用品を届ける仕事です。伝票に沿って荷物を確認し、店舗ごとの受け入れ場所へ運び、担当者と数量を照合します。\n\n道順だけでなく、荷物を傷つけない積み方や店舗での挨拶も大切な仕事です。戻ったら伝票と実績を整理し、車両の状態を確認します。担当件数や荷物の重さは掲載前の確認事項です。\n\n仕事の進め方\n出発前に配車担当とルートを共有します。遅れや納品先からの相談は事務所へ連絡し、対応方法を確認します。\n\n職場紹介\n配車担当と配送スタッフが連携する拠点を想定しています。職場の人数、休憩設備、車両の種類はヒアリングで確認してから掲載します。\n\n入社後の流れ\nヒアリングの発言を基に、先輩が4週間同乗する研修案を記載しています。期間と独り立ちの基準は企業の確認が必要です。\n\n応募後の流れ\n面談と業務説明を想定しています。選考回数や連絡方法は未確認です。この文面は架空の案であり、公開前に担当者が確認します。" + '\n\n募集条件\n' + '職種：地域配送ドライバー\n仕事内容：地域の店舗へ日用品を配送します。入社後4週間は先輩社員が同乗します。\n給与：月給250,000円〜280,000円\n勤務時間：8:00〜17:00（休憩60分）\n休日：週休2日、日曜休み\n応募条件：普通自動車免許。未経験者も応募できます。\n待遇：交通費支給。研修期間や給与条件は掲載前に確認が必要です。',
        applications: { confirmed: 0, estimated: 0, unknown: 0 },
        note: '未掲載の架空案。応募集計の対象外です。LLMは実行していません。根拠例：研修についてのヒアリング発言。',
      },
    ],
  },
  {
    id: 'demo-job-002', title: '倉庫内ピッキングスタッフ', company: 'デモ物流B',
    media: 'Airワーク', mediaJobId: 'DEMO-AIR-002', accountId: 'DEMO-ACCOUNT-01', location: '大分県別府市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 2,
    overallApplications: { total: 12, missingDate: 5, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-06': 1, '2026-09-11': 1, '2026-09-17': 1, '2026-09-22': 2, '2026-09-26': 1, '2026-10-01': 1 } },
    versions: [
      {
        id: 'demo-002-v1', label: '初回取得', observedAt: '2026-09-05T09:00:00+09:00',
        publishedFrom: '2026-09-05T09:00:00+09:00', publishedUntil: '2026-09-20T09:00:00+09:00',
        certainty: 'estimated', kind: 'published', source: '求人XLSX（デモ）',
        body: "キャッチコピー：食品が並ぶ棚から、地域のお店へつなぐ仕事。\n\n仕事内容\n出荷リストを見ながら食品を棚から取り出し、店舗別に仕分けます。名称、数量、賞味期限を照合し、箱の破損や袋の汚れがないかを確認します。\n\n棚の商品番号と現物を見比べて作業を進めます。数量が合わない場合は確認担当に報告して記録を残します。重量物の運搬を含まない持ち場を担当します。\n\n仕事の進め方\n始業時に出荷予定を確認し、担当エリアでピッキングします。仕分け後は別のスタッフと検品し、完了したリストを提出します。\n\n職場紹介\n商品管理と出荷の担当が連携する食品倉庫です。決められた保管場所と衛生ルールを守り、整理された通路で作業します。\n\n入社後の流れ\n衛生管理と棚の配置を学びます。先輩とリストの見方や検品を練習し、扱う商品を少しずつ覚えます。\n\n応募後の流れ\n担当者が勤務可能な曜日を確認します。面談で作業内容と勤務時間を説明し、希望の勤務開始日を相談します。" + '\n\n募集条件\n' + '職種：倉庫内ピッキング\n仕事内容：食品の仕分けと検品。\n給与：時給1,100円\n勤務時間：9:00〜15:00、週3日から\n応募条件：経験不問。重量物の運搬はありません。',
        applications: { confirmed: 3, estimated: 2, unknown: 1 }, note: '掲載開始は取得日時からの推定。応募数は架空の固定値です。',
      },
      {
        id: 'demo-002-v2', label: '時給・担当業務変更', observedAt: '2026-09-20T09:00:00+09:00',
        publishedFrom: '2026-09-20T09:00:00+09:00', certainty: 'estimated', kind: 'published', source: '求人XLSX（デモ）',
        body: "キャッチコピー：検品から運搬まで、食品の出荷をチームで支える仕事。\n\n仕事内容\n出荷リストを見ながら食品を棚から取り出し、店舗別に仕分けます。名称、数量、賞味期限を照合し、箱の破損や袋の汚れがないかを確認します。\n\n担当範囲には食品ケースなど重量物の運搬を含みます。台車の通路と積載ルールを確認し、持ち上げに不安がある荷物は担当者へ相談します。数量の不足は記録して報告します。\n\n仕事の進め方\n始業時に出荷予定と運搬の分担を確認します。ピッキングした後、別のスタッフと検品し、ケースを所定の出荷場所へ移動します。\n\n職場紹介\n商品管理と出荷の担当が連携する食品倉庫です。決められた保管場所と衛生ルールを守り、整理された通路で作業します。\n\n入社後の流れ\n衛生管理と棚の配置に加え、台車の扱い方を学びます。先輩と検品と運搬を練習してから持ち場を担当します。\n\n応募後の流れ\n担当者が勤務可能な曜日を確認します。面談で重量物の取り扱いを説明し、希望の勤務開始日を相談します。" + '\n\n募集条件\n' + '職種：倉庫内ピッキング\n仕事内容：食品の仕分けと検品。\n給与：時給1,200円\n勤務時間：9:00〜16:00、週3日から\n応募条件：経験不問。重量物の運搬があります。',
        applications: { confirmed: 2, estimated: 3, unknown: 1 }, note: '「ありません」から「あります」への変更を含む架空例。取得間の応募は確定配賦しません。',
      },
    ],
  },
  {
    id: 'demo-job-003', title: '受付事務スタッフ', company: 'デモサービスC',
    media: 'HRハッカー', mediaJobId: 'DEMO-HRH-003', accountId: 'DEMO-SHOP-01', location: '大分県大分市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 0,
    overallApplications: { total: 12, missingDate: 2, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-09': 1, '2026-09-12': 2, '2026-09-19': 1, '2026-09-23': 2, '2026-09-25': 1, '2026-09-29': 2, '2026-10-03': 1 } },
    // HRハッカーの期間別実績（架空の課金例）
    hrhPerformance: { schema_version: 1, source: 'hrhacker', job_id: 'DEMO-HRH-003', captured_at: '2026-10-05T00:00:00Z', rows: [
      { period_start: '2026-09-08', period_end: '2026-09-21', impressions: 2600, clicks: 95, cost_yen: 20000, applications: 4 },
      { period_start: '2026-09-22', period_end: '2026-10-05', impressions: 3100, clicks: 128, cost_yen: 28000, applications: 6 },
    ] },
    versions: [
      {
        id: 'demo-003-v1', label: '初回掲載', observedAt: '2026-09-08T10:00:00+09:00',
        publishedFrom: '2026-09-08T10:00:00+09:00', publishedUntil: '2026-09-22T10:00:00+09:00',
        certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：来客の最初の窓口として、落ち着いた対応を届ける。\n\n仕事内容\n来訪されたお客様を受付でお迎えし、予約内容を確認して担当者へ取り次ぎます。専用画面への予約入力、案内資料の準備、受付まわりの整理を担当します。\n\n電話では予約日時や来訪目的を伺い、聞き取った内容を記録します。判断が必要な問い合わせは担当部署へ確認して回答します。予約変更があった場合は入力内容を照合します。\n\n仕事の進め方\n朝に予約予定を確認し、書類を準備します。電話と来客が重なったときは周囲へ声をかけ、対応漏れがないよう引き継ぎます。\n\n職場紹介\n受付と事務担当が近くで働くオフィスです。来訪予定や変更を共有し、担当者が不在のときも用件を記録して渡せるようにしています。\n\n入社後の流れ\n受付ルールと予約画面の使い方を確認します。事務経験を活かしながら、電話の取り次ぎと来客対応を先輩と練習します。\n\n応募後の流れ\n担当者が面談日時をご案内します。事務経験や電話対応の経験を伺い、担当業務と勤務条件を説明します。" + '\n\n募集条件\n' + '職種：受付事務\n仕事内容：来客受付、予約入力。\n給与：月給190,000円\n勤務時間：9:00〜18:00\n応募条件：事務経験1年以上。電話対応は必須です。',
        applications: { confirmed: 5, estimated: 0, unknown: 0 }, note: '掲載期間・応募数ともに架空の例です。',
      },
      {
        id: 'demo-003-v2', label: '応募条件変更', observedAt: '2026-09-22T10:15:00+09:00',
        publishedFrom: '2026-09-22T10:00:00+09:00', certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：受付の基本から学び、お客様を迎える仕事へ。\n\n仕事内容\n来訪されたお客様を受付でお迎えし、予約内容を確認して担当者へ取り次ぎます。専用画面への予約入力、案内資料の準備、受付まわりの整理を担当します。\n\n電話では予約日時や来訪目的を伺い、聞き取った内容を記録します。判断が必要な問い合わせは担当部署へ確認して回答します。予約変更があった場合は入力内容を照合します。\n\n仕事の進め方\n朝に予約予定を確認し、書類を準備します。電話と来客が重なったときは周囲へ声をかけ、対応漏れがないよう引き継ぎます。\n\n職場紹介\n受付と事務担当が近くで働くオフィスです。来訪予定や変更を共有し、担当者が不在のときも用件を記録して渡せるようにしています。\n\n入社後の流れ\n事務経験がない方にも予約入力の手順から説明します。電話の聞き取りと取り次ぎを練習し、先輩と一緒に受付を担当します。\n\n応募後の流れ\n担当者が面談日時をご案内します。希望の働き方を伺い、必須業務である電話対応を含めて仕事内容を説明します。" + '\n\n募集条件\n' + '職種：受付事務\n仕事内容：来客受付、予約入力。\n給与：月給200,000円\n勤務時間：9:00〜18:00\n応募条件：事務経験は必須ではありません。電話対応は必須です。',
        applications: { confirmed: 6, estimated: 0, unknown: 0 }, note: '給与額と経験必須条件の変更を示す架空例です。',
      },
    ],
  },
  {
    id: 'demo-job-004', title: '施設清掃スタッフ', company: 'デモ環境D',
    media: 'Airワーク', mediaJobId: 'DEMO-AIR-004', accountId: 'DEMO-ACCOUNT-01', location: '大分県中津市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 6,
    overallApplications: { total: 6, missingDate: 3, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-15': 1, '2026-09-29': 1, '2026-10-02': 1 } },
    versions: [
      {
        id: 'demo-004-v1', label: '初回取得', observedAt: '2026-09-10T08:00:00+09:00',
        certainty: 'unknown', kind: 'published', source: '求人XLSX（デモ・更新日時なし）',
        body: "キャッチコピー：朝の施設を整え、気持ちよい一日の始まりを支える。\n\n仕事内容\n施設の廊下、入口、共有スペースを掃き掃除と拭き掃除で整えます。ごみを回収し、備品の破損に気づいた際は管理担当へ報告します。\n\n利用者が通る場所では作業中の表示を置き、周囲を確認します。洗剤は指定された用途と量を守って使い、終了後は道具を洗って所定の場所に戻します。\n\n仕事の進め方\n朝の集合時に担当エリアを確認します。入口から共有スペースへ順に進め、終了した場所をチェック表に記録して引き継ぎます。\n\n職場紹介\n複数のスタッフがエリアを分担する施設清掃です。利用者の動線を共有し、困った汚れや設備の不具合は責任者へ相談します。\n\n入社後の流れ\n清掃用具の使い方と安全確認を学びます。先輩と一緒に巡回し、場所ごとの手順と確認項目を覚えます。\n\n応募後の流れ\n勤務可能日と通勤方法を確認します。面談では朝の勤務時間と車通勤不可の条件を説明し、勤務開始日を相談します。" + '\n\n募集条件\n' + '職種：施設清掃\n仕事内容：共有スペースの清掃。\n給与：時給1,050円\n勤務時間：6:00〜9:00、週5日\n応募条件：車通勤不可。経験不問。',
        applications: { confirmed: 0, estimated: 0, unknown: 4 }, note: '掲載時刻が不明な架空例。応募4件は版を特定できません。',
      },
      {
        id: 'demo-004-v2', label: '通勤条件変更', observedAt: '2026-09-28T08:00:00+09:00',
        certainty: 'unknown', kind: 'published', source: '求人XLSX（デモ・更新日時なし）',
        body: "キャッチコピー：朝の施設を整える仕事。車での通勤にも対応。\n\n仕事内容\n施設の廊下、入口、共有スペースを掃き掃除と拭き掃除で整えます。ごみを回収し、備品の破損に気づいた際は管理担当へ報告します。\n\n利用者が通る場所では作業中の表示を置き、周囲を確認します。洗剤は指定された用途と量を守って使い、終了後は道具を洗って所定の場所に戻します。\n\n仕事の進め方\n朝の集合時に担当エリアを確認します。入口から共有スペースへ順に進め、終了した場所をチェック表に記録して引き継ぎます。\n\n職場紹介\n複数のスタッフがエリアを分担する施設清掃です。利用者の動線を共有し、困った汚れや設備の不具合は責任者へ相談します。\n\n入社後の流れ\n清掃用具の使い方と安全確認を学びます。先輩と一緒に巡回し、場所ごとの手順と確認項目を覚えます。\n\n応募後の流れ\n勤務可能日と通勤方法を確認します。車通勤を希望する方には駐車場所を案内し、新しい勤務時間と開始日を相談します。" + '\n\n募集条件\n' + '職種：施設清掃\n仕事内容：共有スペースの清掃。\n給与：時給1,100円\n勤務時間：6:00〜10:00、週4日\n応募条件：車通勤可。経験不問。',
        applications: { confirmed: 0, estimated: 0, unknown: 2 }, note: '本文の変更は確認できますが、掲載切り替わり時刻は不明です。応募数は架空です。',
      },
    ],
  },
  {
    id: 'demo-job-005', title: '調理補助スタッフ', company: 'デモフードE',
    media: 'HRハッカー', mediaJobId: 'DEMO-HRH-005', accountId: 'DEMO-SHOP-01', location: '大分県日田市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 1,
    overallApplications: { total: 12, missingDate: 5, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-13': 2, '2026-09-18': 1, '2026-09-22': 1, '2026-09-27': 1, '2026-10-01': 2 } },
    versions: [
      {
        id: 'demo-005-v1', label: '初回掲載', observedAt: '2026-09-12T09:00:00+09:00',
        publishedFrom: '2026-09-12T09:00:00+09:00', publishedUntil: '2026-09-26T09:00:00+09:00',
        certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：昼食の時間を、盛り付けと片付けで支える仕事。\n\n仕事内容\n厨房で完成した料理を指定の食器へ盛り付け、数量と見た目を確認します。食器や調理器具を洗い、乾燥後は保管場所へ戻します。\n\n食材の扱いと手洗いの手順を守り、作業台を清潔に保ちます。アレルギー対応など判断が必要な内容は調理担当へ確認します。資格が必要な調理作業は担当者が行います。\n\n仕事の進め方\n当日の食数と見本を確認して準備します。提供時間に合わせて担当者と連携し、終了後は洗い場と作業台を片付けます。\n\n職場紹介\n調理担当と補助スタッフが役割を分ける厨房です。提供時間には声をかけ合い、食器の不足や作業の遅れを早めに共有します。\n\n入社後の流れ\n衛生管理と食器の置き場所を覚えます。見本に沿った盛り付けと洗浄の手順を、先輩と一緒に練習します。\n\n応募後の流れ\n勤務可能な曜日を確認します。面談では土日勤務が必須であることと昼の担当業務を説明し、勤務開始日を相談します。" + '\n\n募集条件\n' + '職種：調理補助\n仕事内容：盛り付けと洗い場。\n給与：時給1,080円\n勤務時間：10:00〜14:00、週3日\n応募条件：土日勤務は必須です。調理資格不要。',
        applications: { confirmed: 7, estimated: 0, unknown: 0 }, note: '応募数は架空の固定値です。',
      },
      {
        id: 'demo-005-v2', label: '勤務日条件変更', observedAt: '2026-09-26T09:10:00+09:00',
        publishedFrom: '2026-09-26T09:00:00+09:00', certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：平日の昼時間も相談できる、厨房のサポート業務。\n\n仕事内容\n厨房で完成した料理を指定の食器へ盛り付け、数量と見た目を確認します。食器や調理器具を洗い、乾燥後は保管場所へ戻します。\n\n食材の扱いと手洗いの手順を守り、作業台を清潔に保ちます。アレルギー対応など判断が必要な内容は調理担当へ確認します。資格が必要な調理作業は担当者が行います。\n\n仕事の進め方\n当日の食数と見本を確認して準備します。提供時間に合わせて担当者と連携し、終了後は洗い場と作業台を片付けます。\n\n職場紹介\n調理担当と補助スタッフが役割を分ける厨房です。提供時間には声をかけ合い、食器の不足や作業の遅れを早めに共有します。\n\n入社後の流れ\n衛生管理と食器の置き場所を覚えます。見本に沿った盛り付けと洗浄の手順を、先輩と一緒に練習します。\n\n応募後の流れ\n週2日から希望曜日を確認します。土日勤務は必須ではないため、平日のみの希望も含めて勤務日と開始日を相談します。" + '\n\n募集条件\n' + '職種：調理補助\n仕事内容：盛り付けと洗い場。\n給与：時給1,150円\n勤務時間：10:00〜15:00、週2日から\n応募条件：土日勤務は必須ではありません。調理資格不要。',
        applications: { confirmed: 4, estimated: 0, unknown: 1 }, note: '給与・勤務日・否定表現を比較する架空例です。',
      },
    ],
  },
  {
    id: 'demo-job-006', title: '製造ラインスタッフ', company: 'デモ製作F',
    media: 'Airワーク', mediaJobId: 'DEMO-AIR-006', accountId: 'DEMO-ACCOUNT-01', location: '大分県宇佐市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 2,
    overallApplications: { total: 9, missingDate: 6, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-16': 1, '2026-09-24': 1, '2026-10-01': 1 } },
    versions: [
      {
        id: 'demo-006-v1', label: '初回取得', observedAt: '2026-09-14T11:00:00+09:00',
        publishedFrom: '2026-09-14T11:00:00+09:00', publishedUntil: '2026-09-29T11:00:00+09:00',
        certainty: 'estimated', kind: 'published', source: '求人XLSX（デモ）',
        body: "キャッチコピー：手順を覚えて、一つひとつの部品を製品へ。\n\n仕事内容\n製造ラインで部品を組み合わせ、指定の位置へ取り付けます。手順書を見ながら工具を使い、完成品の外観と取り付け状態を確認します。\n\n傷や取り付けの不備を見つけたら良品と分けて報告します。数量と確認内容を記録して次の工程へ引き渡します。機械の異常は作業を止めて担当者に知らせます。\n\n仕事の進め方\n始業時に作業内容と安全事項を確認します。担当工程を進め、区切りごとに製品と数量を確認し、終了時に工具を片付けます。\n\n職場紹介\n組み立て担当と検査担当が連携する製造拠点です。品質の確認と安全な工具の使用を重視し、工程間で不具合の情報を共有します。\n\n入社後の流れ\n安全教育から始め、工具の持ち方と手順書の読み方を学びます。先輩の確認を受けながら組み立てと検査を練習します。\n\n応募後の流れ\n担当者が面談日程をご連絡します。日勤の勤務時間と担当工程を説明し、入社時期の希望を伺います。" + '\n\n募集条件\n' + '職種：製造ライン\n仕事内容：部品の組み立てと検査。\n給与：月給220,000円\n勤務時間：8:30〜17:30\n応募条件：夜勤はありません。製造経験不問。',
        applications: { confirmed: 3, estimated: 1, unknown: 1 }, note: '掲載期間の境界は取得日時からの推定。全件架空です。',
      },
      {
        id: 'demo-006-v2', label: '交替勤務へ変更', observedAt: '2026-09-29T11:00:00+09:00',
        publishedFrom: '2026-09-29T11:00:00+09:00', certainty: 'estimated', kind: 'published', source: '求人XLSX（デモ）',
        body: "キャッチコピー：交替制で製造を支え、前後の工程につなぐ仕事。\n\n仕事内容\n製造ラインで部品を組み合わせ、指定の位置へ取り付けます。手順書を見ながら工具を使い、完成品の外観と取り付け状態を確認します。\n\n傷や取り付けの不備を見つけたら良品と分けて報告します。数量と確認内容を記録して次の工程へ引き渡します。機械の異常は作業を止めて担当者に知らせます。\n\n仕事の進め方\n日勤と夜勤の交替時に進捗と注意事項を引き継ぎます。担当工程を進め、区切りごとに製品と数量を確認し、終了時に工具を片付けます。\n\n職場紹介\n組み立て担当と検査担当が連携する製造拠点です。品質の確認と安全な工具の使用を重視し、勤務帯をまたいで不具合の情報を共有します。\n\n入社後の流れ\n安全教育と工具の扱いから学びます。組み立てと検査を練習した後、交替時の引き継ぎと夜勤時の連絡方法を確認します。\n\n応募後の流れ\n担当者が面談日程をご連絡します。夜勤を含む交替制の勤務時間を説明し、働き方と入社時期の希望を伺います。" + '\n\n募集条件\n' + '職種：製造ライン\n仕事内容：部品の組み立てと検査。\n給与：月給240,000円\n勤務時間：8:30〜17:30／20:30〜5:30の交替制\n応募条件：夜勤があります。製造経験不問。',
        applications: { confirmed: 1, estimated: 2, unknown: 1 }, note: '勤務時間と夜勤の有無が変わった架空例。応募数の差が変更によるものかは分かりません。',
      },
    ],
  },
  {
    id: 'demo-job-007', title: '店舗販売スタッフ', company: 'デモリテールG',
    media: 'HRハッカー', mediaJobId: 'DEMO-HRH-007', accountId: 'DEMO-SHOP-01', location: '大分県佐伯市',
    
    // 応募日別の件数（架空）。HubSpot の応募レコードを模した値で、実在の応募ではありません。
    attributionUnknown: 0,
    overallApplications: { total: 5, missingDate: 0, fetchedAt: '2026-10-05T09:00:00+09:00', distributions: {}, byDate: { '2026-09-20': 1, '2026-09-28': 1, '2026-10-02': 2, '2026-10-04': 1 } },
    versions: [
      {
        id: 'demo-007-v1', label: '初回掲載', observedAt: '2026-09-16T10:00:00+09:00',
        publishedFrom: '2026-09-16T10:00:00+09:00', publishedUntil: '2026-10-01T10:00:00+09:00',
        certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：商品との出会いを、接客と売り場づくりで支える。\n\n仕事内容\nお客様からの問い合わせに対応し、商品の場所や特徴を案内します。売り場の商品を補充し、値札と陳列位置を確認して見やすい状態を保ちます。\n\n在庫の不足は担当者へ確認し、取り寄せの案内方法を相談します。入荷した商品の数量を照合して保管場所を整理し、閉店前には売り場と共有部分を片付けます。\n\n仕事の進め方\n出勤時に入荷と注意事項を確認します。接客の合間に補充を進め、引き継ぎ時は問い合わせや在庫の状況を共有します。\n\n職場紹介\n接客担当と商品管理担当が連携する地域の店舗です。商品知識を共有し、迷った案内や対応は責任者へ確認できる体制です。\n\n入社後の流れ\n販売経験を活かし、売り場の配置と商品の特徴を覚えます。店舗固有の接客方法と閉店作業を先輩と確認します。\n\n応募後の流れ\n担当者が面談日時をご案内します。販売経験と勤務可能日を伺い、午後から閉店までの勤務条件を説明します。" + '\n\n募集条件\n' + '職種：店舗販売\n仕事内容：接客と商品補充。\n給与：時給1,100円\n勤務時間：13:00〜21:00、週4日\n応募条件：販売経験必須。学生の応募不可。',
        applications: { confirmed: 2, estimated: 0, unknown: 0 }, note: '応募数は架空の固定値です。',
      },
      {
        id: 'demo-007-v2', label: '応募対象変更', observedAt: '2026-10-01T10:20:00+09:00',
        publishedFrom: '2026-10-01T10:00:00+09:00', certainty: 'confirmed', kind: 'published', source: '求人CSV（デモ）',
        body: "キャッチコピー：接客を学びながら、夕方の売り場を支える仕事。\n\n仕事内容\nお客様からの問い合わせに対応し、商品の場所や特徴を案内します。売り場の商品を補充し、値札と陳列位置を確認して見やすい状態を保ちます。\n\n在庫の不足は担当者へ確認し、取り寄せの案内方法を相談します。入荷した商品の数量を照合して保管場所を整理し、閉店前には売り場と共有部分を片付けます。\n\n仕事の進め方\n出勤時に入荷と注意事項を確認します。接客の合間に補充を進め、引き継ぎ時は問い合わせや在庫の状況を共有します。\n\n職場紹介\n接客担当と商品管理担当が連携する地域の店舗です。商品知識を共有し、迷った案内や対応は責任者へ確認できる体制です。\n\n入社後の流れ\n販売経験がない方にも挨拶と商品案内の基本から説明します。先輩と補充や閉店作業を練習し、商品を少しずつ覚えます。\n\n応募後の流れ\n担当者が面談日時をご案内します。学生の方も含めて勤務可能な曜日を伺い、夕方からの勤務と入社時期を相談します。" + '\n\n募集条件\n' + '職種：店舗販売\n仕事内容：接客と商品補充。\n給与：時給1,180円\n勤務時間：16:00〜21:00、週3日\n応募条件：販売経験不問。学生の応募可。',
        applications: { confirmed: 3, estimated: 0, unknown: 0 }, note: '応募条件と給与の変更を示す架空例です。',
      },
    ],
  },
  {
    id: 'demo-job-008', title: '設備点検スタッフ', company: 'デモ設備H',
    media: 'Airワーク', mediaJobId: 'DEMO-AIR-008', accountId: 'DEMO-ACCOUNT-01', location: '大分県臼杵市', versions: [],
  },
];

/**
 * Fictional market data for demo mode (no request to /api/job-copy/market). Like the real Indeed
 * data it is monthly by prefecture and ends at a fixed month (2026-08 here), so the timeline shows
 * the months after it as a no-data period. The screen reads the last month from the data.
 */
const demoMarketMonths = Array.from({ length: 14 }, (_, index) => {
  const month = 7 + index;
  return `${String(2025 + Math.floor((month - 1) / 12))}-${String((month - 1) % 12 + 1).padStart(2, '0')}`;
});
const demoMarketTitles = ['ドライバー', '倉庫作業', '受付事務', '清掃スタッフ', '調理補助', '製造スタッフ', '販売スタッフ'];
export function demoMarketData(title = '', prefecture = ''): MarketData {
  const base = { source: '架空の市場データ（デモ）。実在の求人数ではありません。', titles: demoMarketTitles, prefectures: ['大分県', '福岡県'], ctk_basis: 'Indeed閲覧者指標は、求職者の人数やこの求人への応募数ではありません。' };
  const titleIndex = demoMarketTitles.indexOf(title);
  if (titleIndex < 0 || !base.prefectures.includes(prefecture)) return { ...base, series: null };
  const scale = (prefecture === '福岡県' ? 3 : 1) * (80 + titleIndex * 25);
  return { ...base, series: {
    prefecture, months: demoMarketMonths,
    job_count: demoMarketMonths.map((_, index) => Math.round(scale * (1 + 0.03 * index - (index % 4 === 0 ? 0.04 : 0)))),
    ctk_count: demoMarketMonths.map((_, index) => Math.round(scale * 6 * (1 + 0.02 * ((index * 5) % 7)))),
    employer_count: demoMarketMonths.map((_, index) => Math.round(scale * 0.4 + index)),
    seekers_per_posting: demoMarketMonths.map((_, index) => Math.round(60 * (1 + 0.02 * ((index * 5) % 7)) / (1 + 0.03 * index)) / 10),
  } };
}
