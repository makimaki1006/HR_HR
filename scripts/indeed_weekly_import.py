"""Write weekly Indeed (HRハッカー sponsored job) results onto HubSpot Listing objects (0-420).

Source: the weekly CSV files exported from the HRハッカー Indeed console, one file per Monday-Sunday
week (JobsCampaigns_YYYYMMDD_YYYYMMDD.csv). Rows are summed per (参照番号, week); 参照番号 is the
Listing property id_hrhakkaa. Nothing is guessed: references with no matching Listing are skipped.

Written properties (they already exist in HubSpot; this script never creates properties):
  indeed_weekly_perf          JSON used by the 案件管理 screen (HrhPerformanceCollection)
  indeed_cost_last4w, indeed_applications_last4w, indeed_cpa_last4w (empty when applications are 0),
  indeed_cost_last_week, indeed_last_week_start, indeed_cost_total, jobcopy_app_url
"Last 4 weeks" = the 4 weeks ending with the latest imported week of that Listing.

Default is a dry run (nothing is written). Use --apply to write. Output is counts only: no names.

  HUBSPOT_ACCESS_TOKEN=... python scripts/indeed_weekly_import.py --dir <folder> --limit 3
"""
import argparse
import collections
import csv
import datetime
import json
import os
import re
import sys
import time
from pathlib import Path



HUBSPOT = 'https://api.hubapi.com'
LISTINGS = '0-420'
PERF = 'indeed_weekly_perf'
SUMMARY = ['indeed_cost_last4w', 'indeed_applications_last4w', 'indeed_cpa_last4w', 'indeed_cost_last_week',
           'indeed_last_week_start', 'indeed_cost_total', 'jobcopy_app_url']
READ_PROPERTIES = ['id_hrhakkaa', PERF] + SUMMARY
APP_URL = 'https://hr-hw.onrender.com/app/job-copy?listing='
DEFAULT_DIR = '/Users/s_fujimaki/indeed-exports/hrhacker-weekly'
FILE_RE = re.compile(r'^JobsCampaigns_(\d{8})_(\d{8})\.csv$')
HUBSPOT_INTERVAL = 0.5  # at most 2 requests per second
BATCH = 100


# ---- CSV parsing -------------------------------------------------------------------------------

def parse_number(text):
    """'-' / '' -> None; '¥1,234' -> 1234; '12' -> 12; '1.5' -> 1.5."""
    value = (text or '').strip().replace('¥', '').replace('￥', '').replace(',', '')
    if value in ('', '-'):
        return None
    try:
        number = float(value)
    except ValueError:
        return None
    return int(number) if number == int(number) else number


def add_known(a, b):
    """Sum where None means missing; None only if both are missing."""
    if a is None:
        return b
    if b is None:
        return a
    return a + b


def week_of(name):
    match = FILE_RE.match(name)
    if not match:
        return None
    try:
        start = datetime.datetime.strptime(match.group(1), '%Y%m%d').date()
        end = datetime.datetime.strptime(match.group(2), '%Y%m%d').date()
    except ValueError:
        return None
    if start > end:
        return None
    return start.isoformat(), end.isoformat()


def read_week(path):
    """One file -> {ref: {impressions, clicks, cost_yen, applications}} summed over campaigns."""
    totals = {}
    with open(path, encoding='utf-8-sig', newline='') as handle:
        for row in csv.DictReader(handle):
            ref = (row.get('参照番号') or '').strip()
            if not re.fullmatch(r'\d{7,8}', ref):
                continue
            item = totals.setdefault(ref, {'impressions': None, 'clicks': None, 'cost_yen': None, 'applications': None})
            item['impressions'] = add_known(item['impressions'], parse_number(row.get('表示回数')))
            item['clicks'] = add_known(item['clicks'], parse_number(row.get('クリック数')))
            item['cost_yen'] = add_known(item['cost_yen'], parse_number(row.get('費用')))
            item['applications'] = add_known(item['applications'], parse_number(row.get('応募数')))
    return totals


def load_dir(directory):
    """-> ({ref: {period_start: row}}, file_count). Only weeks with impressions > 0 or cost > 0 are kept."""
    weeks = {}
    files = 0
    for path in sorted(Path(directory).iterdir()):
        week = week_of(path.name)
        if week is None:
            continue
        files += 1
        for ref, item in read_week(path).items():
            if not ((item['impressions'] or 0) > 0 or (item['cost_yen'] or 0) > 0):
                continue
            weeks.setdefault(ref, {})[week[0]] = {'period_start': week[0], 'period_end': week[1], **item}
    return weeks, files


