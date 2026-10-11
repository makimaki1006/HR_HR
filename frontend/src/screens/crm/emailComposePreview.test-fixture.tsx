// Local E2E entry only; never included in production bundles.
import { createRoot } from 'react-dom/client';
import { CallQueueScreen } from './CallQueueScreen';
import { allowFakeGmail, fakeEmailApi } from './emailApi';
allowFakeGmail();
const send=fakeEmailApi.send;
fakeEmailApi.send=async req => {
  await new Promise(resolve => { setTimeout(resolve,15_000); });
  return send(req);
};
const root=document.getElementById('app-root');
if (!root) throw new Error('compose preview root missing');
createRoot(root).render(<CallQueueScreen initialSearch="?mode=fixture" />);
