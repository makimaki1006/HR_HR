import { useState } from 'react';
import type { Bucket } from '../../../generated/Bucket';
import type { BiasTagGap } from '../../../generated/BiasTagGap';
import type { CompanyBias } from '../../../generated/CompanyBias';
import type { CompanyGroup } from '../../../generated/CompanyGroup';
import type { ReachBandTags } from '../../../generated/ReachBandTags';
import type { ReachCell } from '../../../generated/ReachCell';
import type { PopularitySplit } from '../../../generated/PopularitySplit';
import type { TagStat } from '../../../generated/TagStat';
import { fixed, fmtInt, MISSING } from '../format';
import { VisualTable } from '../charts/PopularityChart';

/** popular / n を四捨五入した整数%。n が 0 のときは null。 */
export function ratePct(popular: number, n: number): number | null {
  return n === 0 ? null : Math.round((popular / n) * 100);
}
const rateText = (popular: number, n: number): string => {
  const p = ratePct(popular, n);
  return p === null ? MISSING : `${String(p)}%`;
};

const GROUPS = [
  { label: '求人の条件', ids: ['salary', 'tags', 'reach', 'style'] },
  { label: '求人票の記載', ids: ['writing', 'clarity'] },
  { label: '掲載期間・会社', ids: ['age', 'company'] },
] as const;

const SUBS = [
  { id: 'age', label: '掲載の長さ' },
  { id: 'company', label: '会社' },
  { id: 'salary', label: '給与帯' },
  { id: 'tags', label: 'キーワード' },
  { id: 'reach', label: '対象の広さ' },
  { id: 'writing', label: '書き方' },
  { id: 'clarity', label: '記載4項目の数' },
  { id: 'style', label: '勤務形態' },
] as const;
type SubId = (typeof SUBS)[number]['id'];

function RateCell({ popular, n }: { popular: number; n: number }) {
  const p = ratePct(popular, n);
  return (
    <>
      <td className="cmp-num">{p === null ? MISSING : `${String(p)}%`}</td>
      <td className="cmp-hbar-cell">
        {p !== null && <i className="cmp-hbar" style={{ width: `${String(p)}%` }} />}
      </td>
    </>
  );
}

function BucketTable({ heading, rows }: { heading: string; rows: readonly Bucket[] }) {
  return (
    <VisualTable chart={{unit:'%',series:['人気・超人気の割合'],rows:rows.map(r=>({label:r.label,values:[{value:ratePct(r.popular,r.n),n:r.n,popular:r.popular}]}))}}>
      <thead>
        <tr>
          <th>{heading}</th>
          <th className="cmp-num">求人数</th>
          <th className="cmp-num">人気・超人気</th>
          <th className="cmp-num">割合</th>
          <th aria-label="割合の棒" />
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={5}>該当する求人がありません。</td>
          </tr>
        )}
        {rows.map((r) => (
          <tr key={r.label}>
            <td>{r.label}</td>
            <td className="cmp-num">{fmtInt(r.n)}</td>
            <td className="cmp-num">{fmtInt(r.popular)}</td>
            <RateCell popular={r.popular} n={r.n} />
          </tr>
        ))}
      </tbody>
    </VisualTable>
  );
}

function TagTable({ heading, rows }: { heading: string; rows: readonly TagStat[] }) {
  return (
    <VisualTable chart={{unit:'%',series:['人気・超人気の割合'],rows:rows.map(r=>({label:r.tag,values:[{value:ratePct(r.popular,r.n),n:r.n,popular:r.popular}]}))}}>
      <thead>
        <tr>
          <th>{heading}</th>
          <th className="cmp-num">求人数</th>
          <th className="cmp-num">人気・超人気</th>
          <th className="cmp-num">割合</th>
          <th aria-label="割合の棒" />
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={5}>該当するキーワードがありません。</td>
          </tr>
        )}
        {rows.map((r) => (
          <tr key={r.tag}>
            <td>{r.tag}</td>
            <td className="cmp-num">{fmtInt(r.n)}</td>
            <td className="cmp-num">{fmtInt(r.popular)}</td>
            <RateCell popular={r.popular} n={r.n} />
          </tr>
        ))}
      </tbody>
    </VisualTable>
  );
}

