// 架空のサンプル。実在の会社・人・電話番号は使わない（公開リポジトリ）。
import type {
  FunnelStage,
  ActivityEntry,
  ApplicantRow,
  ColumnConfig,
  Customer,
  PortalAccount,
  SheetConfig,
  TrustedDevice,
} from './types';

export const NOW = '2026-10-20T14:32:00+09:00';

/** 困ったときの連絡先（架空）。ログイン前の画面・一覧が読めないとき・設定の画面で、同じものを出す。 */
export const SUPPORT = {
  name: '応募者の管理 サポート窓口',
  person: '山口 理沙',
  tel: '03-0000-0000',
  mail: 'support@sample-portal.example',
  hours: '平日 9:00 から 18:00',
};

/** 顧客がログインに使うアドレス（架空）。案内・招待メール・設定の画面で同じものを出す。 */
export const LOGIN_URL = 'https://portal.sample-portal.example/login';

/** ログインの失敗が続いたときのロックの長さ。 */
export const LOCK_MINUTES = 15;

export const customers: Customer[] = [
  { id: 'c1', name: 'サンプル介護サービス', stage: 'both', disabled: false, lastUsedAt: '2026-10-20T13:05:00+09:00' },
  { id: 'c2', name: 'みどり運輸（架空）', stage: 'sheet_only', disabled: false, lastUsedAt: null },
  { id: 'c3', name: 'ひかり歯科クリニック（架空）', stage: 'portal_only', disabled: false, lastUsedAt: '2026-10-19T18:40:00+09:00' },
  { id: 'c4', name: 'あおば警備（架空）', stage: 'both', disabled: true, lastUsedAt: '2026-09-02T10:12:00+09:00' },
];

const col = (header: string, opts: Partial<ColumnConfig> = {}): ColumnConfig => ({
  header,
  label: '',
  visible: true,
  writable: false,
  options: null,
  multiline: false,
  inList: true,
  ...opts,
});

export const SELECTION_OPTIONS = ['書類選考落ち', '未連絡', '応募者連絡待ち', '面接辞退', '面接日時確定', '面接済', '内定', '不採用', '入社日調整中', '内定辞退'];
export const MEDIA_OPTIONS = ['indeed', 'engage', '求人ボックス', 'ジモティー', 'Airワーク', 'バイトル'];
export const JOB_OPTIONS = ['ドライバー'];
export const EMPLOYMENT_OPTIONS = ['正社員', 'アルバイト'];

/**
 * 状況の選択肢 → 段階（数字の集計用）。
 * 採用（hired）に当たる選択肢が無いので、「入社予定日」に日付が入っている人を採用として数える（hiredDateColumn）。
 * 入社日調整中は、日付が入るまでは内定として数える。
 */
export const DEFAULT_STAGE_MAP: Record<string, FunnelStage> = {
  未連絡: 'applied',
  応募者連絡待ち: 'screening',
  面接日時確定: 'interview_set',
  面接済: 'interviewed',
  内定: 'offer',
  入社日調整中: 'offer',
  書類選考落ち: 'rejected',
  不採用: 'rejected',
  面接辞退: 'declined',
  内定辞退: 'declined',
};

export const DENTAL_STATUS_OPTIONS = ['新規', '連絡済み', '見学予定', '面接予定', '面接済み', '内定', '入職', '見送り', '辞退'];
export const DENTAL_STAGE_MAP: Record<string, FunnelStage> = {
  新規: 'applied',
  連絡済み: 'screening',
  見学予定: 'interview_set',
  面接予定: 'interview_set',
  面接済み: 'interviewed',
  内定: 'offer',
  入職: 'hired',
  見送り: 'rejected',
  辞退: 'declined',
};

/** 社内の人の表示名（試作）。 */
export const STAFF_NAME = '弊社 山口';

/**
 * 実際の応募者管理シート「応募者情報管理」の列。見出しの色は A〜K が青（弊社が書く）、L〜S が緑（貴社が書く）。
 * 青の列は画面から書けない、緑の列は書ける。A:B と M:O はシートでは結合セルだが、1 つの項目として読む。
 */
