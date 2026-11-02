import { useState } from 'react';
import type { FeatureGroup } from '../../../generated/FeatureGroup';
import type { PopularitySplit } from '../../../generated/PopularitySplit';
import { FeatureChart } from '../FeatureChart';
import { pctText } from '../featureModel';
import { fmtInt } from '../format';

const CAUTION = '割合の母数は項目ごとに異なります。件数が20件未満の比較は薄い色で表示します。載っていない項目を「なし」と断定することはできません。';

function GroupPanel({ group }: { group: FeatureGroup }) {
  const [sub, setSub] = useState(0);
  const multi = group.charts.length > 1;
  const chart = group.charts[Math.min(sub, group.charts.length - 1)];
  if (!chart) return null;
  return (
    <section className="cmp-feat-panel" aria-label={group.label}>
      {multi && (
        <div className="cmp-feat-pills">
          {group.charts.map((c, i) => (
            <button
              type="button"
              key={c.id}
              className="cmp-feat-pill"
              aria-pressed={i === sub}
              onClick={() => {
                setSub(i);
              }}
            >
              {c.title}
            </button>
          ))}
        </div>
      )}
      <h3>{multi ? `${chart.title}ごとの割合` : `${group.label}の項目ごとの割合`}</h3>
      <FeatureChart key={chart.id} chart={chart} />
    </section>
  );
}

export function PopularityTab({ data }: { data: PopularitySplit }) {
  const [gid, setGid] = useState<string | null>(null);
  const groups = data.features;
  const group = groups.find((g) => g.id === gid) ?? groups[0];
  return (
    <div className="cmp-feat">
      <p className="cmp-feat-lead">
        アップロードした1本のCSVの求人を、載っている項目(賞与あり・車通勤OKなど)ごとに分けて、人気・超人気の求人が占める割合を比べます。
      </p>
      <div className="cmp-feat-sum">
        <strong>{`全${fmtInt(data.total)}件のうち、人気・超人気${fmtInt(data.popular)}件(${pctText(data.popular, data.total)})`}</strong>
      </div>
      {group === undefined ? (
        <p className="cmp-feat-note2">このCSVでは項目別の比較を出せませんでした。</p>
      ) : (
        <>
          <div className="cmp-feat-groups">
            {groups.map((g) => (
              <button
                type="button"
                key={g.id}
                className="cmp-feat-gcard"
                aria-pressed={g.id === group.id}
                onClick={() => {
                  setGid(g.id);
                }}
              >
                <b>{g.label}</b>
                <span>{g.caption}</span>
              </button>
            ))}
          </div>
          <div className="cmp-feat-main">
            <div className="cmp-feat-note">{CAUTION}</div>
            <GroupPanel key={group.id} group={group} />
          </div>
        </>
      )}
    </div>
  );
}
