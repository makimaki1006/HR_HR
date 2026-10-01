"""Versioned public minimum wages. Importing this module performs no I/O."""
import csv
from dataclasses import dataclass
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

DEFAULT_SOURCE = Path(__file__).resolve().parents[1] / "data/minimum_wage_rates.csv"
NATIONAL_AVERAGES = {2025: 1121, 2026: 1177}
def today_jst():
    return datetime.now(timezone(timedelta(hours=9))).date()
PREFECTURES = set("北海道 青森県 岩手県 宮城県 秋田県 山形県 福島県 茨城県 栃木県 群馬県 埼玉県 千葉県 東京都 神奈川県 新潟県 富山県 石川県 福井県 山梨県 長野県 岐阜県 静岡県 愛知県 三重県 滋賀県 京都府 大阪府 兵庫県 奈良県 和歌山県 鳥取県 島根県 岡山県 広島県 山口県 徳島県 香川県 愛媛県 高知県 福岡県 佐賀県 長崎県 熊本県 大分県 宮崎県 鹿児島県 沖縄県".split())


@dataclass(frozen=True)
class Rate:
    fiscal_year: int
    prefecture: str
    hourly_min_wage: int
    effective_date: date
    source_url: str


def load_rates(path=DEFAULT_SOURCE):
    """Reject duplicate, incomplete, invalid or unreferenced annual datasets."""
    with Path(path).open(encoding="utf-8-sig", newline="") as source:
        reader = csv.DictReader(source)
        required = {"fiscal_year", "prefecture", "hourly_min_wage", "effective_date", "source_url"}
        if not required.issubset(reader.fieldnames or []):
            raise ValueError("Minimum-wage source columns do not match the contract")
        rates = []
        keys = set()
        for row in reader:
            rate = Rate(int(row["fiscal_year"]), row["prefecture"],
                        int(row["hourly_min_wage"]), date.fromisoformat(row["effective_date"]),
                        row["source_url"])
            key = rate.fiscal_year, rate.prefecture
            if key in keys or rate.hourly_min_wage <= 0 or not rate.source_url.startswith("https://"):
                raise ValueError(f"Invalid minimum-wage record: {key}")
            if rate.effective_date.year not in (rate.fiscal_year, rate.fiscal_year + 1):
                raise ValueError(f"Effective date outside fiscal year: {key}")
            keys.add(key)
            rates.append(rate)
    years = {r.fiscal_year for r in rates}
    if not years:
        raise ValueError("Empty minimum-wage source")
    baseline = None
    for year in years:
        names = {r.prefecture for r in rates if r.fiscal_year == year}
        if names != PREFECTURES or (baseline is not None and names != baseline):
            raise ValueError(f"Expected the same 47 prefectures for fiscal year {year}")
        baseline = names
    return rates


def select_current(rates, as_of=None):
    """Choose each prefecture's latest actually effective rate, never a future rate."""
    as_of = today_jst() if as_of is None else as_of
    if isinstance(as_of, str):
        as_of = date.fromisoformat(as_of)
    selected = {}
    for rate in rates:
        if rate.effective_date <= as_of:
            old = selected.get(rate.prefecture)
            if old is None or (rate.effective_date, rate.fiscal_year) > (old.effective_date, old.fiscal_year):
                selected[rate.prefecture] = rate
    if len(selected) != 47:
        raise ValueError(f"No effective rate for all 47 prefectures on {as_of}")
    return selected
