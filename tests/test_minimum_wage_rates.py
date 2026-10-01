"""Concrete effective-date, history preservation and read-only schema checks."""
import csv
import sqlite3
import sys
from datetime import date
from pathlib import Path
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from minimum_wage_rates import DEFAULT_SOURCE, NATIONAL_AVERAGES, load_rates, select_current
from update_minimum_wages import build_sql, inspect_schema
from upload_minimum_wage_history import build_rows
from compute_v2_external import compute_minimum_wage


def fixture_db(path):
    db = sqlite3.connect(path)
    db.execute("CREATE TABLE v2_external_minimum_wage(prefecture TEXT PRIMARY KEY,hourly_min_wage INTEGER,effective_date TEXT,fiscal_year INTEGER)")
    db.execute("CREATE TABLE v2_external_minimum_wage_history(fiscal_year INTEGER,prefecture TEXT,hourly_min_wage INTEGER,PRIMARY KEY(fiscal_year,prefecture))")
    db.execute("INSERT INTO v2_external_minimum_wage_history VALUES(2020,'東京都',1013)")
    db.commit()
    return db


def test_source_has_47_prefectures_each_year_and_correct_annual_averages():
    rates = load_rates()
    assert len(rates) == 94
    assert NATIONAL_AVERAGES == {2025:1121, 2026:1177}
    assert sum(r.fiscal_year == 2025 for r in rates) == 47
    assert sum(r.fiscal_year == 2026 for r in rates) == 47
    assert next(r for r in rates if (r.fiscal_year,r.prefecture) == (2025,'東京都')).effective_date == date(2025,10,3)


def test_future_prefecture_is_not_applied_early_and_changes_on_its_date():
    rates = load_rates()
    kyoto = next(r for r in rates if (r.fiscal_year,r.prefecture) == (2026,'京都府'))
    assert kyoto.effective_date == date(2026,11,16)
    before = select_current(rates, '2026-11-15')['京都府']
    after = select_current(rates, '2026-11-16')['京都府']
    assert (before.fiscal_year,before.hourly_min_wage) == (2025,1122)
    assert (after.fiscal_year,after.hourly_min_wage) == (2026,kyoto.hourly_min_wage)
    current = select_current(rates,'2026-10-01')
    assert current['沖縄県'].fiscal_year == 2025
    assert current['京都府'].fiscal_year == 2025
    assert current['香川県'].fiscal_year == 2026


def test_readonly_inspection_and_sql_preserve_existing_history(tmp_path):
    path = tmp_path / 'fixture.db'
    db = fixture_db(path)
    before = path.read_bytes()
    schemas, derived = inspect_schema(path)
    assert path.read_bytes() == before
    assert derived == []
    sql = build_sql(load_rates(),'2026-10-01',schemas)
    assert 'DROP' not in sql
    assert sql.count('BEGIN IMMEDIATE;') == 1
    db.executescript(sql)  # disposable test DB only
    assert db.execute("SELECT hourly_min_wage FROM v2_external_minimum_wage_history WHERE fiscal_year=2020").fetchone() == (1013,)
    assert db.execute("SELECT fiscal_year,hourly_min_wage FROM v2_external_minimum_wage WHERE prefecture='京都府'").fetchone() == (2025,1122)
    assert db.execute("SELECT hourly_min_wage FROM v2_external_minimum_wage_history WHERE fiscal_year=2025 AND prefecture='全国'").fetchone() == (1121,)
    assert db.execute("SELECT hourly_min_wage FROM v2_external_minimum_wage_history WHERE fiscal_year=2026 AND prefecture='全国'").fetchone() == (1177,)
    assert db.execute("SELECT COUNT(*) FROM v2_external_minimum_wage").fetchone() == (47,)
    db.executescript(sql)
    assert db.execute("SELECT COUNT(*) FROM v2_external_minimum_wage_history").fetchone() == (97,)
    db.close()


