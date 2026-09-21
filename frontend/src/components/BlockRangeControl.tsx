import { useEffect, useRef } from 'react';

import { formatCount } from '../lib/format';
import { IconChevron } from './Icons';
import { Button, Field, FieldLabel, Input, Panel, cx } from './ui';

interface Props {
  from: string;
  to: string;
  invalid: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onFromChange: (value: string) => void;
  onToChange: (value: string) => void;
}

function blockLabel(value: string): string {
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? formatCount(parsed) : '—';
}

export function BlockRangeControl({
  from,
  to,
  invalid,
  open,
  onOpenChange,
  onFromChange,
  onToChange,
}: Props) {
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) onOpenChange(false);
    };

    document.addEventListener('pointerdown', onPointerDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
    };
  }, [onOpenChange, open]);

  const digitsOnly = (value: string) => value.replace(/\D/g, '');

  return (
    <div ref={root} className="relative flex-none">
      <Button
        variant="ghost"
        className={cx(
          'h-8 gap-1.5 px-2.5 text-xs font-medium text-text-muted',
          invalid && 'text-critical',
        )}
        onClick={() => onOpenChange(!open)}
        aria-expanded={open}
        aria-controls="block-range-panel"
      >
        <span>Advanced</span>
        <IconChevron className={cx('transition-transform duration-150', open && 'rotate-180')} />
      </Button>

      {open && (
        <Panel
          id="block-range-panel"
          className="absolute left-0 top-[calc(100%+8px)] z-30 w-[300px] animate-drawer-in p-3 shadow-pop max-[620px]:left-auto max-[620px]:right-0"
          aria-label="Block range"
        >
          <div className="mb-3 flex items-center justify-between border-b border-hairline pb-2 text-[11px] text-text-muted">
            <span>Block range</span>
            <span className="tabular-nums text-text-secondary">
              {blockLabel(from)} → {blockLabel(to)}
            </span>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field>
              <FieldLabel htmlFor="q-from">From block</FieldLabel>
              <Input
                id="q-from"
                className="w-full tabular-nums"
                invalid={invalid}
                value={from}
                inputMode="numeric"
                onChange={(event) => onFromChange(digitsOnly(event.target.value))}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="q-to">To block</FieldLabel>
              <Input
                id="q-to"
                className="w-full tabular-nums"
                invalid={invalid}
                value={to}
                inputMode="numeric"
                onChange={(event) => onToChange(digitsOnly(event.target.value))}
              />
            </Field>
          </div>
          <p className="mb-0 mt-2 text-[11px] leading-relaxed text-text-muted">
            The graph includes transactions from this block range.
          </p>
        </Panel>
      )}
    </div>
  );
}
