"""Synthetic checks for the AirWork image URL writer. HTTP is mocked; no external request."""
import json
import unittest
from unittest.mock import patch

import job_copy_airwork_images as collector
import job_copy_airwork_image_urls as writer

SLUG_A = 'https://arwrk.net/recruit/synthetic-a'
CDN = 'https://cdn.arwrk.net/images/rct/1/2/'


def gallery(images, nuxt=''):
    return (f'<html><head><link rel="canonical" href="{{canonical}}"></head><body><div class="job-img"><ul class="slider">'
            + ''.join(f'<li><img src="{url}"></li>' for url in images)
            + f'</ul><div class="thumbs"><img src="{CDN}t?thumb"></div></div>{nuxt}</body></html>')


def page(url, images, nuxt=''):
    return gallery(images, nuxt).replace('{canonical}', url).encode()


def nuxt_empty_page(url, images_literal='[]', arguments='[]'):
    script = ('<script>window.__NUXT__=(function(a,b){return {data:[{recruitData:{jobOverViewSection:'
              f'{{publishedAt:"2026",images:{images_literal}}}}}]}}}}({arguments},1));</script>')
    return ('<html><head><link rel="canonical" href="' + url + '"></head><body><h1>x</h1>' + script + '</body></html>').encode()


class Response:
    def __init__(self, raw=b'', status=200, mime='text/html'):
        self.raw, self.status_code, self.headers = raw, status, {'Content-Type': mime}

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def iter_content(self, size):
        yield self.raw


class Pages:
    def __init__(self, answers):
        self.answers, self.calls = answers, []

    def get(self, url, **_):
        self.calls.append(url)
        return self.answers[url]


class FakeHubSpot:
    def __init__(self, targets, everything=None):
        self.targets, self.everything, self.writes, self.searches = targets, everything or [], [], 0

    def search_all(self, filters, properties, limit=None):
        names = {f['propertyName'] for f in filters}
        if 'kyuujin_status' in names:
            rows = self.targets
            for f in filters:
                if f['propertyName'] == 'hs_object_id':
                    rows = [r for r in rows if r['id'] == f['value']]
            return iter(rows[:limit] if limit else rows)
        self.searches += 1
        return iter(self.everything)

    def batch_update(self, inputs):
        self.writes.append(inputs)
        return True


def listing(id_, media, url=None, account='acct1', current=None):
    props = {'id_airwork': media, 'airwork_account_login_id': account}
    if url:
        props['url_airwork'] = url
    if current is not None:
        props[writer.PROPERTY] = current
    return {'id': id_, 'properties': props}


def execute(hubspot, answers, **options):
    session = Pages(answers)
    summary = writer.run(hubspot, session, sleep=lambda _s: None, clock=lambda: 0.0, log=lambda _m: None, **options)
    return summary, session


class SiteDecision(unittest.TestCase):
    def test_own_url_is_strictly_validated_and_must_match_the_id(self):
        self.assertEqual(writer.own_site(SLUG_A + '/55/', '55'), SLUG_A)
        for bad in (SLUG_A + '/56/', SLUG_A + '/55/?x=1', 'http://arwrk.net/recruit/a/55/', 'https://arwrk.net.evil/recruit/a/55/',
                    'https://arwrk.net/recruit/a/b/55/', 'https://arwrk.net/other/a/55/', None, ''):
            self.assertIsNone(writer.own_site(bad, '55'), bad)

    def test_account_slug_comes_from_any_status_and_conflicts_are_excluded(self):
        accounts = writer.account_sites([
            listing('1', '10', 'https://arwrk.net/recruit/slug-one/10/', 'acct1'),
            listing('2', '11', 'https://arwrk.net/recruit/slug-one/11/', 'acct1'),
            listing('3', '12', 'https://arwrk.net/recruit/slug-x/12/', 'acct2'),
            listing('4', '13', 'https://arwrk.net/recruit/slug-y/13/', 'acct2'),
            listing('5', '14', None, 'acct3'),
        ])
        self.assertEqual(accounts, {'acct1': 'https://arwrk.net/recruit/slug-one', 'acct2': None})
        props = {'id_airwork': '99', 'airwork_account_login_id': 'acct1'}
        self.assertEqual(writer.decide_site(props, accounts), ('https://arwrk.net/recruit/slug-one', 'account'))
        self.assertEqual(writer.decide_site({**props, 'airwork_account_login_id': 'acct2'}, accounts), (None, 'conflict'))
        self.assertEqual(writer.decide_site({**props, 'airwork_account_login_id': 'acct3'}, accounts), (None, 'unknown'))
        own = {**props, 'url_airwork': 'https://arwrk.net/recruit/mine/99/'}
        self.assertEqual(writer.decide_site(own, accounts), ('https://arwrk.net/recruit/mine', 'own'))

    def test_values_drop_thumb_queries_keep_order_and_compare_by_lines(self):
        urls = writer.clean_urls([CDN + 'b?thumb', CDN + 'a'])
        self.assertEqual(urls, [CDN + 'b', CDN + 'a'])
        self.assertEqual(writer.field_value(urls), CDN + 'b\n' + CDN + 'a')
        self.assertEqual(writer.field_value([]), 'NO_IMAGES')
        self.assertTrue(writer.same_value('NO_IMAGES\r\n', 'NO_IMAGES'))
        self.assertTrue(writer.same_value(f' {CDN}a \n\n{CDN}b', f'{CDN}a\n{CDN}b'))
        self.assertFalse(writer.same_value(f'{CDN}b\n{CDN}a', f'{CDN}a\n{CDN}b'))
        self.assertFalse(writer.same_value(None, 'NO_IMAGES'))