def test_computation_uses_actual_effective_date(tmp_path):
    db = fixture_db(tmp_path / 'compute.db')
    compute_minimum_wage(db, '2026-10-01')
    assert db.execute("SELECT hourly_min_wage,effective_date,fiscal_year FROM v2_external_minimum_wage WHERE prefecture='京都府'").fetchone() == (1122,'2025-11-21',2025)
    assert db.execute("SELECT COUNT(*) FROM v2_external_minimum_wage_history").fetchone() == (1,)
    db.close()


def test_invalid_schema_is_rejected_without_mutating_db(tmp_path):
    path = tmp_path / 'bad.db'
    sqlite3.connect(path).close()
    before = path.read_bytes()
    with pytest.raises(ValueError,match='Incompatible schema'):
        inspect_schema(path)
    assert path.read_bytes() == before


def test_missing_or_duplicate_source_is_rejected(tmp_path):
    source = tmp_path / 'invalid.csv'
    rows = list(csv.DictReader(DEFAULT_SOURCE.open(encoding='utf-8-sig')))
    with source.open('w',encoding='utf-8',newline='') as out:
        writer = csv.DictWriter(out,fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows+[rows[0]])
    with pytest.raises(ValueError,match='Invalid'):
        load_rates(source)
    with pytest.raises(ValueError,match='No effective rate'):
        select_current(load_rates(),'2025-01-01')


def test_history_generator_never_relables_current_year_as_2025():
    sql = build_sql(load_rates(),'2026-12-03',history_only=True,legacy_history=build_rows())
    assert 'INSERT INTO v2_external_minimum_wage (' not in sql
    assert '(2025,' in sql and '(2026,' in sql
    assert "(2025,'全国',1121)" in sql
    assert "(2026,'全国',1177)" in sql


def test_cross_month_end_boundaries_and_unknown_national_are_not_guessed():
    import pandas as pd
    from build_cross_tables import build_wage_public
    source = pd.DataFrame([{'prefecture':pref,'year_month':ym,'size_class':'5人以上','industry':'調査産業計','scheduled_earnings':200000}
                           for pref in ['京都府','沖縄県','全国'] for ym in ['2026-10','2026-11','2026-12']])
    output = build_wage_public(source,{})
    values={(r.prefecture,r.year_month):r.min_wage_hourly for r in output.itertuples()}
    rates=load_rates()
    assert values['京都府','2026-10'] == 1122
    assert values['京都府','2026-11'] == next(r.hourly_min_wage for r in rates if (r.prefecture,r.fiscal_year)==('京都府',2026))
    assert values['沖縄県','2026-11'] == 1023
    assert values['沖縄県','2026-12'] == next(r.hourly_min_wage for r in rates if (r.prefecture,r.fiscal_year)==('沖縄県',2026))
    assert pd.isna(values['全国','2026-12'])


def test_living_cost_snapshot_metadata_keeps_statistical_years_separate():
    from build_municipality_living_cost_proxy import build_records
    db=sqlite3.connect(':memory:')
    db.execute('CREATE TABLE v2_external_prefecture_stats(prefecture TEXT,price_index REAL)')
    db.execute("INSERT INTO v2_external_prefecture_stats VALUES('京都府',100)")
    db.execute('CREATE TABLE municipality_code_master(municipality_code TEXT,prefecture TEXT,municipality_name TEXT)')
    db.execute("INSERT INTO municipality_code_master VALUES('26100','京都府','京都市')")
    row=build_records(db,'2026-10-01')[0]
    assert row[5] == 1122 and row[7] == 1122
    assert row[10] == 2026
    assert 'price_year=2024;wage_year=2025;wage_effective=2025-11-21;as_of=2026-10-01' in row[9]
    db.close()


