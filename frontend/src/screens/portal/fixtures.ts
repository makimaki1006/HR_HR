// 架空のサンプル。実在の会社・人・電話番号は使わない（公開リポジトリ）。
import type {
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

export const SELECTION_OPTIONS = ['未対応', '書類選考中', '面接調整中', '面接予定', '内定', '採用', '不採用', '辞退'];
export const INTERVIEW_RESULT_OPTIONS = ['', '合格', '不合格', '保留', '欠席'];

export const sheets: SheetConfig[] = [
  {
    id: 's1',
    customerId: 'c1',
    tabName: '応募者一覧',
    headerRow: 1,
    connection: 'ok',
    titleColumn: '氏名',
    statusColumn: '選考状況',
    sortColumn: '応募日',
    columns: [
      col('応募日', { kind: 'date', group: 'summary' }),
      col('氏名', { group: 'summary' }),
      col('応募職種', { group: 'summary' }),
      col('応募媒体', { group: 'summary' }),
      col('年齢', { group: 'summary' }),
      col('電話番号', { kind: 'phone', group: 'summary' }),
      col('ふりがな', { inList: false, group: 'other' }),
      col('メールアドレス', { inList: false, kind: 'email', group: 'summary' }),
      col('一次対応日', { inList: false, kind: 'date', group: 'staff' }),
      col('一次対応の結果', { label: '弊社からの連絡結果', multiline: true, inList: false, group: 'staff' }),
      col('選考状況', { writable: true, options: SELECTION_OPTIONS, kind: 'choice', group: 'customer' }),
      col('面接日', { writable: true, kind: 'datetime', group: 'customer' }),
      col('面接結果', { writable: true, options: INTERVIEW_RESULT_OPTIONS, kind: 'choice', group: 'customer' }),
      col('貴社記入欄', { label: '貴社メモ', writable: true, multiline: true, inList: false, group: 'customer' }),
      col('社内メモ', { visible: false, inList: false, group: 'other' }),
    ],
  },
  {
    id: 's2',
    customerId: 'c1',
    tabName: '2025年度',
    headerRow: 1,
    connection: 'ok',
    titleColumn: '氏名',
    statusColumn: '選考状況',
    sortColumn: '応募日',
    columns: [
      col('応募日', { kind: 'date', group: 'summary' }),
      col('氏名', { group: 'summary' }),
      col('応募職種', { group: 'summary' }),
      col('選考状況', { writable: true, options: SELECTION_OPTIONS, kind: 'choice', group: 'customer' }),
      col('貴社記入欄', { label: '貴社メモ', writable: true, multiline: true, inList: false, group: 'customer' }),
    ],
  },
  {
    id: 's3',
    customerId: 'c2',
    tabName: '応募者',
    headerRow: 2,
    connection: 'not_shared',
    columns: [],
  },
];

const FAMILY = ['山田', '佐藤', '鈴木', '高橋', '田中', '伊藤', '渡辺', '中村', '小林', '加藤', '吉田', '山本'];
const GIVEN = ['花子', '太郎', '美咲', '健太', '陽子', '翔', '由美', '大輔', '真理', '浩二', 'さくら', '拓也'];
const KANA_F = ['やまだ', 'さとう', 'すずき', 'たかはし', 'たなか', 'いとう', 'わたなべ', 'なかむら', 'こばやし', 'かとう', 'よしだ', 'やまもと'];
const KANA_G = ['はなこ', 'たろう', 'みさき', 'けんた', 'ようこ', 'しょう', 'ゆみ', 'だいすけ', 'まり', 'こうじ', 'さくら', 'たくや'];
const MEDIA = ['Indeed', 'AirWork', 'ハローワーク', '求人ボックス'];
const JOBS = ['介護職（正社員）', '介護職（パート）', '看護師', '送迎ドライバー', '調理補助'];
const CONTACT = [
  '10/14 電話で連絡。面接を希望。平日午後なら可。',
  '電話に出ず、SMS を送信。折り返し待ち。',
  '本人と話せた。夜勤は不可、日勤のみ希望。',
  '他社で内定が出ており、辞退の意向。',
  '資格（介護福祉士）あり。早めの面接を希望。',
];

const pad = (n: number) => String(n).padStart(2, '0');
const at = (list: string[], i: number): string => list[i % list.length] ?? '';

function makeRows(count: number): ApplicantRow[] {
  const rows: ApplicantRow[] = [];
  for (let i = 0; i < count; i += 1) {
    const day = 20 - Math.floor(i / 2);
    const month = day > 0 ? 10 : 9;
    const d = day > 0 ? day : 30 + day;
    const status = at(SELECTION_OPTIONS, i * 3);
    const hasInterview = status === '面接予定' || status === '内定' || status === '採用' || status === '不採用';
    rows.push({
      rowNumber: i + 2,
      changedSinceLastView: i === 0 || i === 1 ? 'new' : i === 4 ? 'updated' : null,
      values: {
        応募日: `2026/${pad(month)}/${pad(d)}`,
        応募媒体: at(MEDIA, i),
        氏名: `${at(FAMILY, i)} ${at(GIVEN, i * 5)}`,
        ふりがな: `${at(KANA_F, i)} ${at(KANA_G, i * 5)}`,
        年齢: String(22 + ((i * 7) % 40)),
        電話番号: '090-0000-' + String(1000 + i).slice(-4),
        メールアドレス: i % 3 === 0 ? '' : 'sample' + String(i + 1) + '@example.com',
        応募職種: at(JOBS, i),
        一次対応日: `2026/${pad(month)}/${pad(Math.max(1, d))}`,
        一次対応の結果: at(CONTACT, i),
        選考状況: status,
        面接日: hasInterview ? `2026/10/${pad(22 + (i % 8))} ${String(10 + (i % 7))}:00` : '',
        面接結果: status === '内定' || status === '採用' ? '合格' : status === '不採用' ? '不合格' : '',
        貴社記入欄: i % 4 === 0 ? '施設見学も同日に案内予定。' : '',
        社内メモ: '（顧客には見せない列）',
      },
    });
  }
  return rows;
}

export const initialRows: Record<string, ApplicantRow[]> = {
  s1: makeRows(46),
  s2: makeRows(18).map((r) => ({ ...r, changedSinceLastView: null, values: { ...r.values, 応募日: (r.values.応募日 ?? '').replace('2026', '2025') } })),
  s3: [],
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
  { id: 'e1', at: '2026-10-20T13:07:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'save', detail: '山田 花子さんの「選考状況」を「面接予定」に変更' },
  { id: 'e2', at: '2026-10-20T13:05:00+09:00', accountEmail: 'saiyou@sample-kaigo.example', customerId: 'c1', kind: 'login', detail: 'Windows の Chrome からログイン' },
  { id: 'e3', at: '2026-10-19T18:41:00+09:00', accountEmail: 'office@hikari-dental.example', customerId: 'c3', kind: 'login_failed', detail: 'パスワードの誤りが 5 回続いたため、15 分間ロック（時間がたつと自動で解除）' },
  { id: 'e4', at: '2026-10-18T15:00:00+09:00', accountEmail: 'jimu@sample-kaigo.example', customerId: 'c1', kind: 'invite', detail: '招待リンクを発行（10/25 まで有効）' },
  { id: 'e5', at: '2026-10-17T09:22:00+09:00', accountEmail: 'shisetsu-cho@sample-kaigo.example', customerId: 'c1', kind: 'save_conflict', detail: '佐藤 翔さんの行は、シートで先に変更されていたため保存しなかった' },
  { id: 'e6', at: '2026-10-17T09:21:00+09:00', accountEmail: 'shisetsu-cho@sample-kaigo.example', customerId: 'c1', kind: 'code_sent', detail: '新しい端末（iPad の Safari）のため確認コードを送信' },
];

/** 試作でログインしている顧客の担当者。 */
export const CURRENT_ACCOUNT_ID = 'a1';