function KeywordChips({ tags }: { tags: readonly TagStat[] }) {
  if (tags.length === 0) return <p className="cmp-note">該当するキーワードがありません。</p>;
  return (
    <p>
      {tags.map((t) => (
        <span key={t.tag} className="cmp-chip">
          {t.tag}（n={fmtInt(t.n)}・人気 {fmtInt(t.popular)}）
        </span>
      ))}
    </p>
  );
}

const pct1 = (v: number): string => `${fixed(v, 1)}%`;

function AgeView({ data }: { data: PopularitySplit }) {
  const sr = data.super_recent;
  return (
    <>
      <h3 className="cmp-block-title">掲載の長さ別の人気・超人気の割合</h3>
      <BucketTable heading="区分" rows={data.by_age} />
      <p className="cmp-note">掲載日数は29日まで実日数、30日以上は一括。</p>
      <h3 className="cmp-block-title">掲載の長さで、人気側に多いキーワード</h3>
      <div className="cmp-pop-cols">
        {data.tags_by_age.map((g) => (
          <div key={g.label}>
            <strong>{g.label}</strong>
            <KeywordChips tags={g.tags} />
          </div>
        ))}
      </div>
      {sr && (
        <section aria-label="14日以内の超人気">
          <h3 className="cmp-block-title">14日以内の超人気</h3>
          <p className="cmp-note">
            超人気 n={fmtInt(sr.n_super)}／比較 n={fmtInt(sr.n_base)}
          </p>
          <VisualTable chart={{unit:'%',series:[`超人気側の割合（${String(sr.n_super)}件）`,`比較側の割合（${String(sr.n_base)}件）`],rows:[...sr.tags.map(t=>({label:t.tag,values:[{value:t.super_pct,n:sr.n_super},{value:t.base_pct,n:sr.n_base}]})),{label:'見出しの資格・信用語',values:[{value:sr.credential_share.super_pct,n:sr.n_super},{value:sr.credential_share.base_pct,n:sr.n_base}]}]}}>
            <thead>
              <tr>
                <th>キーワード</th>
                <th className="cmp-num">超人気側の割合 (n={fmtInt(sr.n_super)})</th>
                <th className="cmp-num">比較側の割合 (n={fmtInt(sr.n_base)})</th>
              </tr>
            </thead>
            <tbody>
              {sr.tags.map((t) => (
                <tr key={t.tag}>
                  <td>{t.tag}</td>
                  <td className="cmp-num">{pct1(t.super_pct)}</td>
                  <td className="cmp-num">{pct1(t.base_pct)}</td>
                </tr>
              ))}
              <tr>
                <td>見出しの資格・信用語</td>
                <td className="cmp-num">{pct1(sr.credential_share.super_pct)}</td>
                <td className="cmp-num">{pct1(sr.credential_share.base_pct)}</td>
              </tr>
            </tbody>
          </VisualTable>
        </section>
      )}
    </>
  );
}