def test_derived_sql_revalues_existing_keys_and_preserves_other_axes_and_history():
    from revalue_minimum_wage_dependencies import build_sql as derived_sql, AXES
    db=sqlite3.connect(':memory:')
    db.execute('CREATE TABLE postings(prefecture TEXT,municipality TEXT,employment_type TEXT,salary_type TEXT,salary_min INTEGER)')
    db.executemany("INSERT INTO postings VALUES('京都府','京都市','パート','時給',?)",[(1000,),(1100,),(1122,),(1150,),(1200,)])
    db.execute('CREATE TABLE v2_wage_compliance(prefecture TEXT,municipality TEXT,emp_group TEXT,total_hourly_postings INTEGER,min_wage INTEGER,below_min_count INTEGER,below_min_rate REAL,avg_hourly_wage REAL,median_hourly_wage REAL)')
    db.execute("INSERT INTO v2_wage_compliance VALUES('京都府','京都市','パート',5,1058,1,0.2,999,999)")
    db.execute('CREATE TABLE v2_region_benchmark(prefecture TEXT,municipality TEXT,emp_group TEXT,'+','.join(a+' REAL' for a in AXES)+',composite_benchmark REAL)')
    db.execute("INSERT INTO v2_region_benchmark(prefecture,municipality,emp_group,salary_competitiveness,wage_compliance,composite_benchmark) VALUES('京都府','京都市','パート',40,80,60)")
    db.execute('CREATE TABLE cross_wage_public(prefecture TEXT,year_month TEXT,min_wage_hourly INTEGER,min_wage_monthly_160h INTEGER)')
    db.executemany('INSERT INTO cross_wage_public VALUES(?,?,?,?)',[('京都府','2024-12',1058,169280),('京都府','2026-10',999,159840),('京都府','2026-11',999,159840)])
    sql=derived_sql(load_rates(),'2026-10-01',['compliance','benchmark','cross'])
    assert 'DROP' not in sql
    db.executescript(sql)
    assert db.execute('SELECT min_wage,below_min_count,below_min_rate FROM v2_wage_compliance').fetchone() == (1122,2,0.4)
    assert db.execute('SELECT avg_hourly_wage,median_hourly_wage FROM v2_wage_compliance').fetchone() == (1114.4,1122)
    assert db.execute('SELECT salary_competitiveness,wage_compliance,composite_benchmark FROM v2_region_benchmark').fetchone() == (40,60,50)
    assert db.execute("SELECT min_wage_hourly FROM cross_wage_public WHERE year_month='2024-12'").fetchone()==(1058,)
    assert db.execute("SELECT min_wage_hourly FROM cross_wage_public WHERE year_month='2026-10'").fetchone()==(1122,)
    expected=next(r.hourly_min_wage for r in load_rates() if (r.prefecture,r.fiscal_year)==('京都府',2026))
    assert db.execute("SELECT min_wage_hourly FROM cross_wage_public WHERE year_month='2026-11'").fetchone()==(expected,)
    # Changed snapshot size must skip both compliance and benchmark.
    db.execute("INSERT INTO postings VALUES('京都府','京都市','パート','時給',1300)")
    db.execute('UPDATE v2_region_benchmark SET wage_compliance=99,composite_benchmark=77')
    db.executescript(sql)
    assert db.execute('SELECT avg_hourly_wage,median_hourly_wage FROM v2_wage_compliance').fetchone() == (1114.4,1122)
    assert db.execute('SELECT wage_compliance,composite_benchmark FROM v2_region_benchmark').fetchone() == (99,77)
    db.close()


def test_benchmark_alone_is_rejected_before_schema_or_sql_work(tmp_path):
    from revalue_minimum_wage_dependencies import build_sql as derived_sql, inspect_schema as derived_schema
    with pytest.raises(ValueError,match='Benchmark requires compliance'):
        derived_sql(load_rates(),'2026-10-01',['benchmark'])
    with pytest.raises(ValueError,match='Benchmark requires compliance'):
        derived_schema(tmp_path/'does-not-exist.db',['benchmark'])
    assert not (tmp_path/'does-not-exist.db').exists()