function realColumns(): ColumnConfig[] {
  return [
    col('応募発生日', { kind: 'date', group: 'summary' }),
    col('応募発生媒体', { options: MEDIA_OPTIONS, kind: 'choice', group: 'summary' }),
    col('応募者名', { writable: true, group: 'summary' }),
    col('フリガナ', { writable: true, inList: false, group: 'other' }),
    col('年齢', { group: 'summary' }),
    col('電話番号', { kind: 'phone', group: 'summary' }),
    col('メールアドレス', { inList: false, kind: 'email', group: 'summary' }),
    col('職種', { options: JOB_OPTIONS, kind: 'choice', inList: false, group: 'summary' }),
    col('雇用形態', { options: EMPLOYMENT_OPTIONS, kind: 'choice', group: 'summary' }),
    col('求職者経歴', { multiline: true, inList: false, group: 'staff' }),
    col('状況', { writable: true, options: SELECTION_OPTIONS, kind: 'choice', group: 'customer' }),
    col('面接予定日', { writable: true, kind: 'date', group: 'customer' }),
    col('時間', { writable: true, group: 'customer' }),
    col('備考', { writable: true, multiline: true, inList: false, group: 'customer' }),
    col('次回アクション', { writable: true, group: 'customer' }),
    col('入社予定日', { writable: true, kind: 'date', group: 'customer' }),
  ];
}

const realSheet = (id: string, tabName: string, extra: Partial<SheetConfig>): SheetConfig => ({
  id,
  customerId: 'c1',
  tabName,
  headerRow: 1,
  exampleRows: 1,
  connection: 'ok',
  titleColumn: '応募者名',
  statusColumn: '状況',
  sortColumn: '応募発生日',
  interviewColumn: '面接予定日',
  interviewTimeColumn: '時間',
  hiredDateColumn: '入社予定日',
  stageMap: DEFAULT_STAGE_MAP,
  columns: realColumns(),
  ...extra,
});

export const sheets: SheetConfig[] = [
  realSheet('s1', '応募者情報管理', { customerCanEditColumns: true }),
  realSheet('s2', '応募者情報管理（2025年度）', {}),
  {
    id: 's3',
    customerId: 'c2',
    tabName: '応募者',
    headerRow: 2,
    connection: 'not_shared',
    columns: [],
  },
  {
    id: 's4',
    customerId: 'c3',
    tabName: '応募管理',
    headerRow: 1,
    connection: 'ok',
    titleColumn: '氏名',
    statusColumn: '状況',
    sortColumn: '応募日',
    interviewColumn: '面接日',
    stageMap: DENTAL_STAGE_MAP,
    columns: [
      col('応募日', { kind: 'date', group: 'summary' }),
      col('氏名', { group: 'summary' }),
      col('希望職種', { group: 'summary' }),
      col('応募媒体', { group: 'summary' }),
      col('電話番号', { kind: 'phone', group: 'summary' }),
      col('状況', { writable: true, options: DENTAL_STATUS_OPTIONS, kind: 'choice', group: 'customer' }),
      col('見学日', { writable: true, kind: 'datetime', group: 'customer' }),
      col('面接日', { writable: true, kind: 'datetime', group: 'customer' }),
      col('院長メモ', { writable: true, multiline: true, inList: false, group: 'customer' }),
      col('弊社対応メモ', { multiline: true, inList: false, group: 'staff' }),
    ],
  },
];

const FAMILY = ['山田', '佐藤', '鈴木', '高橋', '田中', '伊藤', '渡辺', '中村', '小林', '加藤', '吉田', '山本'];
const GIVEN = ['花子', '太郎', '美咲', '健太', '陽子', '翔', '由美', '大輔', '真理', '浩二', 'さくら', '拓也'];
const KANA_F = ['ヤマダ', 'サトウ', 'スズキ', 'タカハシ', 'タナカ', 'イトウ', 'ワタナベ', 'ナカムラ', 'コバヤシ', 'カトウ', 'ヨシダ', 'ヤマモト'];
const KANA_G = ['ハナコ', 'タロウ', 'ミサキ', 'ケンタ', 'ヨウコ', 'ショウ', 'ユミ', 'ダイスケ', 'マリ', 'コウジ', 'サクラ', 'タクヤ'];
const CONTACT = [
  '10/14 電話で連絡。面接を希望。平日午後なら可。',
  '電話に出ず、SMS を送信。折り返し待ち。',
  '本人と話せた。夜勤は不可、日勤のみ希望。',
  '他社で内定が出ており、辞退の意向。',
  '資格（介護福祉士）あり。早めの面接を希望。',
];