function GroupCompare({ multi, single }: { multi: CompanyGroup; single: CompanyGroup }) {
  return (
    <VisualTable chart={{unit:'%',series:['人気・超人気の割合'],rows:[{label:'2件以上',values:[{value:ratePct(multi.popular,multi.n),n:multi.n,popular:multi.popular}]},{label:'1件',values:[{value:ratePct(single.popular,single.n),n:single.n,popular:single.popular}]}]}}>
      <thead>
        <tr>
          <th />
          <th className="cmp-num">会社数</th>
          <th className="cmp-num">求人数</th>
          <th className="cmp-num">人気・超人気</th>
          <th className="cmp-num">割合</th>
        </tr>
      </thead>
      <tbody>
        {(
          [
            ['2件以上', multi],
            ['1件', single],
          ] as const
        ).map(([label, g]) => (
          <tr key={label}>
            <td>{label}</td>
            <td className="cmp-num">{fmtInt(g.companies)}</td>
            <td className="cmp-num">{fmtInt(g.n)}</td>
            <td className="cmp-num">{fmtInt(g.popular)}</td>
            <td className="cmp-num">{rateText(g.popular, g.n)}</td>
          </tr>
        ))}
      </tbody>
    </VisualTable>
  );
}

const signed1 = (v: number): string => {
  const t = fixed(v, 1);
  return t === '-0.0' ? '0.0' : v > 0 ? `+${t}` : t;
};

function BiasTagTable({ heading, rows }: { heading: string; rows: readonly BiasTagGap[] }) {
  return (
    <VisualTable chart={{unit:'pt',series:['会社を除く前の差','会社を除いた後の差'],note:'件数は除く前の該当求人数です。除いた後の該当件数はこの集計では未取得です。',rows:rows.map(r=>({label:r.tag,values:[{value:r.before,n:r.n,nLabel:'除く前の該当求人数'},{value:r.after}]}))}}>
      <thead>
        <tr>
          <th>{heading}</th>
          <th className="cmp-num">求人数</th>
          <th className="cmp-num">会社を除く前の差</th>
          <th className="cmp-num">会社を除いた後の差</th>
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={4}>該当するキーワードがありません。</td>
          </tr>
        )}
        {rows.map((r) => (
          <tr key={r.tag}>
            <td>{r.tag}</td>
            <td className="cmp-num">{fmtInt(r.n)}</td>
            <td className="cmp-num">{signed1(r.before)}</td>
            <td className="cmp-num">{r.after === null ? MISSING : signed1(r.after)}</td>
          </tr>
        ))}
      </tbody>
    </VisualTable>
  );
}

function BiasView({ b }: { b: CompanyBias }) {
  return (
    <>
      <h3 className="cmp-block-title">判定</h3>
      <p>
        人気・超人気の求人 {fmtInt(b.popular)} 件（n={fmtInt(b.total)}）のうち、会社に偏りがある会社は{' '}
        {fmtInt(b.biased_companies)} 社（掲載3件以上の {fmtInt(b.judged_companies)} 社中）。その会社の人気・超人気は{' '}
        {fmtInt(b.biased_popular)} 件で人気・超人気全体の {fixed(b.biased_popular_pct, 1)}%、求人は {fmtInt(b.biased_n)}{' '}
        件で全求人の {fixed(b.biased_n_pct, 1)}%。人気が期待を下回る会社は {fmtInt(b.fewer_companies)} 社。
      </p>
      <VisualTable chart={{unit:'件',series:['実際の人気・超人気','期待人気数'],rows:b.rows.map(r=>({label:r.name,values:[{value:r.popular,n:r.n},{value:r.expected,n:r.n}]}))}}>
        <thead>
          <tr>
            <th>会社名</th>
            <th className="cmp-num">掲載数</th>
            <th className="cmp-num">人気・超人気</th>
            <th className="cmp-num">期待人気数</th>
            <th className="cmp-num">差</th>
            <th>判定</th>
          </tr>
        </thead>
        <tbody>
          {b.rows.length === 0 && (
            <tr>
              <td colSpan={6}>掲載3件以上の会社がありません。</td>
            </tr>
          )}
          {b.rows.map((r) => (
            <tr key={r.name}>
              <td>{r.name}</td>
              <td className="cmp-num">{fmtInt(r.n)}</td>
              <td className="cmp-num">{fmtInt(r.popular)}</td>
              <td className="cmp-num">{fixed(r.expected, 1)}</td>
              <td className="cmp-num">{signed1(r.diff)}</td>
              <td>{r.verdict}</td>
            </tr>
          ))}
        </tbody>
      </VisualTable>
      <p className="cmp-note">
        期待人気数は、その会社の求人を除いたほかの会社の求人のうち、給与帯・キーワードの数・掲載の長さが同じ求人の人気・超人気の割合から出しています。
        同じ層のほかの会社の求人が10件に満たないときは、掲載の長さを外した層、給与帯だけの層、ほかの会社全体の割合の順に切り替えています
        （掲載の長さを外した求人 {fmtInt(b.fallback_no_age)} 件、給与帯だけ {fmtInt(b.fallback_salary_only)} 件、全体の割合{' '}
        {fmtInt(b.fallback_overall)} 件）。期待より2件以上多く、偏りが偶然とは言えない水準のときを「会社に偏りがある」、
        2件以上少なく同じ水準のときを「人気が期待を下回る」、それ以外を「給与・条件の範囲」としています。表は掲載3件以上の会社で、
        会社に偏りがある会社を先頭に、差の大きい順で最大20社です。
      </p>
      <h3 className="cmp-block-title">会社を除いた条件の差</h3>
      <p className="cmp-note">
        会社に偏りがある {fmtInt(b.biased_companies)} 社の求人を除いた {fmtInt(b.after_total)} 件（人気・超人気{' '}
        {fmtInt(b.after_popular)} 件）で、各キーワードの 人気求人での割合 − 全体の割合（ポイント）を、除く前と並べています。
        会社を除いても差が残るキーワードは、条件によるものとして読めます。求人3件以上のキーワードだけです。
      </p>
      <BiasTagTable heading="人気側に多いキーワード" rows={b.tags_more} />
      <BiasTagTable heading="人気側に少ないキーワード" rows={b.tags_less} />
    </>
  );
}

