import type { CanvasDocument } from './canvasDocument';

const LAST_CANVAS_KEY = 'ledgerscope:last-canvas:v1';
const SESSION_KEY = 'ledgerscope:session:v1';

interface LastCanvas {
  user: string;
  canvas: string;
}

export function readLastCanvas(user: string): string | null {
  try {
    const stored = localStorage.getItem(LAST_CANVAS_KEY);
    const parsed = stored ? (JSON.parse(stored) as LastCanvas) : null;
    return parsed?.user === user ? parsed.canvas : null;
  } catch {
    return null;
  }
}

export function writeLastCanvas(user: string, canvas: string | null): void {
  try {
    if (canvas) localStorage.setItem(LAST_CANVAS_KEY, JSON.stringify({ user, canvas } satisfies LastCanvas));
    else localStorage.removeItem(LAST_CANVAS_KEY);
  } catch {
    return;
  }
}

export function readSession(): unknown {
  try {
    const stored = localStorage.getItem(SESSION_KEY);
    return stored ? JSON.parse(stored) : null;
  } catch {
    return null;
  }
}

export function writeSession(document: CanvasDocument): void {
  try {
    localStorage.setItem(SESSION_KEY, JSON.stringify(document));
  } catch {
    return;
  }
}