const pad = (n: number) => String(n).padStart(2, '0');
const at = (list: string[], i: number): string => list[i % list.length] ?? '';

/** i 番目（新しい順）の応募の年月日。6 か月ほどに散らす（今月が一番多い）。 */
function spread(i: number): { back: number; month: number; day: number } {
  const back = i < 12 ? 0 : i < 20 ? 1 : i < 27 ? 2 : i < 34 ? 3 : i < 40 ? 4 : 5;
  const inMonth = i < 12 ? i : i < 20 ? i - 12 : i < 27 ? i - 20 : i < 34 ? i - 27 : i < 40 ? i - 34 : i - 40;
  return { back, month: 10 - back, day: back === 0 ? Math.max(1, 20 - Math.floor(inMonth * 1.6)) : Math.max(1, 28 - inMonth * 4) };
}

/** 日付の書き方は人によってそろっていない。 */
function dateText(y: number, m: number, d: number, variant: number): string {
  switch (variant % 4) {
    case 0: return `${String(y)}年${String(m)}月${String(d)}日`;
    case 1: return `${String(y)}/${String(m)}/${String(d)}`;
    case 2: return `${String(y)}/${pad(m)}/${pad(d)}`;
    default: return `${String(m)}月${String(d)}日`;
  }
}

const OLD_STATUS = ['入社日調整中', '不採用', '面接辞退', '書類選考落ち', '面接済', '内定辞退', '入社日調整中', '不採用', '内定', '書類選考落ち', '面接済', '入社日調整中'];
const RECENT_STATUS = ['未連絡', '未連絡', '応募者連絡待ち', '面接日時確定', '面接日時確定', '応募者連絡待ち', '面接日時確定', '面接辞退', '未連絡', '応募者連絡待ち'];
const EDU = ['高校卒業', '専門学校卒業', '大学卒業', '高校卒業'];
const ADDR = ['北山県 青空市', '北山県 みどり町', '南川県 はるか市', '東原県 あさひ市', '北山県 ひので市'];
const WORK = ['会社員（運送）', 'アルバイト', '無職', '会社員（営業）', '自営業'];
const LICENSE = ['普通自動車免許', '普通自動車免許、フォークリフト', '準中型免許', '普通自動車免許、中型免許'];
const AVAIL = ['平日 夕方以降', '土日のみ', '平日午前', 'いつでも可'];
const MEMO = [
  '10/14 電話に出ず、SMS を送信。\n10/15 折り返しあり。面接日を調整中。',
  '本人と話せた。日勤のみ希望。遠方のため、通勤方法を面接で確認する。',
  '他社と並行して応募している。結果が出たら連絡をもらう約束。',
  '前職の退職日が未定。決まり次第、入社日を調整する。\n履歴書は面接当日に持参。',
  '',
  '面接の連絡後、返信なし。1 週間後にもう一度連絡する。',
];
const NEXT = ['面接の前日に確認の連絡', '結果を電話で伝える', '入社書類を送る', '折り返しを待つ', '', '履歴書の返送を確認', ''];

const careerOf = (i: number): string => `最終学歴：${at(EDU, i)}\n現住所：${at(ADDR, i * 2)}\n現在の職業：${at(WORK, i)}\n保有資格：${at(LICENSE, i)}\n面接可能日程：${at(AVAIL, i * 3)}`;