const manYen = (v: number | null): string => (v === null ? MISSING : `${fixed(v / 10000, 1)}万円`);

function SalaryView({ data }: { data: PopularitySplit }) {
  const s = data.salary;
  return (
    <>
      <h3 className="cmp-block-title">月給下限の中央値（参考）</h3>
      <VisualTable chart={{unit:'万円',series:['月給下限の中央値'],rows:[{label:'人気・超人気',values:[{value:s.popular_median===null?null:s.popular_median/10000,n:s.popular_n}]},{label:'それ以外',values:[{value:s.other_median===null?null:s.other_median/10000,n:s.other_n}]}]}}>
        <thead>
          <tr>
            <th />
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">月給下限の中央値</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>人気・超人気</td>
            <td className="cmp-num">{fmtInt(s.popular_n)}</td>
            <td className="cmp-num">{manYen(s.popular_median)}</td>
          </tr>
          <tr>
            <td>それ以外</td>
            <td className="cmp-num">{fmtInt(s.other_n)}</td>
            <td className="cmp-num">{manYen(s.other_median)}</td>
          </tr>
        </tbody>
      </VisualTable>
      <h3 className="cmp-block-title">給与帯別の人気・超人気の割合</h3>
      <BucketTable heading="区分" rows={s.bands} />
    </>
  );
}

const rate1 = (v: number | null): string => (v === null ? MISSING : `${fixed(v, 1)}%`);

function StratumCell({ c }: { c: ReachCell }) {
  return c.n === 0 ? (
    <>
      <td className="cmp-num">{MISSING}</td>
      <td className="cmp-num">{MISSING}</td>
    </>
  ) : (
    <>
      <td className="cmp-num">{rate1(c.rate_pct)}</td>
      <td className="cmp-num">{fmtInt(c.n)}</td>
    </>
  );
}

