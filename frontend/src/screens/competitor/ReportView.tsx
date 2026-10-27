import type { CompetitorReport } from '../../generated/CompetitorReport';
import { Tabs } from './Tabs';
import { ExcelTab } from './tabs/ExcelTab';
import { ConsultationTab } from './tabs/ConsultationTab';
import { GoogleTab } from './tabs/GoogleTab';
import { IndeedTab } from './tabs/IndeedTab';
import { PopulationTab } from './tabs/PopulationTab';
import { PopularityTab } from './tabs/PopularityTab';
import { CompanyCountTab } from './tabs/CompanyCountTab';

/** 「採用のヒント」タブを画面に出すか。コードと API の出力は残してあり、true に戻せば再び出る (PDF・旧画面は別)。 */
export const SHOW_CONSULTATION_TAB = false;

const ALL_TAB_IDS = ['excel', 'google', 'indeed', 'population', 'popularity', 'companies', 'consultation'] as const;
export type TabId = (typeof ALL_TAB_IDS)[number];
// フラグは定数で、false の間は条件が常に偽になる (戻せるよう残す)。
/* eslint-disable @typescript-eslint/no-unnecessary-condition */
export const TAB_IDS: readonly TabId[] = SHOW_CONSULTATION_TAB
  ? ALL_TAB_IDS
  : ALL_TAB_IDS.filter((id) => id !== 'consultation');

export function isTabId(value: string | null): value is TabId {
  return value !== null && (TAB_IDS as readonly string[]).includes(value);
}

interface Props {
  report: CompetitorReport;
  tab: TabId;
  onTabChange: (tab: TabId) => void;
}

/** レポート本体 (PDF は別経路でサーバが作る。新タブは PDF に載らない)。タブの ID・順序は旧画面 (PDF の収まり検査) と同じ。 */
export function ReportView({ report, tab, onTabChange }: Props) {
  const split = report.popularity_split ?? null;
  // 人気の分かれ方が無いレポートで ?tab=popularity 等が来たら先頭のタブに戻す。
  const selected: TabId = split === null && (tab === 'popularity' || tab === 'companies') ? 'excel' : tab;
  return (
    <Tabs
      label="競合調査の表示切り替え"
      selected={selected}
      onSelect={(id) => {
        if (isTabId(id)) onTabChange(id);
      }}
      items={[
        { id: 'excel', label: '給与・待遇', content: <ExcelTab report={report} /> },
        { id: 'google', label: 'Google検索需要', content: <GoogleTab data={report.google} /> },
        { id: 'indeed', label: 'Indeed採用レポート', content: <IndeedTab data={report.indeed} /> },
        {
          id: 'population',
          label: '人口・地域データ',
          content: <PopulationTab data={report.population} />,
        },
        ...(split === null
          ? []
          : [
              { id: 'popularity', label: '人気の分かれ方', content: <PopularityTab data={split} /> },
              { id: 'companies', label: '会社別の求人数', content: <CompanyCountTab data={split} /> },
            ]),
        ...(SHOW_CONSULTATION_TAB
          ? [
              {
                id: 'consultation',
                label: '採用のヒント',
                content: (
                  <ConsultationTab
                    data={report.consultation}
                    unit={report.meta.is_hourly ? '円/時' : '万円/月'}
                    decimals={report.excel.decimals}
                  />
                ),
              },
            ]
          : []),
      ]}
    />
  );
}
/* eslint-enable @typescript-eslint/no-unnecessary-condition */
