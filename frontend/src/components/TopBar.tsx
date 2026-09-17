import type { Theme } from '../lib/theme';
import { IconLogo, IconMoon, IconSun } from './Icons';

interface Props {
  theme: Theme;
  onThemeChange: (next: Theme) => void;
}

export function TopBar({ theme, onThemeChange }: Props) {
  return (
    <header className="topbar">
      <div className="brand">
        <IconLogo className="brand-mark" />
        <span>Ledgerscope</span>
        <span className="brand-sub">ETH transaction graph</span>
      </div>

      <div className="topbar-spacer" />

      <button
        type="button"
        className="btn btn-ghost btn-icon"
        onClick={() => onThemeChange(theme === 'dark' ? 'light' : 'dark')}
        title={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
        aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
      >
        {theme === 'dark' ? <IconSun /> : <IconMoon />}
      </button>
    </header>
  );
}
