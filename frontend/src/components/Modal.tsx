import { useEffect, useRef, type ReactNode } from 'react';

import { IconClose } from './Icons';
import { Button, Panel, cx } from './ui';

interface Props {
  title: string;
  subtitle?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: 'sm' | 'md' | 'lg';
}

const widths = {
  sm: 'max-w-[400px]',
  md: 'max-w-[520px]',
  lg: 'max-w-[720px]',
};

export function Modal({ title, subtitle, onClose, children, footer, width = 'md' }: Props) {
  const panel = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);

  useEffect(() => {
    const first = panel.current?.querySelector<HTMLElement>('[data-autofocus], input, select, textarea');
    first?.focus();
  }, []);

  return (
    <div
      className="fixed inset-0 z-50 grid place-items-center overflow-y-auto bg-[color-mix(in_srgb,var(--plane)_72%,transparent)] p-4 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onPointerDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div ref={panel} className={cx('w-full', widths[width])}>
        <Panel className="flex max-h-[calc(100vh-32px)] w-full flex-col bg-surface-1 shadow-pop">
          <header className="flex items-start gap-3 border-b border-hairline px-5 pb-3 pt-4">
            <div className="min-w-0 flex-1">
              <h2 className="m-0 text-base font-semibold">{title}</h2>
              {subtitle && <div className="mt-1 text-xs text-text-muted">{subtitle}</div>}
            </div>
            <Button variant="ghost" icon className="-mr-2 -mt-1" onClick={onClose} aria-label="Close">
              <IconClose />
            </Button>
          </header>
          <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">{children}</div>
          {footer && <footer className="flex justify-end gap-2 border-t border-hairline px-5 py-3">{footer}</footer>}
        </Panel>
      </div>
    </div>
  );
}

export function FormError({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <p className="m-0 rounded-ui-sm border border-[color-mix(in_srgb,var(--critical)_35%,transparent)] bg-[color-mix(in_srgb,var(--critical)_8%,transparent)] px-3 py-2 text-xs text-critical" role="alert">
      {message}
    </p>
  );
}

export function errorText(cause: unknown): string {
  return cause instanceof Error ? cause.message : String(cause);
}
