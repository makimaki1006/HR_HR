import { useState } from 'react';
import type { Bucket } from '../../../generated/Bucket';
import type { CompanyGroup } from '../../../generated/CompanyGroup';
import type { PopularitySplit } from '../../../generated/PopularitySplit';
import type { TagStat } from '../../../generated/TagStat';
import { fixed, fmtInt, MISSING } from '../format';

/** popular / n を四捨五入した整数%。n が 0 のときは null。 */
export function ratePct(popular: number, n: number): number | null {
  return n === 0 ? null : Math.round((popular / n) * 100);
}
const rateText = (popular: number, n: number): string => {
  const p = ratePct(popular, n);
  return p === null ? MISSING : `${String(p)}%`;
};

const SUBS = [
  { id: 'age', label: '掲載の長さ' },
  { id: 'company', label: '会社' },
  { id: 'salary', label: '給与帯' },
  { id: 'tags', label: '条件タグ' },
  { id: 'writing', label: '書き方' },
  { id: 'clarity', label: 'わかりやすさ' },
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
    <table className="cmp-table">
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
    </table>
  );
}

function TagTable({ heading, rows }: { heading: string; rows: readonly TagStat[] }) {
  return (
    <table className="cmp-table">
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
            <td colSpan={5}>該当するタグがありません。</td>
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
    </table>
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
          <table className="cmp-table">
            <thead>
              <tr>
                <th>タグ</th>
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
          </table>
        </section>
      )}
    </>
  );
}

function GroupCompare({ multi, single }: { multi: CompanyGroup; single: CompanyGroup }) {
  return (
    <table className="cmp-table">
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
    </table>
  );
}

const manYen = (v: number | null): string => (v === null ? MISSING : `${fixed(v / 10000, 1)}万円`);

function SalaryView({ data }: { data: PopularitySplit }) {
  const s = data.salary;
  return (
    <>
      <h3 className="cmp-block-title">月給下限の中央値（参考）</h3>
      <table className="cmp-table">
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
      </table>
      <h3 className="cmp-block-title">給与帯別の人気・超人気の割合</h3>
      <BucketTable heading="区分" rows={s.bands} />
    </>
  );
}

function PopularityBody({ sub, data }: { sub: SubId; data: PopularitySplit }) {
  switch (sub) {
    case 'age':
      return <AgeView data={data} />;
    case 'company': {
      const top = [...data.companies.rows].sort((a, b) => b.n - a.n).slice(0, 10);
      return (
        <>
          <h3 className="cmp-block-title">複数の求人を出す会社と、1件の会社</h3>
          <GroupCompare multi={data.companies.multi} single={data.companies.single} />
          <h3 className="cmp-block-title">上位の会社の人気・超人気の割合</h3>
          <BucketTable
            heading="会社名"
            rows={top.map((r) => ({ label: r.name, n: r.n, popular: r.popular, super_popular: r.super_popular }))}
          />
        </>
      );
    }
    case 'salary':
      return <SalaryView data={data} />;
    case 'tags':
      return (
        <>
          <h3 className="cmp-block-title">福利厚生系タグ別</h3>
          <TagTable heading="区分" rows={data.tags.welfare} />
          <h3 className="cmp-block-title">応募しやすさ系タグ別</h3>
          <TagTable heading="区分" rows={data.tags.easy_apply} />
          <h3 className="cmp-block-title">条件タグの数別</h3>
          <BucketTable heading="区分" rows={data.tags.count_buckets} />
        </>
      );
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
          <h3 className="cmp-block-title">わかりやすさの点別</h3>
          <BucketTable heading="区分" rows={data.clarity} />
          <p className="cmp-note">
            見出しがある／給与の数字がある／タグが3個以上／休日の日数が書いてある、を1点ずつ数えた合計。福利厚生・タグ数と重なる部分があります。
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
      <div className="cmp-pop-sub" role="group" aria-label="見方の切り替え">
        {SUBS.map((s) => (
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
