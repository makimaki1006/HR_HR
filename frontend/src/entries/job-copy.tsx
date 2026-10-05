import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { JobCopyApp } from '../screens/job-copy/JobCopyApp';
import '../styles/app.css';

const root = document.getElementById('app-root');
if (root) createRoot(root).render(<StrictMode><JobCopyApp /></StrictMode>);
