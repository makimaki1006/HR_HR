"""Capture referenced media images privately; never print URLs or job content."""
import base64
import hashlib
import io
import json
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import urlparse

import requests
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parents[1] / 'data' / 'job-copy-local'
ALLOWED = {'s3-ap-northeast-1.amazonaws.com'}
MAX_BYTES = 8 * 1024 * 1024


def capture(url):
    try:
        reference_hash = hashlib.sha256(url.encode('utf-8')).hexdigest()
        if urlparse(url).hostname not in ALLOWED or urlparse(url).scheme not in {'http', 'https'}:
            return None
        url = url.replace('http://', 'https://', 1)
        with requests.get(url, timeout=(10, 30), stream=True, allow_redirects=False) as response:
            response.raise_for_status()
            raw = bytearray()
            for chunk in response.iter_content(65536):
                raw.extend(chunk)
                if len(raw) > MAX_BYTES:
                    return None
        digest = hashlib.sha256(raw).hexdigest()
        with Image.open(io.BytesIO(raw)) as source:
            if source.format not in {'JPEG', 'PNG', 'WEBP'} or source.width * source.height > 30_000_000:
                return None
            extension = {'JPEG': 'jpg', 'PNG': 'png', 'WEBP': 'webp'}[source.format]
            (ROOT / 'images' / f'{digest}.{extension}').write_bytes(raw)
            image = ImageOps.exif_transpose(source).convert('RGB')
            image.thumbnail((900, 900))
            output = io.BytesIO()
            image.save(output, format='JPEG', quality=75)
        return {'id': digest, 'contentHash': digest, 'sourceReferenceHash': reference_hash,
                'url': 'data:image/jpeg;base64,' + base64.b64encode(output.getvalue()).decode('ascii')}
    except Exception:
        return None


def main():
    selected = json.loads((ROOT / 'selected.json').read_text(encoding='utf-8'))
    (ROOT / 'images').mkdir(exist_ok=True)
    urls = list(dict.fromkeys(url for job in selected for url in job['originalImages']))
    with ThreadPoolExecutor(max_workers=4) as pool:
        downloaded = dict(zip(urls, pool.map(capture, urls)))
    jobs = []
    for job in selected:
        references = job.pop('originalImages')
        images = [{**downloaded[url], 'id': f"{downloaded[url]['id']}-{index}",
                   'caption': f'掲載画像{index + 1}'} for index, url in enumerate(references) if downloaded[url]]
        job['images'] = images
        job['imageAcquisition'] = {'expected': len(references), 'downloaded': len(images),
                                   'failed': len(references) - len(images)}
        jobs.append(job)
    result = json.loads((ROOT / 'capture-result.json').read_text(encoding='utf-8'))
    captured = datetime.fromtimestamp((ROOT / result['csv_filename']).stat().st_mtime, timezone.utc).isoformat(timespec='milliseconds')
    bundle = {'schemaVersion': 1, 'capturedAt': captured, 'jobs': jobs}
    (ROOT / 'moc-bundle.json').write_text(json.dumps(bundle, ensure_ascii=False), encoding='utf-8')
    summary = {'jobs': len(jobs), 'imageReferences': sum(j['imageAcquisition']['expected'] for j in jobs),
               'downloadedReferences': sum(len(j['images']) for j in jobs),
               'failedReferences': sum(j['imageAcquisition']['failed'] for j in jobs),
               'uniqueImages': sum(value is not None for value in downloaded.values())}
    (ROOT / 'image-result.json').write_text(json.dumps(summary), encoding='utf-8')
    print(json.dumps(summary))


if __name__ == '__main__':
    main()
