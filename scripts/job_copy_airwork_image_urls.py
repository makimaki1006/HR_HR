"""Write public AirWork image URLs into the HubSpot Listing field aw_kyuujinhyou_gazou.

Only URLs are collected; no image is downloaded or stored. One URL per line, in display
order. A public page that was read and has no photos gets the single line NO_IMAGES.
Pages that are missing (404/410) or fail for any other reason are never written.

Default is a dry run (nothing is written). Use --apply to PATCH HubSpot.
Output is counts only: no customer names, URLs or IDs are printed.

  HUBSPOT_ACCESS_TOKEN=... python scripts/job_copy_airwork_image_urls.py --dry-run --limit 5
"""
import argparse
import collections
import json
import os
import re
import sys
import time
from urllib.parse import urlsplit

import requests

import job_copy_airwork_images as collector

HUBSPOT = 'https://api.hubapi.com'
LISTINGS = '0-420'
PROPERTY = 'aw_kyuujinhyou_gazou'
NO_IMAGES = 'NO_IMAGES'
READ_PROPERTIES = ['id_airwork', 'url_airwork', 'airwork_account_login_id', PROPERTY]
HUBSPOT_INTERVAL = 0.5  # at most 2 requests per second
PAGE_INTERVAL = 1.0  # about 1 public page per second
BATCH = 100


def own_site(url, media_id):
    """Account site from the listing's own url_airwork, or None if it is not exactly /recruit/<slug>/<id>/."""
    if not isinstance(url, str) or any(ord(c) <= 32 or ord(c) == 127 for c in url):
        return None
    parts = urlsplit(url)
    match = re.fullmatch(r'/recruit/([A-Za-z0-9_-]+)/([0-9]{1,30})/?', parts.path)
    if (parts.scheme != 'https' or parts.netloc != 'arwrk.net' or parts.query or parts.fragment
            or not match or match.group(2) != media_id):
        return None
    return 'https://arwrk.net/recruit/' + match.group(1)


def account_sites(listings):
    """Account login id -> slug, from every listing (ended ones included).

    An account with more than one slug is a conflict and is left out (None); nothing is guessed.
    """
    slugs = collections.defaultdict(set)
    for record in listings:
        props = record.get('properties') or {}
        account = (props.get('airwork_account_login_id') or '').strip()
        media = (props.get('id_airwork') or '').strip()
        site = own_site(props.get('url_airwork'), media)
        if account and site:
            slugs[account].add(site)
    return {account: (next(iter(found)) if len(found) == 1 else None) for account, found in slugs.items()}


def decide_site(props, accounts):
    """(site, source). source is own, account, conflict or unknown."""
    media = (props.get('id_airwork') or '').strip()
    site = own_site(props.get('url_airwork'), media)
    if site:
        return site, 'own'
    account = (props.get('airwork_account_login_id') or '').strip()
    if account in accounts:
        return (accounts[account], 'account') if accounts[account] else (None, 'conflict')
    return None, 'unknown'


def clean_urls(urls):
    """Main image URL only: query strings such as ?thumb are dropped, order is kept."""
    return [urlsplit(url)._replace(query='', fragment='').geturl() for url in urls]


def field_value(urls):
    return '\n'.join(urls) if urls else NO_IMAGES


def same_value(current, new):
    lines = [line.strip() for line in (current or '').replace('\r\n', '\n').split('\n') if line.strip()]
    return lines == new.split('\n')


class HubSpot:
    def __init__(self, token, session=None, sleep=time.sleep, clock=time.monotonic):
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

    def search_all(self, filters, properties, limit=None):
        """Walk every match in id order; the id cursor avoids the 10,000 result search cap."""
        seen, last = 0, 0
        while True:
            body = {'filterGroups': [{'filters': filters + [{'propertyName': 'hs_object_id', 'operator': 'GT', 'value': str(last)}]}],
                    'sorts': [{'propertyName': 'hs_object_id', 'direction': 'ASCENDING'}],
                    'properties': properties, 'limit': 100}
            response = self.call('POST', f'/crm/v3/objects/{LISTINGS}/search', body)
            if response.status_code != 200:
                raise RuntimeError('hubspot_search_failed_%d' % response.status_code)
            results = response.json().get('results', [])
            for record in results:
                yield record
                seen += 1
                if limit is not None and seen >= limit:
                    return
            if len(results) < 100:
                return
            last = int(results[-1]['id'])

    def batch_update(self, inputs):
        response = self.call('POST', f'/crm/v3/objects/{LISTINGS}/batch/update', {'inputs': inputs})
        return response.status_code in (200, 201, 204, 207) and not (response.status_code == 207 and response.json().get('errors'))