class ZeroPhotoPages(unittest.TestCase):
    URL = SLUG_A + '/7/'

    def test_empty_embedded_images_means_zero_photos_not_unknown_layout(self):
        self.assertEqual(collector.parse_gallery(nuxt_empty_page(self.URL), self.URL), [])
        self.assertEqual(collector.parse_gallery(nuxt_empty_page(self.URL, 'a', '[]'), self.URL), [])

    def test_non_empty_or_unreadable_embedded_images_stay_unrecognized(self):
        for raw in (nuxt_empty_page(self.URL, '["https://cdn.arwrk.net/images/rct/1/2/x"]'),
                    nuxt_empty_page(self.URL, 'a', '"x"'),
                    nuxt_empty_page(self.URL, 'zz', '[]'),
                    b'<html><head><link rel="canonical" href="' + self.URL.encode() + b'"></head><body>no data</body></html>'):
            with self.assertRaisesRegex(collector.CaptureError, 'gallery_layout_unrecognized'):
                collector.parse_gallery(raw, self.URL)

    def test_other_canonical_still_fails_even_with_empty_images(self):
        with self.assertRaisesRegex(collector.CaptureError, 'page_identity_mismatch'):
            collector.parse_gallery(nuxt_empty_page(SLUG_A + '/8/'), self.URL)