/** 実際のシートの形の行。2 行目は記入例（応募者ではない）、3 行目から応募者。 */
function makeRealRows(count: number, year: number, recent: boolean): ApplicantRow[] {
  const rows: ApplicantRow[] = [
    {
      rowNumber: 2,
      changedSinceLastView: null,
      values: {
        応募発生日: '記入例）2023年1月20日',
        応募発生媒体: 'indeed',
        応募者名: '記入例 太郎',
        フリガナ: 'キニュウレイ タロウ',
        年齢: '35',
        電話番号: '09000000000',
        メールアドレス: 'example@example.com',
        職種: 'ドライバー',
        雇用形態: '正社員',
        求職者経歴: '最終学歴：〇〇高校卒業\n現住所：〇〇県〇〇市\n現在の職業：〇〇\n保有資格：普通自動車免許\n面接可能日程：平日夕方以降',
        状況: '未連絡',
        面接予定日: '2023/01/30',
        時間: '11:00',
        備考: '（記入例）経緯のメモを書く欄',
        次回アクション: '（記入例）面接の前日に連絡',
        入社予定日: '',
      },
    },
  ];
  for (let i = 0; i < count; i += 1) {
    const { back, month, day } = spread(i);
    const status = recent && back === 0 && i < 10 ? at(RECENT_STATUS, i) : at(OLD_STATUS, i + back);
    const future = status === '面接日時確定';
    const interviewed = ['面接済', '内定', '入社日調整中', '不採用', '内定辞退'].includes(status);
    const iDay = future ? 22 + (i % 8) : Math.min(back === 0 ? 19 : 28, day + 1 + (i % 4));
    const iMonth = back === 0 ? 10 : month;
    const timeText = i % 6 === 5 ? '' : `${String(10 + (i % 7))}:${i % 4 === 1 ? '30' : '00'}`;
    let interview = '';
    if (future || interviewed) interview = dateText(year, iMonth, Math.max(iDay, day), i + 1);
    else if (status === '面接辞退') interview = i % 2 === 0 ? dateText(year, iMonth, Math.max(iDay, day), i) : '記載なし';
    else if (status === '書類選考落ち') interview = i % 2 === 0 ? '記載なし' : '';
    const hasInterview = interview !== '' && interview !== '記載なし';
    let hire = '';
    if (status === '入社日調整中') hire = i % 4 === 3 ? '' : dateText(year, month + 1, 1, i);
    else if (status === '内定') hire = '記載なし';
    rows.push({
      rowNumber: i + 3,
      changedSinceLastView: recent && (i === 0 || i === 1) ? 'new' : null,
      values: {
        応募発生日: dateText(year, month, day, i + 2),
        応募発生媒体: at(MEDIA_OPTIONS, i + Math.floor(i / 5)),
        応募者名: i % 5 === 4 ? `${at(FAMILY, i)}${at(GIVEN, i * 5)}` : `${at(FAMILY, i)} ${at(GIVEN, i * 5)}`,
        フリガナ: `${at(KANA_F, i)} ${at(KANA_G, i * 5)}`,
        年齢: String(22 + ((i * 7) % 40)),
        電話番号: '0900000' + String(1000 + i).slice(-4),
        メールアドレス: i % 6 === 0 ? '記載なし' : i % 6 === 3 ? '' : 'sample' + String(i + 1) + '@example.com',
        職種: 'ドライバー',
        雇用形態: i % 3 === 2 ? 'アルバイト' : '正社員',
        求職者経歴: careerOf(i),
        状況: status,
        面接予定日: interview,
        時間: hasInterview ? timeText : '',
        備考: at(MEMO, i + back),
        次回アクション: status === '未連絡' || status === '応募者連絡待ち' || status === '面接日時確定' ? at(NEXT, i) : status === '入社日調整中' ? '入社書類を送る' : '',
        入社予定日: hire,
      },
    });
  }
  return rows;
}

/** 記録のサンプル（activity）と行の値を合わせる。 */
function patch(rows: ApplicantRow[], rowNumber: number, values: Record<string, string>): ApplicantRow[] {
  return rows.map((r) => (r.rowNumber === rowNumber ? { ...r, values: { ...r.values, ...values } } : r));
}

const CAREER_6 = careerOf(3);
const E7_AFTER = CAREER_6 + '\n10/20 追記：10月25日以降も面接可能。';