function ReachGapTable({ heading, rows }: { heading: string; rows: ReachBandTags['more'] }) {
  return (
    <VisualTable chart={{unit:'pt',series:['人気求人での割合−全体の割合'],rows:rows.map(r=>({label:r.tag,values:[{value:r.diff,n:r.n}]}))}}>
      <thead>
        <tr>
          <th>{heading}</th>
          <th className="cmp-num">求人数</th>
          <th className="cmp-num">差</th>
        </tr>
      </thead>
      <tbody>
        {rows.length === 0 && (
          <tr>
            <td colSpan={3}>該当するキーワードがありません。</td>
          </tr>
        )}
        {rows.map((r) => (
          <tr key={r.tag}>
            <td>{r.tag}</td>
            <td className="cmp-num">{fmtInt(r.n)}</td>
            <td className="cmp-num">{signed1(r.diff)}</td>
          </tr>
        ))}
      </tbody>
    </VisualTable>
  );
}

function ReachView({ data }: { data: PopularitySplit }) {
  const r = data.reach;
  return (
    <>
      <h3 className="cmp-block-title">間口を広げる語の数別</h3>
      <VisualTable chart={{unit:'%',series:[`全求人に占める割合（${String(data.total)}件）`,'条件内の人気・超人気の割合',`人気・超人気全体に占める割合（${String(data.popular)}件）`],rows:r.bands.map(b=>({label:b.label,values:[{value:data.total===0?null:b.n_share_pct,n:data.total},{value:b.rate_pct,n:b.n,popular:b.popular},{value:data.popular===0?null:b.popular_share_pct,n:data.popular}]}))}}>
        <thead>
          <tr>
            <th>間口を広げる語</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">全求人に占める割合</th>
            <th className="cmp-num">人気・超人気</th>
            <th className="cmp-num">人気率</th>
            <th className="cmp-num">人気・超人気全体に占める割合</th>
          </tr>
        </thead>
        <tbody>
          {r.bands.map((b) => (
            <tr key={b.label}>
              <td>{b.label}</td>
              <td className="cmp-num">{fmtInt(b.n)}</td>
              <td className="cmp-num">{pct1(b.n_share_pct)}</td>
              <td className="cmp-num">{fmtInt(b.popular)}</td>
              <td className="cmp-num">{rate1(b.rate_pct)}</td>
              <td className="cmp-num">{pct1(b.popular_share_pct)}</td>
            </tr>
          ))}
        </tbody>
      </VisualTable>
      <p className="cmp-note">
        求人数 n={fmtInt(data.total)}／人気・超人気 n={fmtInt(data.popular)}
      </p>
      <h3 className="cmp-block-title">ほかのキーワードの数を揃えた比較</h3>
      <VisualTable chart={{unit:'%',series:['間口を広げる語なし','間口を広げる語あり'],rows:r.strata.map(s=>({label:s.label,values:[{value:s.without_open.rate_pct,n:s.without_open.n,popular:s.without_open.popular},{value:s.with_open.rate_pct,n:s.with_open.n,popular:s.with_open.popular}]}))}}>
        <thead>
          <tr>
            <th rowSpan={2}>間口を広げる語以外のキーワード</th>
            <th colSpan={2} className="cmp-num">
              間口を広げる語なし
            </th>
            <th colSpan={2} className="cmp-num">
              間口を広げる語あり
            </th>
          </tr>
          <tr>
            <th className="cmp-num">人気率</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">人気率</th>
            <th className="cmp-num">求人数</th>
          </tr>
        </thead>
        <tbody>
          {r.strata.map((s) => (
            <tr key={s.label}>
              <td>{s.label}</td>
              <StratumCell c={s.without_open} />
              <StratumCell c={s.with_open} />
            </tr>
          ))}
        </tbody>
      </VisualTable>
      <h3 className="cmp-block-title">帯ごとのキーワードの差</h3>
      <p className="cmp-note">
        差は、その帯の人気・超人気の求人でのキーワードの割合から、その帯の全求人でのキーワードの割合を引いた値です（求人数3件以上のキーワード。間口を広げる語は除く）。
      </p>
      {r.band_tags.map((b) => (
        <section key={b.label} aria-label={`間口を広げる語 ${b.label}`}>
          <h4 className="cmp-block-title">
            間口を広げる語 {b.label}（求人数 n={fmtInt(b.n)}・人気・超人気 n={fmtInt(b.popular)}）
          </h4>
          {b.shown ? (
            <div className="cmp-pop-cols">
              <ReachGapTable heading="人気側に多いキーワード" rows={b.more} />
              <ReachGapTable heading="人気側に少ないキーワード" rows={b.less} />
            </div>
          ) : (
            <p className="cmp-note">
              求人数 {fmtInt(b.n)} 件のため、この帯の比較は出しません。
            </p>
          )}
        </section>
      ))}
      <h3 className="cmp-block-title">応募のしやすさの印</h3>
      <p className="cmp-note">
        「高返信率」「カンタン応募」は、Indeedが求人カードに付けている印です。印の有無ごとの人気率を並べています。
      </p>
      <VisualTable chart={{unit:'%',series:['人気・超人気の割合'],rows:r.reply.marks.flatMap(m=>[{label:`${m.label} あり`,values:[{value:m.with_mark.rate_pct,n:m.with_mark.n,popular:m.with_mark.popular}]},{label:`${m.label} なし`,values:[{value:m.without_mark.rate_pct,n:m.without_mark.n,popular:m.without_mark.popular}]}])}}>
        <thead>
          <tr>
            <th>印</th>
            <th>有無</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">人気</th>
            <th className="cmp-num">超人気</th>
            <th className="cmp-num">人気率</th>
          </tr>
        </thead>
        <tbody>
          {r.reply.marks.flatMap((m) =>
            [
              { w: 'あり', x: m.with_mark },
              { w: 'なし', x: m.without_mark },
            ].map(({ w, x }) => (
              <tr key={`${m.label}-${w}`}>
                <td>{m.label}</td>
                <td>{w}</td>
                <td className="cmp-num">{fmtInt(x.n)}</td>
                <td className="cmp-num">{x.n === 0 ? MISSING : fmtInt(x.popular)}</td>
                <td className="cmp-num">{x.n === 0 ? MISSING : fmtInt(x.super_popular)}</td>
                <td className="cmp-num">{rate1(x.rate_pct)}</td>
              </tr>
            )),
          )}
        </tbody>
      </VisualTable>
      <p className="cmp-note">人気は人気と超人気の合計です。求人数 n={fmtInt(data.total)}。</p>
      <h3 className="cmp-block-title">間口を広げる語と「高返信率」の組み合わせ</h3>
      <VisualTable chart={{unit:'%',series:['高返信率あり','高返信率なし'],rows:r.reply.cross.map(x=>({label:x.label,values:[{value:x.with_reply.rate_pct,n:x.with_reply.n,popular:x.with_reply.popular},{value:x.without_reply.rate_pct,n:x.without_reply.n,popular:x.without_reply.popular}]}))}}>
        <thead>
          <tr>
            <th rowSpan={2}>間口を広げる語</th>
            <th colSpan={2} className="cmp-num">
              高返信率あり
            </th>
            <th colSpan={2} className="cmp-num">
              高返信率なし
            </th>
          </tr>
          <tr>
            <th className="cmp-num">人気率</th>
            <th className="cmp-num">求人数</th>
            <th className="cmp-num">人気率</th>
            <th className="cmp-num">求人数</th>
          </tr>
        </thead>
        <tbody>
          {r.reply.cross.map((x) => (
            <tr key={x.label}>
              <td>{x.label}</td>
              <StratumCell c={x.with_reply} />
              <StratumCell c={x.without_reply} />
            </tr>
          ))}
        </tbody>
      </VisualTable>
      <p className="cmp-note">求人数が0件の組み合わせは「—」です。</p>
      <p className="cmp-note">
        分け方: 求人に付くキーワードのうち、{r.open_words.join('・')}の数で「なし」「1つ」「2つ以上」に分けています。1つの求人は1回だけ数えます。キーワードの差は求人数が{fmtInt(r.min_band_n)}件以上の帯だけ出します。人気・超人気は、Indeed上でその表示が付いている求人です。
      </p>
    </>
  );
}