# ---- merge and summary -------------------------------------------------------------------------

def parse_existing(raw):
    """The stored JSON, or None when empty or not usable (it is then rebuilt from the CSV files)."""
    if not raw or not raw.strip():
        return None
    try:
        data = json.loads(raw)
    except ValueError:
        return None
    if not isinstance(data, dict) or not isinstance(data.get('rows'), list):
        return None
    return data


def merge_rows(existing, new_rows):
    """Same week is replaced by the new row; result is sorted by week."""
    by_week = {}
    for row in (existing or {}).get('rows', []):
        if isinstance(row, dict) and isinstance(row.get('period_start'), str):
            by_week[row['period_start']] = row
    for row in new_rows:
        by_week[row['period_start']] = row
    return [by_week[key] for key in sorted(by_week)]


def build_collection(ref, rows, captured_at, existing):
    """Keep the old captured_at when nothing changed so an unchanged re-import writes nothing."""
    if existing and existing.get('rows') == rows and existing.get('job_id') == ref:
        captured_at = existing.get('captured_at', captured_at)
    return {'schema_version': 1, 'source': 'hrhacker', 'job_id': ref, 'captured_at': captured_at, 'rows': rows}


def summarize(rows):
    """Human summary values from the stored rows. cpa is '' when applications are 0."""
    if not rows:
        return None
    latest = max(row['period_start'] for row in rows)
    cutoff = (datetime.date.fromisoformat(latest) - datetime.timedelta(days=21)).isoformat()
    last4 = [row for row in rows if row['period_start'] >= cutoff]
    cost4 = sum(row.get('cost_yen') or 0 for row in last4)
    apps4 = sum(row.get('applications') or 0 for row in last4)
    last = next(row for row in rows if row['period_start'] == latest)
    return {
        'indeed_cost_last4w': cost4,
        'indeed_applications_last4w': apps4,
        'indeed_cpa_last4w': round(cost4 / apps4) if apps4 > 0 else '',
        'indeed_cost_last_week': last.get('cost_yen') or 0,
        'indeed_last_week_start': latest,
        'indeed_cost_total': sum(row.get('cost_yen') or 0 for row in rows),
    }


def norm(value):
    """Comparable form of a HubSpot property value (numbers, dates and strings)."""
    if value is None:
        return ''
    text = str(value).strip()
    if re.fullmatch(r'\d{4}-\d{2}-\d{2}.*', text):
        return text[:10]
    if re.fullmatch(r'\d{12,}', text):  # date stored as epoch milliseconds
        return datetime.datetime.fromtimestamp(int(text) / 1000, datetime.timezone.utc).date().isoformat()
    try:
        number = float(text)
        return str(int(number)) if number == int(number) else str(number)
    except ValueError:
        return text


def plan_listing(listing_id, ref, props, new_rows, captured_at):
    """-> (properties to write or None when unchanged, rows)."""
    existing = parse_existing(props.get(PERF))
    rows = merge_rows(existing, new_rows)
    collection = build_collection(ref, rows, captured_at, existing)
    wanted = {PERF: json.dumps(collection, ensure_ascii=False, separators=(',', ':')),
              **summarize(rows), 'jobcopy_app_url': APP_URL + str(listing_id)}
    same = norm(props.get(PERF)) == norm(wanted[PERF]) and all(norm(props.get(k)) == norm(wanted[k]) for k in SUMMARY)
    return (None if same else wanted), rows


# ---- HubSpot -----------------------------------------------------------------------------------