function sampleRows(): ApplicantRow[] {
  let rows = makeRealRows(40, 2026, true);
  rows = patch(rows, 3, { 状況: '未連絡', 備考: '同日に研修の見学も案内予定。' });
  rows = patch(rows, 4, { 状況: '面接日時確定', 面接予定日: '2026/10/23', 時間: '11:00' });
  rows = patch(rows, 5, { 状況: '不採用', 面接予定日: '2026/10/18', 時間: '14:00', 次回アクション: '不採用の連絡を入れる' });
  rows = patch(rows, 6, { 求職者経歴: E7_AFTER });
  // 今日と明日の面接（「今日」の帯の確認用）。書き方はそろえていない。
  rows = patch(rows, 7, { 状況: '面接日時確定', 面接予定日: '2026/10/20', 時間: '15:30', 次回アクション: '面接の前日に確認の連絡' });
  rows = patch(rows, 8, { 状況: '面接日時確定', 面接予定日: '10月20日', 時間: '17:00' });
  rows = patch(rows, 9, { 状況: '面接日時確定', 面接予定日: '2026/10/21', 時間: '10:00' });
  rows = patch(rows, 10, { 状況: '面接日時確定', 面接予定日: '10月21日', 時間: '14:30' });
  // 変わった行の色を見せる行。
  rows = rows.map((r) => (r.rowNumber === 7 ? { ...r, changedSinceLastView: 'updated' as const, changedColumns: ['状況'] } : r));
  return rows;
}

/** 歯科のシート（列の違う例）の行。 */
function makeDentalRows(count: number): ApplicantRow[] {
  const rows: ApplicantRow[] = [];
  for (let i = 0; i < count; i += 1) {
    const { month, day } = spread(i);
    rows.push({
      rowNumber: i + 2,
      changedSinceLastView: null,
      values: {
        応募日: `2026/${pad(month)}/${pad(day)}`,
        氏名: `${at(FAMILY, i)} ${at(GIVEN, i * 5)}`,
        希望職種: at(['歯科衛生士', '歯科助手', '受付'], i),
        応募媒体: at(['Indeed', 'AirWork', 'ハローワーク', '求人ボックス'], i + Math.floor(i / 5)),
        電話番号: '090-0000-' + String(1000 + i).slice(-4),
        状況: at(DENTAL_STATUS_OPTIONS, i * 2),
        見学日: i % 3 === 0 ? '2026/10/2' + String(i % 9) + ' 13:00' : '',
        面接日: i % 4 === 0 ? '2026/10/2' + String((i + 2) % 9) + ' 17:30' : '',
        院長メモ: i % 5 === 0 ? '経験 3 年以上なら優先。' : '',
        弊社対応メモ: at(CONTACT, i),
      },
    });
  }
  return rows;
}

/** シートの全行（見出しの下の記入例の行を含む）。画面に出す前に applicantRows で応募者の行だけにする。 */
export const initialRows: Record<string, ApplicantRow[]> = {
  s1: sampleRows(),
  s2: makeRealRows(18, 2025, false).map((r) => ({ ...r, changedSinceLastView: null })),
  s3: [],
  s4: makeDentalRows(24),
};

export const accounts: PortalAccount[] = [
  { id: 'a1', customerId: 'c1', email: 'saiyou@sample-kaigo.example', displayName: '採用担当 木村', status: 'active', extraCodeEmail: 'kimura.mobile@example.com', lastLoginAt: '2026-10-20T13:05:00+09:00', invitedAt: '2026-10-01T10:00:00+09:00', inviteExpiresAt: null },
  { id: 'a2', customerId: 'c1', email: 'shisetsu-cho@sample-kaigo.example', displayName: '施設長 森', status: 'active', extraCodeEmail: null, lastLoginAt: '2026-10-17T09:21:00+09:00', invitedAt: '2026-10-01T10:00:00+09:00', inviteExpiresAt: null },
  { id: 'a3', customerId: 'c1', email: 'jimu@sample-kaigo.example', displayName: '', status: 'invited', extraCodeEmail: null, lastLoginAt: null, invitedAt: '2026-10-18T15:00:00+09:00', inviteExpiresAt: '2026-10-25T15:00:00+09:00' },
  { id: 'a4', customerId: 'c1', email: 'old-staff@example.com', displayName: '前任 石井', status: 'disabled', extraCodeEmail: null, lastLoginAt: '2026-07-30T11:00:00+09:00', invitedAt: '2026-06-01T10:00:00+09:00', inviteExpiresAt: null },
  { id: 'a5', customerId: 'c3', email: 'office@hikari-dental.example', displayName: '受付 原', status: 'locked', extraCodeEmail: null, lastLoginAt: '2026-10-19T18:40:00+09:00', invitedAt: '2026-10-05T10:00:00+09:00', inviteExpiresAt: null, lockedUntil: '2026-10-20T14:46:00+09:00' },
];