function PopularityBody({ sub, data }: { sub: SubId; data: PopularitySplit }) {
  switch (sub) {
    case 'age':
      return <AgeView data={data} />;
    case 'company':
      return (
        <>
          <h3 className="cmp-block-title">複数の求人を出す会社と、1件の会社</h3>
          <GroupCompare multi={data.companies.multi} single={data.companies.single} />
          <BiasView b={data.company_bias} />
        </>
      );
    case 'salary':
      return <SalaryView data={data} />;
    case 'tags':
      return (
        <>
          <h3 className="cmp-block-title">福利厚生系キーワード別</h3>
          <TagTable heading="区分" rows={data.tags.welfare} />
          <h3 className="cmp-block-title">応募しやすさ系キーワード別</h3>
          <TagTable heading="区分" rows={data.tags.easy_apply} />
          <h3 className="cmp-block-title">キーワードの数別</h3>
          <BucketTable heading="区分" rows={data.tags.count_buckets} />
        </>
      );
    case 'reach':
      return <ReachView data={data} />;
    case 'writing':
      return (
        <>
          {data.writing.map((g) => (
            <div key={g.title}>
              <h3 className="cmp-block-title">{g.title}</h3>
              <BucketTable heading="区分" rows={g.rows} />
            </div>
          ))}
        </>
      );
    case 'clarity':
      return (
        <>
          <h3 className="cmp-block-title">記載4項目の数別</h3>
          <BucketTable heading="区分" rows={data.clarity} />
          <p className="cmp-note">
            見出しがある／給与の数字がある／キーワードが3個以上／休日の日数が書いてある、を1点ずつ数えた合計。福利厚生・キーワード数と重なる部分があります。
          </p>
        </>
      );
    case 'style':
      return (
        <>
          <h3 className="cmp-block-title">勤務形態別</h3>
          <BucketTable heading="区分" rows={data.work_style} />
        </>
      );
  }
}

