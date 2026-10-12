import {createRoot} from 'react-dom/client';
import {CallQueueScreen} from './CallQueueScreen';
const root=document.getElementById('root');if(!root)throw new Error('preview root missing');
createRoot(root).render(<CallQueueScreen/>);
