"""Generate limited derived-wage updates; never connect for writes.

python scripts/revalue_minimum_wage_dependencies.py --as-of 2026-10-01 --tables compliance benchmark --output target/wage-derived.sql
python scripts/revalue_minimum_wage_dependencies.py --as-of 2026-10-01 --tables cross --output target/wage-cross.sql
Optional --schema-db performs read-only local column checks before SQL generation.
Compliance uses the same existing postings snapshot and employment grouping.
Rows whose snapshot counts differ are skipped and must be investigated.
Cross preserves existing salary values and pre-CSV historical values; only covered
prefecture/month minimum wages are revalued using month-end effective dates.
"""
import argparse
import sqlite3
from pathlib import Path
from minimum_wage_rates import DEFAULT_SOURCE, load_rates, select_current, today_jst
from update_minimum_wages import quote

AXES = "salary_competitiveness job_market_tightness wage_compliance industry_diversity info_transparency text_urgency posting_freshness real_wage_power labor_fluidity working_age_ratio population_growth foreign_workforce".split()


def build_sql(rates, as_of, tables):
    if 'benchmark' in tables and 'compliance' not in tables:
        raise ValueError('Benchmark requires compliance in the same transaction')
    current = select_current(rates, as_of)
    sql = [f"-- Japan as-of: {as_of}; stop on errors and rollback; user executes once.\nBEGIN IMMEDIATE;\n"]
    if 'compliance' in tables:
        values = ','.join(f"({quote(p)},{r.hourly_min_wage})" for p,r in sorted(current.items()))
        cte = f"""WITH rates(prefecture,wage) AS (VALUES {values}),
        native AS (
          SELECT prefecture,COALESCE(municipality,'') AS municipality,
            CASE WHEN employment_type LIKE '%正社員%' THEN '正社員'
                 WHEN employment_type LIKE '%パート%' THEN 'パート' ELSE 'その他' END AS emp_group,
            salary_min
          FROM postings WHERE prefecture IS NOT NULL AND prefecture<>'' AND salary_type='時給' AND salary_min>0
        ), scoped AS (
          SELECT * FROM native UNION ALL
          SELECT prefecture,'' AS municipality,emp_group,salary_min FROM native WHERE municipality<>''
        ), ranked AS (
          SELECT s.*,ROW_NUMBER() OVER(PARTITION BY prefecture,municipality,emp_group ORDER BY salary_min) AS rn,
                 COUNT(*) OVER(PARTITION BY prefecture,municipality,emp_group) AS group_n
          FROM scoped s
        ), counts AS (
          SELECT s.prefecture,s.municipality,s.emp_group,r.wage,COUNT(*) AS n,
            SUM(CASE WHEN s.salary_min<r.wage THEN 1 ELSE 0 END) AS below,
            AVG(s.salary_min) AS avg_wage,
            AVG(CASE WHEN rn IN ((group_n+1)/2,(group_n+2)/2) THEN salary_min END) AS median_wage
          FROM ranked s JOIN rates r USING(prefecture)
          GROUP BY s.prefecture,s.municipality,s.emp_group HAVING COUNT(*)>=5
        )
        """
        match = "c.prefecture=w.prefecture AND c.municipality=w.municipality AND c.emp_group=w.emp_group AND c.n=w.total_hourly_postings"
        sql.append(cte + f"""UPDATE v2_wage_compliance AS w SET
          min_wage=(SELECT wage FROM counts c WHERE {match}),
          below_min_count=(SELECT below FROM counts c WHERE {match}),
          below_min_rate=(SELECT 1.0*below/n FROM counts c WHERE {match}),
          avg_hourly_wage=(SELECT avg_wage FROM counts c WHERE {match}),
          median_hourly_wage=(SELECT median_wage FROM counts c WHERE {match})
          WHERE EXISTS(SELECT 1 FROM counts c WHERE {match});\n""")
        sql.append("-- No inserts/deletes. Count changes are skipped; same-count inputs update mean/median from the same salary rows.\n")
    if 'benchmark' in tables:
        match = "c.prefecture=b.prefecture AND c.municipality=b.municipality AND c.emp_group=b.emp_group"
        valid = "c.prefecture=v.prefecture AND c.municipality=v.municipality AND c.emp_group=v.emp_group AND c.total_hourly_postings=v.n AND c.min_wage=v.wage AND c.below_min_count=v.below AND c.avg_hourly_wage=v.avg_wage AND c.median_hourly_wage=v.median_wage"
        sql.append(cte+f"UPDATE v2_region_benchmark AS b SET wage_compliance=(SELECT (1.0-c.below_min_rate)*100 FROM v2_wage_compliance c WHERE {match}) WHERE EXISTS(SELECT 1 FROM v2_wage_compliance c JOIN counts v ON {valid} WHERE {match});\n")
        total = '+'.join(f'COALESCE({axis},0.0)' for axis in AXES)
        count = '+'.join(f'({axis} IS NOT NULL)' for axis in AXES)
        sql.append(cte+f"UPDATE v2_region_benchmark AS b SET composite_benchmark=({total})/NULLIF(({count}),0) WHERE EXISTS(SELECT 1 FROM v2_wage_compliance c JOIN counts v ON {valid} WHERE {match});\n")
    if 'cross' in tables:
        # CTE VALUES materializes the versioned source without creating persistent tables.
        values = ','.join(f"({quote(r.prefecture)},{quote(r.effective_date.isoformat())},{r.hourly_min_wage})" for r in rates)
        cutoff = "date(w.year_month||'-01','+1 month','-1 day')"
        selected = f"SELECT r.wage FROM rates r WHERE r.prefecture=w.prefecture AND r.effective_date<={cutoff} ORDER BY r.effective_date DESC LIMIT 1"
        sql.append(f"WITH rates(prefecture,effective_date,wage) AS (VALUES {values}) UPDATE cross_wage_public AS w SET min_wage_hourly=({selected}),min_wage_monthly_160h=160*({selected}) WHERE EXISTS(SELECT 1 FROM rates r WHERE r.prefecture=w.prefecture AND r.effective_date<={cutoff});\n")
        sql.append("-- Nationwide mixed-month wage cannot be computed without weights. Existing national annual references are retained; they must not be labeled monthly legal rates. New CSV generation leaves national minimum-wage fields NULL.\n")
    sql.append("COMMIT;\n-- Read-only verification:\n")
    if 'compliance' in tables:
        sql.append(cte + "SELECT c.prefecture,c.municipality,c.emp_group,c.n,w.total_hourly_postings,c.wage,w.min_wage,c.below,w.below_min_count FROM counts c LEFT JOIN v2_wage_compliance w ON c.prefecture=w.prefecture AND c.municipality=w.municipality AND c.emp_group=w.emp_group WHERE w.min_wage IS NULL OR c.wage<>w.min_wage OR c.n<>w.total_hourly_postings OR c.below<>w.below_min_count;\n")
    if 'cross' in tables:
        sql.append("SELECT prefecture,year_month,min_wage_hourly,min_wage_monthly_160h FROM cross_wage_public WHERE prefecture IN ('京都府','沖縄県') AND year_month>='2026-09' ORDER BY prefecture,year_month;\n")
    return ''.join(sql)


