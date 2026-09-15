import { useEffect, useState } from 'react';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'ledgerscope:theme';

export function initialTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'dark' || stored === 'light') return stored;
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

export function useTheme(): [Theme, (next: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(initialTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  return [theme, setTheme];
}

export interface VizPalette {
  surface: string;
  plane: string;
  ink: string;
  inkSecondary: string;
  inkMuted: string;
  grid: string;
  eoa: string;
  contract: string;
  focus: string;
  token: string;
  edge: string;
}

const ROLES: Record<keyof VizPalette, string> = {
  surface: '--surface-1',
  plane: '--plane',
  ink: '--text-primary',
  inkSecondary: '--text-secondary',
  inkMuted: '--text-muted',
  grid: '--gridline',
  eoa: '--series-eoa',
  contract: '--series-contract',
  focus: '--series-focus',
  token: '--series-token',
  edge: '--edge',
};

export function readPalette(): VizPalette {
  const styles = getComputedStyle(document.documentElement);
  const out = {} as VizPalette;
  for (const [role, variable] of Object.entries(ROLES) as Array<
    [keyof VizPalette, string]
  >) {
    out[role] = styles.getPropertyValue(variable).trim() || '#808080';
  }
  return out;
}

export function withAlpha(color: string, alpha: number): string {
  const hex = color.trim();
  if (!hex.startsWith('#')) return hex;
  const digits =
    hex.length === 4
      ? hex
          .slice(1)
          .split('')
          .map((c) => c + c)
          .join('')
      : hex.slice(1, 7);
  const value = Number.parseInt(digits, 16);
  if (Number.isNaN(value)) return hex;
  const r = (value >> 16) & 255;
  const g = (value >> 8) & 255;
  const b = value & 255;
  return `rgba(${r}, ${g}, ${b}, ${Math.max(0, Math.min(1, alpha))})`;
}