export const devices: TrustedDevice[] = [
  { id: 'd1', accountId: 'a1', label: 'Windows の Chrome', lastUsedAt: '2026-10-20T13:05:00+09:00', expiresAt: '2027-01-18T13:05:00+09:00', current: true },
  { id: 'd2', accountId: 'a1', label: 'iPhone の Safari', lastUsedAt: '2026-10-16T19:48:00+09:00', expiresAt: '2027-01-14T19:48:00+09:00', current: false },
];

export const activity: ActivityEntry[] = [
  { id: 'e1', at: '2026-10-20T13:07:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'save', detail: '山田 花子さんの「備考」を変更', actorName: '採用担当 木村', source: 'portal', sheetId: 's1', rowNumber: 3, column: '備考', before: '', after: '同日に研修の見学も案内予定。' },
  { id: 'e2', at: '2026-10-20T13:05:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'login', detail: 'Windows の Chrome からログイン' },
  { id: 'e3', at: '2026-10-19T18:41:00+09:00', accountEmail: 'office@hikari-dental.example', customerId: 'c3', kind: 'login_failed', detail: 'パスワードの誤りが 5 回続いたため、15 分間ロック（時間がたつと自動で解除）' },
  { id: 'e4', at: '2026-10-18T15:00:00+09:00', accountEmail: 'jimu@sample-kaigo.example', customerId: 'c1', kind: 'invite', detail: '招待リンクを発行（10/25 まで有効）' },
  { id: 'e5b', at: '2026-10-17T09:15:00+09:00', accountEmail: '', customerId: 'c1', kind: 'sheet_change', detail: '佐藤 翔さんの「面接予定日」がシートで変更', actorName: '弊社 山口', source: 'sheet', sheetId: 's1', rowNumber: 4, column: '面接予定日', before: '', after: '2026/10/23' },
  { id: 'e5', at: '2026-10-17T09:22:00+09:00', accountEmail: 'shisetsu-cho@sample-kaigo.example', customerId: 'c1', kind: 'save_conflict', detail: '佐藤 翔さんの「面接予定日」は、シートで先に変更されていたため保存しなかった', actorName: '施設長 森', source: 'portal', sheetId: 's1', rowNumber: 4, column: '面接予定日', before: '', after: '2026/10/24' },
  { id: 'e7', at: '2026-10-20T11:40:00+09:00', accountEmail: '', customerId: 'c1', kind: 'sheet_change', detail: '高橋 健太さんの「求職者経歴」を追記', actorName: '弊社 山口', source: 'sheet', sheetId: 's1', rowNumber: 6, column: '求職者経歴', before: CAREER_6, after: E7_AFTER },
  { id: 'e8', at: '2026-10-19T16:12:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'save', detail: '鈴木 さくらさんの「次回アクション」を「不採用の連絡を入れる」に変更', actorName: '採用担当 木村', source: 'portal', sheetId: 's1', rowNumber: 5, column: '次回アクション', before: '', after: '不採用の連絡を入れる' },
  { id: 'e9', at: '2026-10-19T16:10:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'save', detail: '鈴木 さくらさんの「状況」を「不採用」に変更', actorName: '採用担当 木村', source: 'portal', sheetId: 's1', rowNumber: 5, column: '状況', before: '面接済', after: '不採用' },
  { id: 'e10', at: '2026-10-18T10:02:00+09:00', accountEmail: '', customerId: 'c1', kind: 'column_change', detail: '「状況」の選択肢に「入社日調整中」を追加', actorName: '弊社 山口', source: 'admin', sheetId: 's1', column: '状況', before: '書類選考落ち、未連絡、応募者連絡待ち、面接辞退、面接日時確定、面接済、内定、不採用、内定辞退', after: SELECTION_OPTIONS.join('、') },
  { id: 'e6', at: '2026-10-17T09:21:00+09:00', accountEmail: 'shisetsu-cho@sample-kaigo.example', customerId: 'c1', kind: 'code_sent', detail: '新しい端末（iPad の Safari）のため確認コードを送信' },
];

/** 試作でログインしている顧客の担当者。 */
export const CURRENT_ACCOUNT_ID = 'a1';