def inspect_schema(path, tables):
    if 'benchmark' in tables and 'compliance' not in tables:
        raise ValueError('Benchmark requires compliance in the same transaction')
    requirements = {}
    if 'compliance' in tables:
        requirements['postings'] = {'prefecture','municipality','employment_type','salary_type','salary_min'}
        requirements['v2_wage_compliance'] = {'prefecture','municipality','emp_group','total_hourly_postings','min_wage','below_min_count','below_min_rate','avg_hourly_wage','median_hourly_wage'}
    if 'benchmark' in tables:
        requirements['v2_wage_compliance'] = {'prefecture','municipality','emp_group','below_min_rate'} | requirements.get('v2_wage_compliance',set())
        requirements['v2_region_benchmark'] = {'prefecture','municipality','emp_group','composite_benchmark'} | set(AXES)
    if 'cross' in tables:
        requirements['cross_wage_public'] = {'prefecture','year_month','min_wage_hourly','min_wage_monthly_160h'}
    with sqlite3.connect(Path(path).resolve().as_uri()+'?mode=ro',uri=True) as db:
        for table, required in requirements.items():
            columns = {r[1] for r in db.execute(f'PRAGMA table_info({table})')}
            if not required.issubset(columns):
                raise ValueError(f'Incompatible columns: {table}')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--as-of',default=today_jst().isoformat())
    parser.add_argument('--source',type=Path,default=DEFAULT_SOURCE)
    parser.add_argument('--tables',nargs='+',choices=['compliance','benchmark','cross'],required=True)
    parser.add_argument('--schema-db',type=Path)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    if 'benchmark' in args.tables and 'compliance' not in args.tables:
        parser.error('--tables benchmark requires compliance in the same transaction')
    if args.schema_db:
        inspect_schema(args.schema_db,args.tables)
    args.output.write_text(build_sql(load_rates(args.source),args.as_of,args.tables),encoding='utf-8')
    print('Generated SQL only; no DB writes. '+('Schema checked read-only.' if args.schema_db else 'Schema NOT checked; inspect before user execution.'))


if __name__=='__main__':
    main()
