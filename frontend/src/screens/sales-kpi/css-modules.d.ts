// CSS の side-effect import (entries/sales-kpi.tsx → sales-kpi.css) を tsc に通すための宣言。
// tsconfig.app.json の `types: []` に vite/client を足さずに済ませる (共有設定を触らない)。
// platform-team が vite-env.d.ts を入れたら、この宣言は重複しても害は無い (ambient module は merge される)。
declare module '*.css';
