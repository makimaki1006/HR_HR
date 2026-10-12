import {createRoot} from 'react-dom/client';
import {AdminScreen} from './AdminScreen';
import './w8.css';
const root=document.getElementById('root');if(!root)throw new Error('preview root missing');
createRoot(root).render(<AdminScreen/>);
