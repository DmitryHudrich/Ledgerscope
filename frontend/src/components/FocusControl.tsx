import { useEffect, useRef, useState } from 'react';

import { FOCUS_RULES, type FocusRule, type FocusScope, type FocusSettings, type LinkRule } from '../lib/focus';
import { formatCount } from '../lib/format';
import { IconFocus } from './Icons';
import { Button, Panel, Tooltip, cx, focusRing } from './ui';

interface Props {
  settings: FocusSettings;
  onChange: (next: FocusSettings) => void;
  bright: number | null;
  total: number;
  disabled: boolean;
  dimmedCount: number;
  selectedCount: number;
  onDimSelected: () => void;
  onUndimSelected: () => void;
  onClearDimmed: () => void;
}

type Slider = 'opacity' | 'blur';

const BRIGHT_NODES: Array<[number, number, number]> = [[40, 22, 5], [52, 54, 5], [108, 16, 5], [176, 48, 5], [150, 60, 4]];
const FADED_NODES: Array<[number, number, number]> = [[16, 42, 4], [212, 18, 4], [236, 56, 4], [128, 64, 3.5], [84, 62, 3.5]];
const ROOT: [number, number] = [118, 40];

function FocusPreview({ opacity, blur, enabled }: { opacity: number; blur: number; enabled: boolean }) {
  const fadedLinks = FADED_NODES.map(([x, y], index) => {
    const [tx, ty] = index % 2 === 0 ? ROOT : BRIGHT_NODES[index % BRIGHT_NODES.length];
    return { x, y, tx, ty };
  });
  return (
    <svg viewBox="0 0 256 76" className="h-[76px] w-full rounded-ui-sm border border-hairline bg-plane" role="img" aria-label="Preview of the faded nodes">
      <g style={{ opacity: enabled ? opacity : 1, filter: enabled && blur > 0 ? `blur(${blur}px)` : undefined }}>
        {fadedLinks.map((link, index) => (
          <line key={index} x1={link.x} y1={link.y} x2={link.tx} y2={link.ty} stroke="var(--series-token)" strokeWidth="1.2" opacity=".7" />
        ))}
        {FADED_NODES.map(([x, y, r], index) => <circle key={index} cx={x} cy={y} r={r} fill="var(--series-eoa)" />)}
      </g>
      {BRIGHT_NODES.map(([x, y], index) => (
        <line key={index} x1={x} y1={y} x2={ROOT[0]} y2={ROOT[1]} stroke="var(--series-token)" strokeWidth="1.4" opacity=".8" />
      ))}
      <line x1={BRIGHT_NODES[3][0]} y1={BRIGHT_NODES[3][1]} x2={BRIGHT_NODES[4][0]} y2={BRIGHT_NODES[4][1]} stroke="var(--series-token)" strokeWidth="1.4" opacity=".8" />
      {BRIGHT_NODES.map(([x, y, r], index) => <circle key={index} cx={x} cy={y} r={r} fill="var(--series-eoa)" />)}
      <circle cx={ROOT[0]} cy={ROOT[1]} r={9} fill="var(--series-focus)" />
    </svg>
  );
}

const THRESHOLDS: Partial<Record<FocusRule, { key: 'connectedMin' | 'sharedMin'; min: number; max: number; unit: string }>> = {
  connected: { key: 'connectedMin', min: 1, max: 20, unit: 'nodes' },
  shared: { key: 'sharedMin', min: 1, max: 10, unit: 'roots' },
};