PUBLISHED = [{'propertyName': 'kyuujin_status', 'operator': 'EQ', 'value': '公開中'},
             {'propertyName': 'id_airwork', 'operator': 'HAS_PROPERTY'}]


def run(hubspot, session, apply=False, limit=None, listing_id=None, sleep=time.sleep, clock=time.monotonic, log=print):
    stats = collections.Counter()
    errors = collections.Counter()
    accounts = None
    pending = []
    last_page = [None]

    def flush():
        if not pending:
            return
        if hubspot.batch_update(list(pending)):
            stats['written'] += len(pending)
        else:
            errors['hubspot_write_failed'] += len(pending)
        pending.clear()

    filters = list(PUBLISHED)
    if listing_id is not None:
        filters.append({'propertyName': 'hs_object_id', 'operator': 'EQ', 'value': str(int(listing_id))})
    for record in hubspot.search_all(filters, READ_PROPERTIES, limit):
        props = record.get('properties') or {}
        stats['targets'] += 1
        site, source = decide_site(props, accounts or {})
        if source in ('conflict', 'unknown') and accounts is None:
            # Account slugs come from every listing, ended ones included; read them only when needed.
            accounts = account_sites(hubspot.search_all(
                [{'propertyName': 'id_airwork', 'operator': 'HAS_PROPERTY'},
                 {'propertyName': 'url_airwork', 'operator': 'HAS_PROPERTY'}],
                READ_PROPERTIES))
            site, source = decide_site(props, accounts)
        stats['site_' + source] += 1
        if site is None:
            stats['site_unknown_total'] += 1
            continue
        try:
            url = collector.job_url(site, (props.get('id_airwork') or '').strip())
        except collector.CaptureError as error:
            errors[error.code] += 1
            continue
        if last_page[0] is not None:
            wait = PAGE_INTERVAL - (clock() - last_page[0])
            if wait > 0:
                sleep(wait)
        last_page[0] = clock()
        try:
            raw, _ = collector._get(session, url, collector.MAX_PAGE_BYTES, page=True)
            urls = clean_urls(collector.parse_gallery(raw, url))
        except collector.CaptureError as error:
            if error.code == 'public_page_unavailable':
                stats['page_missing'] += 1
            else:
                errors[error.code] += 1
            continue
        if not urls:
            stats['zero_images'] += 1
        value = field_value(urls)
        if same_value(props.get(PROPERTY), value):
            stats['skipped_same'] += 1
            continue
        stats['to_write'] += 1
        if apply:
            pending.append({'id': record['id'], 'properties': {PROPERTY: value}})
            if len(pending) >= BATCH:
                flush()
        if stats['targets'] % 50 == 0:
            log(f"processed {stats['targets']} listings")
    flush()
    summary = {
        'mode': 'apply' if apply else 'dry-run',
        'targets': stats['targets'],
        'url_source': {'own': stats['site_own'], 'account': stats['site_account'],
                       'conflict': stats['site_conflict'], 'unknown': stats['site_unknown']},
        'to_write': stats['to_write'],
        'written': stats['written'],
        'skipped_same': stats['skipped_same'],
        'zero_images': stats['zero_images'],
        'page_missing': stats['page_missing'],
        'site_unknown': stats['site_unknown_total'],
        'errors': dict(errors),
    }
    return summary


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--dry-run', action='store_true', help='default: count only, write nothing')
    mode.add_argument('--apply', action='store_true', help='PATCH HubSpot')
    parser.add_argument('--limit', type=int, help='process at most N listings')
    parser.add_argument('--listing-id', help='process this one HubSpot Listing id')
    args = parser.parse_args(argv)
    token = os.environ.get('HUBSPOT_ACCESS_TOKEN', '').strip()
    if not token:
        print(json.dumps({'error': 'missing_hubspot_token'}))
        return 1
    session = requests.Session()
    session.trust_env = False
    try:
        summary = run(HubSpot(token), session, apply=args.apply, limit=args.limit, listing_id=args.listing_id)
    except RuntimeError as error:
        print(json.dumps({'error': str(error)}))
        return 1
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == '__main__':
    sys.exit(main())
