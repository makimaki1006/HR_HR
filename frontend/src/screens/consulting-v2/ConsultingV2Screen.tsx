// /app/consulting-v2: 新しいコンサルKPI の画面案 (ナビ「コンサルKPI v2」)。
// 今のコンサルKPI (/consulting) と同じ /api/consulting/* の応答を読み、同じ値を描く。このファイルは読み取り専用で、書き込みは無い。
import { useCallback } from 'react';
import { useQueryRoute } from '../admin/useQueryRoute';
import { BoardView } from './BoardView';
import { DetailView } from './DetailView';
import { TeamView } from './TeamView';
import { TodayView } from './TodayView';
import { parseRoute, routeHref, VIEWS, type Route } from './route';

export function ConsultingV2Screen() {
  const [route, navigate] = useQueryRoute(parseRoute, routeHref);
  const go = useCallback(
    (p: Partial<Route>): void => {
      navigate({ ...route, ...p });
    },
    [navigate, route],
  );
  const open = (view: Route['view']): void => {
    navigate({ view, c: '', flag: '', band: '', id: '' });
  };
  const current = VIEWS.find((v) => v.key === route.view) ?? VIEWS[0];
  return (
    <div className="cv2">
      <header className="cv2-header">
        <span className="cv2-brand">
          HR_HR <span>コンサルKPI v2</span>
        </span>
        <span className="cv2-header-links">
          <a href="/consulting">今のコンサルKPIを開く</a>
          <a href="/">← ダッシュボード</a>
        </span>
      </header>
      <div className="cv2-layout">
        <aside className="cv2-side" aria-label="画面の切り替え">
          <p>毎日</p>
          {VIEWS.map((v) => (
            <button
              key={v.key}
              type="button"
              className={v.key === route.view ? 'cv2-side-on' : undefined}
              aria-current={v.key === route.view ? 'page' : undefined}
              onClick={() => { open(v.key); }}
            >
              {v.label}
            </button>
          ))}
          <small>このほかの画面（顧客と拠点、満了と継続、成果と継続など）は、今のコンサルKPIで見られます。</small>
        </aside>
        <main className="cv2-main" id="cv2-main">
          <div className="cv2-title">
            <div>
              <h1>コンサルKPI</h1>
              <span className="cv2-dim">{current?.note}</span>
            </div>
            <small>今のコンサルKPIと同じデータを表示しています</small>
          </div>
          {route.view === 'team' && <TeamView go={go} />}
          {route.view === 'today' && <TodayView route={route} go={go} />}
          {route.view === 'board' && <BoardView route={route} go={go} />}
          {route.view === 'detail' && <DetailView route={route} go={go} />}
        </main>
      </div>
    </div>
  );
}
