import { useState } from 'react';
import { DASH, num, orText, yen } from './format';
import { Async, CaseLink, HubspotLink, Kpi, Section } from './parts';
import type { Route } from './route';
import type { CustomerResponse } from './types';
import { useApi } from './useApi';

export function CustomerView({ route, go }: { route: Route; go: (r: Partial<Route>) => void }) {
  const [reload, setReload] = useState(0);
  const [q, setQ] = useState('');
  const path = route.houjin === '' ? '/api/consulting/customer' : `/api/consulting/customer?houjin=${encodeURIComponent(route.houjin)}`;
  const result = useApi<CustomerResponse>(path, reload);
  return (
    <Async result={result} onRetry={() => { setReload((n) => n + 1); }}>
      {(d) => {
        // 法人を指定していない応答は一覧だけ。既定の法人があればそれを開く
        if (route.houjin === '') {
          const list = (d.index ?? []).filter((c) => q.trim() === '' || (c.name ?? '').includes(q.trim()));
          return (
            <Section title={`法人を選ぶ（${num(list.length)} 社）`}>
              <div className="cv2-filters">
                <label className="cv2-field">
                  法人名の一部
                  <input value={q} onChange={(e) => { setQ(e.target.value); }} />
                </label>
                {d.default_houjin ? (
                  <button type="button" onClick={() => { go({ houjin: d.default_houjin ?? '' }); }}>取引がいちばん多い法人を開く</button>
                ) : null}
              </div>
              <div className="cv2-table-wrap">
                <table className="cv2-table">
                  <thead>
                    <tr>
                      <th scope="col">法人</th>
                      <th scope="col" className="cv2-num">契約数</th>
                      <th scope="col" className="cv2-num">稼働数</th>
                      <th scope="col" className="cv2-num">拠点数</th>
                      <th scope="col" className="cv2-num">累計契約金額</th>
                      <th scope="col" className="cv2-num">従業員数</th>
                      <th scope="col" className="cv2-num">最大継続回数</th>
                    </tr>
                  </thead>
                  <tbody>
                    {list.slice(0, 200).map((c) => (
                      <tr key={c.houjin}>
                        <td>
                          <button type="button" className="cv2-link" onClick={() => { go({ houjin: c.houjin }); }}>{orText(c.name, '法人名なし')}</button>
                          {c.focus === true ? <span className="cv2-tag">重点顧客</span> : null}
                        </td>
                        <td className="cv2-num">{num(c.deals ?? null)}</td>
                        <td className="cv2-num">{num(c.active ?? null)}</td>
                        <td className="cv2-num">{num(c.sites ?? null)}</td>
                        <td className="cv2-num">{yen(c.ltv)}</td>
                        <td className="cv2-num">{c.kigyouzentaininzuu === null || c.kigyouzentaininzuu === undefined ? DASH : `${num(c.kigyouzentaininzuu)}人`}</td>
                        <td className="cv2-num">{c.max_renewal_no === null || c.max_renewal_no === undefined ? DASH : `${num(c.max_renewal_no)}回`}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {list.length > 200 ? <p className="cv2-dim">先頭の 200 社だけ出しています。法人名で絞ってください。</p> : null}
            </Section>
          );
        }
        if (d.meta.found !== true || !d.customer) {
          return (
            <>
              <p className="cv2-note" role="status">この法人は見つかりません。</p>
              <button type="button" onClick={() => { go({ houjin: '' }); }}>法人の一覧へ戻る</button>
            </>
          );
        }
        const c = d.customer;
        const deals = d.deals ?? [];
        const sites = new Map<string, typeof deals>();
        for (const x of deals) {
          const k = x.site_name ?? x.site ?? '';
          sites.set(k, [...(sites.get(k) ?? []), x]);
        }
        const portal = d.meta.hubspot_portal_id;
        return (
          <>
            <p>
              <button type="button" onClick={() => { go({ houjin: '' }); }}>← 法人の一覧へ戻る</button>
            </p>
            <div className="cv2-detail-head">
              <h2>{orText(c.name, '法人名なし')}</h2>
              {c.focus === true && c.focus_why?.length ? <p className="cv2-dim">重点顧客: {c.focus_why.join('・')}</p> : null}
            </div>
            <div className="cv2-kpis">
              <Kpi label="契約数" value={num(c.deals ?? null)} unit="件" note={`稼働中 ${num(c.active ?? null)} 件`} />
              <Kpi label="拠点数" value={num(c.sites ?? null)} unit="拠点" />
              <Kpi label="累計契約金額" value={yen(c.ltv)} note="契約期間全体の金額の合計（月額ではない）" />
              <Kpi label="最大継続回数" value={c.max_renewal_no === null || c.max_renewal_no === undefined ? DASH : num(c.max_renewal_no)} unit="回" note={c.last_expiration ? `直近の満了 ${c.last_expiration}` : undefined} />
            </div>
            <Section title="応募 → 面接 → 承諾">
              <p>
                応募 <b>{num(d.funnel?.oubo ?? null) || DASH}</b> → 面接 <b>{num(d.funnel?.mensetu ?? null) || DASH}</b> → 承諾 <b>{num(d.funnel?.syoudaku ?? null) || DASH}</b>
              </p>
              <p className="cv2-dim">取引に入っている値の合計です。入っていない取引は数えていません。</p>
            </Section>
            {[...sites.entries()].map(([site, ds]) => (
              <Section key={site} title={`拠点: ${site === '' ? '拠点名なし' : site}（契約 ${num(ds.length)} 件）`}>
                <div className="cv2-table-wrap">
                  <table className="cv2-table">
                    <thead>
                      <tr>
                        <th scope="col">案件</th>
                        <th scope="col">ステージ</th>
                        <th scope="col">契約開始</th>
                        <th scope="col">満了日</th>
                        <th scope="col" className="cv2-num">継続</th>
                        <th scope="col" className="cv2-num">金額</th>
                        <th scope="col" className="cv2-num">応募 / 面接 / 承諾</th>
                        <th scope="col">リンク</th>
                      </tr>
                    </thead>
                    <tbody>
                      {ds.map((x) => (
                        <tr key={x.deal_id}>
                          <td>
                            <button type="button" className="cv2-link" onClick={() => { go({ view: 'detail', id: x.deal_id }); }}>{orText(x.name, '取引名なし')}</button>
                            {x.is_active === true ? <span className="cv2-tag">稼働中</span> : null}
                          </td>
                          <td>{x.stage ?? DASH}</td>
                          <td>{x.start ?? DASH}</td>
                          <td>{x.expiration ?? DASH}</td>
                          <td className="cv2-num">{x.renewal_no === null || x.renewal_no === undefined ? DASH : num(x.renewal_no)}</td>
                          <td className="cv2-num">{yen(x.amount)}</td>
                          <td className="cv2-num">{[x.oubo, x.mensetu, x.syoudaku].map((v) => (v === null || v === undefined ? DASH : num(v))).join(' / ')}</td>
                          <td><CaseLink dealId={x.deal_id} /> <HubspotLink portal={portal} dealId={x.deal_id} /></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </Section>
            ))}
            {(d.mtgs ?? []).length > 0 && (
              <Section title="MTG の記録（抽出できたもの）">
                <ul className="cv2-results">
                  {(d.mtgs ?? []).filter((m) => m.extracted === true).slice(0, 20).map((m) => (
                    <li key={`${m.deal_id}-${m.date ?? ''}`}>
                      <b>{m.date ?? DASH}</b> {m.risk ? <span className="cv2-tag">{m.risk}</span> : null}
                      <p className="cv2-dim">やること: {orText(m.todo, '記載なし')} ／ 顧客の懸念: {orText(m.concern, '記載なし')}</p>
                    </li>
                  ))}
                </ul>
              </Section>
            )}
          </>
        );
      }}
    </Async>
  );
}