export function PopularityTab({ data }: { data: PopularitySplit }) {
  const [sub, setSub] = useState<SubId>('age');
  const [group, setGroup] = useState(2);
  return (
    <section className="cmp-page" id="competitor-popularity">
      <header className="cmp-page-head">
        <span className="cmp-eyebrow">求人の傾向</span>
        <h2>人気の分かれ方</h2>
        <p className="cmp-sub">求人数 {fmtInt(data.total)} 件（人気・超人気 {fmtInt(data.popular)} 件）</p>
      </header>
      <p className="cmp-note">
        人気・超人気は合算して比べています（Indeed上でその表示が付いている求人とそれ以外）
      </p>
      <div className="cmp-pop-groups" role="group" aria-label="比較する情報">
        {GROUPS.map((g,i)=><button key={g.label} type="button" aria-pressed={group===i} onClick={()=>{setGroup(i);setSub(g.ids[0]);}}>{g.label}</button>)}
      </div>
      <p className="cmp-note">表示されていない条件を「条件なし」と断定できません。割合の差は、条件を変えた効果を示すものではありません。</p>
      <div className="cmp-pop-sub" role="group" aria-label="見方の切り替え">
        {SUBS.filter(s => (GROUPS[group]?.ids as readonly string[] | undefined)?.includes(s.id)).map((s) => (
          <button
            key={s.id}
            type="button"
            className="cmp-pop-sub-btn"
            aria-pressed={sub === s.id}
            onClick={() => {
              setSub(s.id);
            }}
          >
            {s.label}
          </button>
        ))}
      </div>
      <PopularityBody sub={sub} data={data} />
    </section>
  );
}
