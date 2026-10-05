"""Attach normalized HR Hacker metrics to a private review snapshot, offline.

No guessed CSV mapping, HubSpot writes, new database or public data. Normalize an
existing source report to this explicit contract before invoking this operator.
Unmatched IDs are errors; duplicate listing/media IDs cannot cross company scope.
"""
import argparse
from datetime import date, datetime
import json
import math
from pathlib import Path
import re


def validate_collection(value):
    if not isinstance(value, dict) or set(value) != {'schema_version', 'source', 'job_id', 'captured_at', 'rows'}:
        raise ValueError('performance_schema_invalid')
    if type(value['schema_version']) is not int or value['schema_version'] != 1 or value['source'] != 'hrhacker':
        raise ValueError('performance_source_invalid')
    if not isinstance(value['job_id'], str) or not re.fullmatch(r'[0-9]{8}', value['job_id']):
        raise ValueError('performance_job_id_invalid')
    try:
        if not isinstance(value['captured_at'], str) or not re.fullmatch(r'\d{4}-\d{2}-\d{2}T.+(?:Z|[+-]\d{2}:\d{2})', value['captured_at']):
            raise ValueError()
        timestamp = datetime.fromisoformat(value['captured_at'].replace('Z', '+00:00'))
        if timestamp.tzinfo is None:
            raise ValueError()
    except (ValueError, TypeError, AttributeError):
        raise ValueError('performance_timestamp_invalid') from None
    if not isinstance(value['rows'], list) or len(value['rows']) > 1000:
        raise ValueError('performance_rows_invalid')
    periods = []
    for row in value['rows']:
        if not isinstance(row, dict) or set(row) != {'period_start', 'period_end', 'impressions', 'clicks', 'cost_yen', 'applications'}:
            raise ValueError('performance_row_invalid')
        try:
            start, end = (date.fromisoformat(row[key]) for key in ('period_start', 'period_end'))
            if start.isoformat() != row['period_start'] or end.isoformat() != row['period_end'] or start > end:
                raise ValueError()
        except (ValueError, TypeError):
            raise ValueError('performance_period_invalid') from None
        periods.append((start, end))
        for key in ('impressions', 'clicks', 'cost_yen', 'applications'):
            amount = row[key]
            if amount is None:
                continue
            if (type(amount) not in (int, float) or not math.isfinite(amount) or amount < 0
                    or amount > 2**53 - 1 or (key != 'cost_yen' and type(amount) is not int)):
                raise ValueError('performance_number_invalid')
        if row['clicks'] is not None and row['impressions'] is not None and row['clicks'] > row['impressions']:
            raise ValueError('performance_counts_invalid')
    periods.sort()
    if any(current[0] <= previous[1] for previous, current in zip(periods, periods[1:])):
        raise ValueError('performance_period_overlap')
    return value


def attach(snapshot, collections):
    if not isinstance(collections, list) or len(collections) > 10000:
        raise ValueError('performance_input_invalid')
    # Work on a copy, publish only after every row matches exactly once.
    result = json.loads(json.dumps(snapshot, allow_nan=False))
    if result.get('schemaVersion') != 1 or not isinstance(result.get('results'), list):
        raise ValueError('snapshot_invalid')
    jobs = result['capture_bundle']['jobs']
    mapping = {}
    for job in jobs:
        if job['media'] == 'HRハッカー':
            mapping.setdefault(job['mediaJobId'], []).append(job['hubspotListingId'])
    results = {row['listing_id']: row for row in result['results']}
    if len(results) != len(result['results']):
        raise ValueError('snapshot_listing_duplicate')
    seen = set()
    for raw in collections:
        data = validate_collection(raw)
        job_id = data['job_id']
        if job_id in seen:
            raise ValueError('performance_job_duplicate')
        seen.add(job_id)
        matches = mapping.get(job_id, [])
        if len(matches) != 1 or matches[0] not in results:
            raise ValueError('performance_job_unmatched_or_ambiguous')
        if 'hrh_performance' in results[matches[0]]:
            raise ValueError('performance_already_attached')
        results[matches[0]]['hrh_performance'] = data
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--snapshot', required=True)
    parser.add_argument('--performance', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    try:
        output = Path(args.output)
        if 'job-copy-local' not in output.resolve().parts or output.exists():
            raise ValueError('private_new_output_required')
        data = attach(json.loads(Path(args.snapshot).read_text(encoding='utf-8')),
                      json.loads(Path(args.performance).read_text(encoding='utf-8')))
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open('x', encoding='utf-8') as target:
            json.dump(data, target, ensure_ascii=False, allow_nan=False)
        print(json.dumps({'ok': True, 'matched_jobs': sum('hrh_performance' in row for row in data['results'])}))
    except (OSError, ValueError, KeyError, TypeError):
        parser.exit(1, 'Performance import rejected; check private source schema, IDs and periods.\n')