export function FocusControl({ settings, onChange, bright, total, disabled, dimmedCount, selectedCount, onDimSelected, onUndimSelected, onClearDimmed }: Props) {
  const root = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [dragging, setDragging] = useState<Slider | null>(null);

  useEffect(() => {
    if (!dragging) return;
    const stop = () => setDragging(null);
    window.addEventListener('pointerup', stop);
    window.addEventListener('pointercancel', stop);
    window.addEventListener('blur', stop);
    return () => {
      window.removeEventListener('pointerup', stop);
      window.removeEventListener('pointercancel', stop);
      window.removeEventListener('blur', stop);
    };
  }, [dragging]);

  const peek = (row: Slider | 'rest') => cx(
    '[transition:opacity_150ms_ease]',
    dragging && dragging !== row && 'pointer-events-none opacity-0',
  );

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const set = (change: Partial<FocusSettings>) => onChange({ ...settings, ...change });
  const toggleRule = (rule: FocusRule) => set({
    enabled: true,
    rules: settings.rules.includes(rule) ? settings.rules.filter((entry) => entry !== rule) : [...settings.rules, rule],
  });

  return (
    <div ref={root} className="relative">
      <Tooltip label={settings.enabled ? 'Focus is on' : 'Focus: fade the rest'} side="left" disabled={open}>
        <Button
          variant="ghost"
          icon
          aria-label="Focus"
          aria-expanded={open}
          aria-pressed={settings.enabled}
          disabled={disabled}
          onClick={() => setOpen(!open)}
        >
          <IconFocus />
        </Button>
      </Tooltip>
      {open && (
        <Panel
          className={cx(
            'absolute right-[calc(100%+10px)] top-0 z-20 grid w-[280px] gap-3 p-3 [transition:background_150ms_ease,box-shadow_150ms_ease,border-color_150ms_ease]',
            dragging ? 'border-transparent bg-transparent shadow-none backdrop-blur-none backdrop-saturate-100' : 'bg-surface-1 shadow-pop',
          )}
          aria-label="Focus settings"
        >
          <div className={cx('flex items-center gap-2', peek('rest'))}>
            <span className="text-[10px] font-semibold uppercase tracking-[.08em] text-text-muted">Focus</span>
            {settings.enabled && bright !== null && (
              <span className="text-[11px] tabular-nums text-text-secondary">{formatCount(bright)} of {formatCount(total)} bright</span>
            )}
            <label className="ml-auto flex cursor-pointer items-center gap-1.5 text-xs">
              <input type="checkbox" className="accent-accent" checked={settings.enabled} onChange={(event) => set({ enabled: event.target.checked })} />
              On
            </label>
          </div>

          <div className={cx('flex rounded-ui-sm border border-hairline bg-plane p-0.5', peek('rest'))} role="radiogroup" aria-label="What fades">
            {([['rules', 'Fade by rules'], ['chosen', 'Fade chosen nodes']] as Array<[FocusScope, string]>).map(([value, label]) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={settings.scope === value}
                className={cx(
                  'h-6 flex-1 cursor-pointer rounded-[4px] px-2 text-[11px] font-medium',
                  settings.scope === value ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]' : 'text-text-muted hover:text-text-primary',
                  focusRing,
                )}
                onClick={() => set({ enabled: true, scope: value })}
              >
                {label}
              </button>
            ))}
          </div>

          {settings.scope === 'chosen' && (
            <div className={cx('grid gap-1.5 rounded-ui-sm border border-hairline p-2 text-xs', peek('rest'))}>
              <span className="text-text-secondary">
                {dimmedCount === 0 ? 'No node is chosen yet.' : `${formatCount(dimmedCount)} ${dimmedCount === 1 ? 'node fades' : 'nodes fade'} with their transactions.`}
              </span>
              <div className="flex flex-wrap gap-1">
                <Button className="h-7 px-2 text-[11px]" disabled={selectedCount === 0} onClick={() => { onDimSelected(); set({ enabled: true }); }}>
                  Fade {selectedCount > 0 ? formatCount(selectedCount) : ''} selected
                </Button>
                <Button variant="ghost" className="h-7 px-2 text-[11px]" disabled={selectedCount === 0} onClick={onUndimSelected}>Restore selected</Button>
                <Button variant="ghost" className="ml-auto h-7 px-2 text-[11px]" disabled={dimmedCount === 0} onClick={onClearDimmed}>Restore all</Button>
              </div>
              <span className="text-[10px] text-text-muted">Select nodes with a click, Shift-click or a drag box, then fade them.</span>
            </div>
          )}

          <div className={cx('grid gap-1', peek('rest'))}>
            <FocusPreview opacity={settings.opacity} blur={settings.blur} enabled />
            {!settings.enabled && <span className="text-[10px] text-text-muted">Preview. Moving a slider turns focus on.</span>}
          </div>

          <label className={cx('grid gap-1 rounded-ui-sm text-[11px] text-text-secondary', peek('opacity'), dragging === 'opacity' && 'bg-surface-1 px-2 py-1.5 shadow-pop')}>
            <span className="flex justify-between"><span className="uppercase tracking-[.06em] text-text-muted">Faded opacity</span><span className="tabular-nums">{Math.round(settings.opacity * 100)}%</span></span>
            <input
              type="range"
              min={0}
              max={100}
              value={Math.round(settings.opacity * 100)}
              className="w-full accent-accent"
              onPointerDown={() => setDragging('opacity')}
              onChange={(event) => set({ enabled: true, opacity: Number(event.target.value) / 100 })}
            />
          </label>
          <label className={cx('grid gap-1 rounded-ui-sm text-[11px] text-text-secondary', peek('blur'), dragging === 'blur' && 'bg-surface-1 px-2 py-1.5 shadow-pop')}>
            <span className="flex justify-between"><span className="uppercase tracking-[.06em] text-text-muted">Blur</span><span className="tabular-nums">{settings.blur === 0 ? 'off' : `${settings.blur}px`}</span></span>
            <input
              type="range"
              min={0}
              max={8}
              step={0.5}
              value={settings.blur}
              className="w-full accent-accent"
              onPointerDown={() => setDragging('blur')}
              onChange={(event) => set({ enabled: true, blur: Number(event.target.value) })}
            />
          </label>

          <div className={cx('grid gap-1', peek('rest'), settings.scope === 'chosen' && 'hidden')}>
            <span className="text-[10px] font-medium uppercase tracking-[.07em] text-text-muted">Keep bright, any of</span>
            {FOCUS_RULES.map(({ rule, title, hint }) => {
              const threshold = THRESHOLDS[rule];
              const checked = settings.rules.includes(rule);
              return (
                <div key={rule} className="flex items-center gap-2 text-xs" title={hint}>
                  <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-2">
                    <input type="checkbox" className="accent-accent" checked={checked} onChange={() => toggleRule(rule)} />
                    <span className="truncate">{title}</span>
                  </label>
                  {threshold && (
                    <label className={cx('flex items-center gap-1 text-[11px] text-text-muted', !checked && 'opacity-50')}>
                      ≥
                      <input
                        type="number"
                        min={threshold.min}
                        max={threshold.max}
                        value={settings[threshold.key]}
                        className={cx('h-6 w-10 rounded-[4px] border border-hairline bg-plane px-1 text-right text-xs tabular-nums text-text-primary', focusRing)}
                        onChange={(event) => {
                          const value = Math.min(threshold.max, Math.max(threshold.min, Math.round(Number(event.target.value) || threshold.min)));
                          set({ enabled: true, [threshold.key]: value });
                        }}
                      />
                      {threshold.unit}
                    </label>
                  )}
                </div>
              );
            })}
          </div>

          <div className={cx('grid gap-1', peek('rest'))}>
            <span className="text-[10px] font-medium uppercase tracking-[.07em] text-text-muted">Transactions stay bright when</span>
            <div className="flex rounded-ui-sm border border-hairline bg-plane p-0.5" role="radiogroup" aria-label="Transactions stay bright when">
              {([['both', 'Both ends bright'], ['either', 'Either end bright']] as Array<[LinkRule, string]>).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={settings.links === value}
                  className={cx(
                    'h-6 flex-1 cursor-pointer rounded-[4px] px-2 text-[11px] font-medium',
                    settings.links === value ? 'bg-surface-2 text-text-primary shadow-[0_1px_2px_rgba(0,0,0,0.12)]' : 'text-text-muted hover:text-text-primary',
                    focusRing,
                  )}
                  onClick={() => set({ enabled: true, links: value })}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <p className={cx('m-0 text-[11px] leading-relaxed text-text-muted', peek('rest'))}>{settings.scope === 'rules' ? 'Selected nodes and the one under the pointer stay bright.' : 'The node under the pointer stays bright.'}</p>
        </Panel>
      )}
    </div>
  );
}