class HubSpotClient:
    def __init__(self, token, session=None, sleep=time.sleep, clock=time.monotonic):
        import requests
        self.session = session or requests.Session()
        self.session.trust_env = False
        self.headers = {'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'}
        self.sleep, self.clock, self.last = sleep, clock, None

    def call(self, method, path, body=None, attempts=6):
        for attempt in range(attempts):
            if self.last is not None:
                wait = HUBSPOT_INTERVAL - (self.clock() - self.last)
                if wait > 0:
                    self.sleep(wait)
            self.last = self.clock()
            response = self.session.request(method, HUBSPOT + path, headers=self.headers, json=body, timeout=60)
            if response.status_code == 429:
                try:
                    delay = float(response.headers.get('Retry-After', '1'))
                except ValueError:
                    delay = 1.0
                self.sleep(max(delay, 1.0))
                continue
            if response.status_code >= 500 and attempt < attempts - 1:
                self.sleep(2.0)
                continue
            return response
        raise RuntimeError('hubspot_rate_limited')

    def find_by_refs(self, refs):
        """Listings whose id_hrhakkaa is in refs (100 per request)."""
        found = []
        for index in range(0, len(refs), BATCH):
            chunk = refs[index:index + BATCH]
            after = None
            while True:
                body = {'filterGroups': [{'filters': [{'propertyName': 'id_hrhakkaa', 'operator': 'IN', 'values': chunk}]}],
                        'properties': READ_PROPERTIES, 'limit': 100}
                if after:
                    body['after'] = after
                response = self.call('POST', f'/crm/v3/objects/{LISTINGS}/search', body)
                if response.status_code != 200:
                    raise RuntimeError('hubspot_search_failed_%d' % response.status_code)
                data = response.json()
                found.extend(data.get('results', []))
                after = ((data.get('paging') or {}).get('next') or {}).get('after')
                if not after:
                    break
        return found

    def batch_update(self, inputs):
        response = self.call('POST', f'/crm/v3/objects/{LISTINGS}/batch/update', {'inputs': inputs})
        if response.status_code == 207:
            return not response.json().get('errors')
        return response.status_code in (200, 201, 204)


def run(hubspot, directory, apply=False, only_refs=None, limit=None, now=None):
    weeks, files = load_dir(directory)
    if only_refs:
        keep = set(only_refs)
        weeks = {ref: value for ref, value in weeks.items() if ref in keep}
    refs = sorted(weeks, key=int)
    captured_at = (now or datetime.datetime.now(datetime.timezone.utc)).strftime('%Y-%m-%dT%H:%M:%SZ')
    stats = collections.Counter(files=files, refs=len(refs))
    errors = collections.Counter()
    by_ref = collections.defaultdict(list)
    for record in hubspot.find_by_refs(refs):
        ref = ((record.get('properties') or {}).get('id_hrhakkaa') or '').strip()
        if ref in weeks:
            by_ref[ref].append(record)
    pending = []
    for ref in refs:
        records = by_ref.get(ref, [])
        if not records:
            stats['unmatched'] += 1
            continue
        if len(records) > 1:
            errors['ambiguous_ref'] += 1
            continue
        stats['matched'] += 1
        record = records[0]
        wanted, _ = plan_listing(record['id'], ref, record.get('properties') or {}, list(weeks[ref].values()), captured_at)
        if wanted is None:
            stats['skipped_same'] += 1
            continue
        if limit is not None and stats['to_write'] >= limit:
            continue
        stats['to_write'] += 1
        pending.append({'id': record['id'], 'properties': wanted})
    if apply:
        for index in range(0, len(pending), BATCH):
            chunk = pending[index:index + BATCH]
            if hubspot.batch_update(chunk):
                stats['written'] += len(chunk)
            else:
                errors['hubspot_write_failed'] += len(chunk)
    return {'mode': 'apply' if apply else 'dry-run', 'files': stats['files'], 'refs': stats['refs'],
            'matched': stats['matched'], 'unmatched': stats['unmatched'], 'to_write': stats['to_write'],
            'written': stats['written'], 'skipped_same': stats['skipped_same'], 'errors': dict(errors),
            'listing_ids': [item['id'] for item in pending] if limit is not None else None}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--dir', default=DEFAULT_DIR, help='folder with the weekly CSV files')
    parser.add_argument('--apply', action='store_true', help='write to HubSpot (default: dry run)')
    parser.add_argument('--only-ref', action='append', default=[], help='process only this 参照番号 (repeatable)')
    parser.add_argument('--limit', type=int, help='write at most N listings (trial run)')
    args = parser.parse_args(argv)
    token = os.environ.get('HUBSPOT_ACCESS_TOKEN', '').strip()
    if not token:
        print(json.dumps({'error': 'missing_hubspot_token'}))
        return 1
    try:
        summary = run(HubSpotClient(token), args.dir, apply=args.apply, only_refs=args.only_ref or None, limit=args.limit)
    except RuntimeError as error:
        print(json.dumps({'error': str(error)}))
        return 1
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == '__main__':
    sys.exit(main())