class RunTests(unittest.TestCase):
    def test_dry_run_counts_but_never_writes_and_reads_pages_in_order(self):
        urls = [CDN + str(i) for i in range(1, 6)]
        url = SLUG_A + '/1/'
        hubspot = FakeHubSpot([listing('100', '1', url)])
        summary, session = execute(hubspot, {url: Response(page(url, urls))})
        self.assertEqual(hubspot.writes, [])
        self.assertEqual((summary['mode'], summary['targets'], summary['to_write'], summary['written']), ('dry-run', 1, 1, 0))
        self.assertEqual(summary['url_source'], {'own': 1, 'account': 0, 'conflict': 0, 'unknown': 0})
        self.assertEqual(session.calls, [url])

    def test_apply_writes_urls_in_order_zero_marker_and_skips_same_value(self):
        u1, u2, u3 = SLUG_A + '/1/', SLUG_A + '/2/', SLUG_A + '/3/'
        order = [CDN + 'e', CDN + 'd', CDN + 'c', CDN + 'b', CDN + 'a', CDN + 'z']
        hubspot = FakeHubSpot([
            listing('100', '1', u1),
            listing('101', '2', u2),
            listing('102', '3', u3, current='\n'.join([CDN + 'x'])),
            listing('103', '4', SLUG_A + '/4/', current='NO_IMAGES'),
        ])
        answers = {u1: Response(page(u1, order)), u2: Response(nuxt_empty_page(u2)),
                   u3: Response(page(u3, [CDN + 'x?thumb'])), SLUG_A + '/4/': Response(nuxt_empty_page(SLUG_A + '/4/'))}
        summary, _ = execute(hubspot, answers, apply=True)
        self.assertEqual(len(hubspot.writes), 1)
        written = {item['id']: item['properties'][writer.PROPERTY] for item in hubspot.writes[0]}
        self.assertEqual(written, {'100': '\n'.join(order), '101': 'NO_IMAGES'})
        self.assertEqual((summary['written'], summary['skipped_same'], summary['zero_images'], summary['targets']), (2, 2, 2, 4))

    def test_404_410_and_other_errors_are_never_written(self):
        base = SLUG_A
        hubspot = FakeHubSpot([listing(str(100 + i), str(i), f'{base}/{i}/') for i in (1, 2, 3, 4)])
        answers = {f'{base}/1/': Response(status=404), f'{base}/2/': Response(status=410),
                   f'{base}/3/': Response(status=500), f'{base}/4/': Response(page(f'{base}/4/', []).replace(b'job-img', b'zzz'))}
        summary, _ = execute(hubspot, answers, apply=True)
        self.assertEqual(hubspot.writes, [])
        self.assertEqual(summary['page_missing'], 2)
        self.assertEqual(summary['errors'], {'upstream_http_failure': 1, 'gallery_layout_unrecognized': 1})
        self.assertEqual(summary['written'], 0)

    def test_missing_own_url_uses_the_account_slug_from_ended_listings_or_is_unknown(self):
        everything = [listing('1', '50', 'https://arwrk.net/recruit/ended-slug/50/', 'acct1'),
                      listing('2', '51', 'https://arwrk.net/recruit/slug-p/51/', 'acct9'),
                      listing('3', '52', 'https://arwrk.net/recruit/slug-q/52/', 'acct9')]
        target_url = 'https://arwrk.net/recruit/ended-slug/60/'
        hubspot = FakeHubSpot([listing('100', '60', None, 'acct1'), listing('101', '61', None, 'acct9'),
                               listing('102', '62', None, 'unseen')], everything)
        summary, session = execute(hubspot, {target_url: Response(page(target_url, [CDN + 'a']))}, apply=True)
        self.assertEqual(session.calls, [target_url])
        self.assertEqual(summary['url_source'], {'own': 0, 'account': 1, 'conflict': 1, 'unknown': 1})
        self.assertEqual(summary['site_unknown'], 2)
        self.assertEqual(hubspot.searches, 1)
        self.assertEqual([item['id'] for item in hubspot.writes[0]], ['100'])

    def test_limit_listing_id_and_batches_of_one_hundred(self):
        targets = [listing(str(1000 + i), str(i), f'{SLUG_A}/{i}/') for i in range(1, 251)]
        answers = {f'{SLUG_A}/{i}/': Response(page(f'{SLUG_A}/{i}/', [CDN + str(i)])) for i in range(1, 251)}
        hubspot = FakeHubSpot(targets)
        summary, session = execute(hubspot, answers, apply=True)
        self.assertEqual([len(batch) for batch in hubspot.writes], [100, 100, 50])
        self.assertEqual(summary['written'], 250)
        small = FakeHubSpot(targets)
        summary, session = execute(small, answers, limit=5)
        self.assertEqual((summary['targets'], len(session.calls), small.writes), (5, 5, []))
        one = FakeHubSpot(targets)
        summary, session = execute(one, answers, listing_id='1007', apply=True)
        self.assertEqual((summary['targets'], summary['written']), (1, 1))
        self.assertEqual(one.writes[0][0]['id'], '1007')

    def test_summary_contains_no_identifiers_or_urls(self):
        url = SLUG_A + '/1/'
        summary, _ = execute(FakeHubSpot([listing('100', '1', url)]), {url: Response(page(url, [CDN + 'a']))})
        text = json.dumps(summary)
        for secret in ('arwrk', 'synthetic-a', '100', 'acct1'):
            self.assertNotIn(secret, text.replace('"targets": 1', ''))


class HubSpotClient(unittest.TestCase):
    class Http:
        def __init__(self, statuses):
            self.statuses, self.calls, self.trust_env = list(statuses), [], True

        def request(self, method, url, **options):
            self.calls.append((method, url))
            status, headers = self.statuses.pop(0)

            class Out:
                status_code = status

                def __init__(self):
                    self.headers = headers

                def json(self):
                    return {'results': []}
            return Out()

    def test_429_waits_for_retry_after_and_retries(self):
        sleeps = []
        http = self.Http([(429, {'Retry-After': '7'}), (200, {})])
        client = writer.HubSpot('t', http, sleep=sleeps.append, clock=lambda: 0.0)
        self.assertEqual(client.call('POST', '/x').status_code, 200)
        self.assertEqual(len(http.calls), 2)
        self.assertIn(7.0, sleeps)

    def test_search_uses_the_id_cursor_and_stops_on_short_page(self):
        pages = [{'results': [{'id': str(i)} for i in range(1, 101)]}, {'results': [{'id': '101'}]}]
        bodies = []

        class Out:
            status_code = 200

            def __init__(self, data):
                self.data = data

            def json(self):
                return self.data
        client = writer.HubSpot('t', self.Http([]), sleep=lambda _s: None, clock=lambda: 0.0)

        def call(method, path, body=None, attempts=6):
            bodies.append(body)
            return Out(pages.pop(0))
        with patch.object(client, 'call', call):
            ids = [r['id'] for r in client.search_all([], ['id_airwork'])]
        self.assertEqual(len(ids), 101)
        self.assertEqual(bodies[0]['filterGroups'][0]['filters'][-1]['value'], '0')
        self.assertEqual(bodies[1]['filterGroups'][0]['filters'][-1]['value'], '100')


if __name__ == '__main__':
    unittest.main()
