// Query building for /api/recruitment_diag/*. Mirrors the old template (qs / buildCommonParams /
// buildGapParams): empty values are not sent, competitors adds limit=100, condition_gap adds the
// own-company conditions.

export const JOB_TYPES = [
  '老人福祉・介護',
  'サービス業',
  '小売業',
  '建設業',
  '医療',
  '教育・保育',
  '製造業',
  '飲食業',
  '運輸業',
  '派遣・人材',
  'IT・通信',
  '宿泊業',
  'その他',
] as const;

export const EMP_TYPES = ['正社員', 'パート', 'その他'] as const;
export const DEFAULT_EMP_TYPE = '正社員';

export interface DiagnosisForm {
  jobType: string;
  empType: string;
  /** Prefecture name (the select value). */
  prefecture: string;
  prefcode: number | null;
  /** Municipality name; '' = whole prefecture. */
  municipality: string;
  citycode: number | null;
  /** Raw input strings (type=number inputs). */
  ownSalaryMan: string;
  ownHolidays: string;
  ownBonus: string;
}

export const EMPTY_FORM: DiagnosisForm = {
  jobType: '',
  empType: DEFAULT_EMP_TYPE,
  prefecture: '',
  prefcode: null,
  municipality: '',
  citycode: null,
  ownSalaryMan: '',
  ownHolidays: '',
  ownBonus: '',
};

type Param = string | number | null | undefined;

/** URLSearchParams in insertion order; null / undefined / '' are skipped. */
export function qs(params: [string, Param][]): string {
  const u = new URLSearchParams();
  for (const [k, v] of params) {
    if (v !== null && v !== undefined && v !== '') u.append(k, String(v));
  }
  return u.toString();
}

function commonParams(f: DiagnosisForm): [string, Param][] {
  return [
    ['job_type', f.jobType],
    ['emp_type', f.empType === '' ? DEFAULT_EMP_TYPE : f.empType],
    ['prefecture', f.prefecture],
    ['municipality', f.municipality],
    ['prefcode', f.prefcode],
    ['citycode', f.citycode],
  ];
}

export function buildCommonQuery(f: DiagnosisForm): string {
  return qs(commonParams(f));
}

export function buildCompetitorsQuery(f: DiagnosisForm): string {
  return `${buildCommonQuery(f)}&limit=100`;
}

/** Monthly salary in man-yen is sent in yen; holidays only when > 0; bonus when >= 0. */
export function buildGapQuery(f: DiagnosisForm): string {
  const params = commonParams(f);
  const salaryMan = parseFloat(f.ownSalaryMan);
  if (!Number.isNaN(salaryMan) && salaryMan > 0) {
    params.push(['company_salary_min', Math.round(salaryMan * 10000)]);
  }
  const holidays = parseFloat(f.ownHolidays);
  if (!Number.isNaN(holidays) && holidays > 0) params.push(['company_annual_holidays', holidays]);
  const bonus = parseFloat(f.ownBonus);
  if (!Number.isNaN(bonus) && bonus >= 0) params.push(['company_bonus_months', bonus]);
  return qs(params);
}

/** null when the form can run; otherwise the on-screen message (replaces the old alert()). */
export function validateForm(f: DiagnosisForm): string | null {
  if (f.jobType === '') return '業種を選択してください';
  if (f.prefecture === '' || f.prefcode === null) return '都道府県を選択してください';
  return null;
}
