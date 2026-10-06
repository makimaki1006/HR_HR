import type { CompetitorReport } from '../../generated/CompetitorReport';
import { Tabs } from './Tabs';
import { ExcelTab } from './tabs/ExcelTab';
import { ConsultationTab } from './tabs/ConsultationTab';
import { GoogleTab } from './tabs/GoogleTab';
import { IndeedTab } from './tabs/IndeedTab';
import { PopulationTab } from './tabs/PopulationTab';

export const TAB_IDS = ['excel', 'google', 'indeed', 'population', 'consultation'] as const;
export type TabId = (typeof TAB_IDS)[number];

export function isTabId(value: string | null): value is TabId {
  return value !== null && (TAB_IDS as readonly string[]).includes(value);
}

interface Props {
  report: CompetitorReport;
  tab: TabId;
  onTabChange: (tab: TabId) => void;
}

/** 5 タブのレポート本体。タブの ID・順序は旧画面 (PDF の収まり検査) と同じ。 */
export function ReportView({ report, tab, onTabChange }: Props) {
  return (
    <Tabs
      label="競合調査の表示切り替え"
      selected={tab}
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
      ]}
    />
  );
}
