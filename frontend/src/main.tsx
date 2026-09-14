import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import App from './App';
import { initialTheme } from './lib/theme';
import './styles.css';

document.documentElement.dataset.theme = initialTheme();

const container = document.getElementById('root');
if (!container) throw new Error('#root is missing from index.html');

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
