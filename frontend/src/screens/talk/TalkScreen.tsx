// /app/talk: トークの振り返り (ADR-033)。Google ログインした社内の人なら誰でも開ける。
// 画面の中身は管理画面の ?view=talk と同じ TalkPanel を使う。URL は ?from=&to=&team=&owner=&call= (view は付けない)。
import { TalkPanel, type TalkRoute } from '../admin/TalkPanel';
import { parseAdminRoute, type AdminRoute } from '../admin/route';
import { useQueryRoute } from '../admin/useQueryRoute';

export function parseTalkRoute(search: string): TalkRoute {
  const q = new URLSearchParams(search);
  q.set('view', 'talk');
  return parseAdminRoute(`?${q.toString()}`) as TalkRoute;
}

export function talkHref(route: TalkRoute): string {
  const q = new URLSearchParams();
  for (const k of ['from', 'to', 'team', 'owner', 'call', 'people', 'only', 'cmp_from', 'cmp_to', 'vs_team'] as const) {
    if (route[k] !== '') q.set(k, route[k]);
  }
  // 架空サンプルの表示は、画面を移っても保つ
  if (typeof window !== 'undefined' && new URLSearchParams(window.location.search).get('mode') === 'fixture') q.set('mode', 'fixture');
  const s = q.toString();
  return s === '' ? window.location.pathname : `?${s}`;
}

export function TalkScreen() {
  const [route, navigate] = useQueryRoute(parseTalkRoute, talkHref);
  const go = (r: AdminRoute): void => {
    if (r.view === 'talk') navigate(r);
  };
  return (
    <div className="w8-page">
      <nav className="w8-nav" aria-label="トークの振り返り">
        <a href="/" className="w8-nav-muted">
          ← ダッシュボード
        </a>
      </nav>
      <TalkPanel route={route} navigate={go} />
    </div>
  );
}
