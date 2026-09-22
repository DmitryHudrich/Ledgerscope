import { useEffect, useRef, useState, type ReactNode } from 'react';

import type { LabelMode } from '../graph/draw';
import type { AnnotationStyle, AnnotationTool } from '../graph/annotations';
import {
  IconDownload,
  IconFit,
  IconFlow,
  IconFreeze,
  IconLabels,
  IconArrow,
  IconEllipse,
  IconLine,
  IconPencil,
  IconPointer,
  IconRectangle,
  IconUnpin,
  IconZoomIn,
  IconZoomOut,
} from './Icons';
import { IconText } from './Icons';
import { Button, Panel, Tooltip } from './ui';

interface Props {
  onZoomIn: () => void;
  onZoomOut: () => void;
  onFit: () => void;
  frozen: boolean;
  onFrozenChange: (next: boolean) => void;
  showFlow: boolean;
  onShowFlowChange: (next: boolean) => void;
  labelMode: LabelMode;
  onLabelModeChange: (next: LabelMode) => void;
  onUnpin: () => void;
  onExport: () => void;
  disabled: boolean;
  annotationTool: AnnotationTool;
  onAnnotationToolChange: (tool: AnnotationTool) => void;
  annotationStyle: AnnotationStyle;
  onAnnotationStyleChange: (style: AnnotationStyle) => void;
}

const LABEL_CYCLE: Record<LabelMode, LabelMode> = {
  auto: 'all',
  all: 'none',
  none: 'auto',
};

const LABEL_TITLE: Record<LabelMode, string> = {
  auto: 'Labels: automatic',
  all: 'Labels: always on',
  none: 'Labels: off',
};

export function ToolRail({
  onZoomIn,
  onZoomOut,
  onFit,
  frozen,
  onFrozenChange,
  showFlow,
  onShowFlowChange,
  labelMode,
  onLabelModeChange,
  onUnpin,
  onExport,
  disabled,
  annotationTool,
  onAnnotationToolChange,
  annotationStyle,
  onAnnotationStyleChange,
}: Props) {
  const root = useRef<HTMLDivElement>(null);
  const [styleOpen, setStyleOpen] = useState(false);

  useEffect(() => {
    setStyleOpen(false);
  }, [annotationTool]);

  useEffect(() => {
    if (!styleOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setStyleOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setStyleOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('keydown', onKeyDown);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('keydown', onKeyDown);
    };
  }, [styleOpen]);

  return (
    <div ref={root} className="absolute right-[calc(var(--inspector-w)+16px)] top-4 z-10 transition-[right] duration-200 ease-[cubic-bezier(0.22,0.61,0.36,1)]">
    <Panel className="flex flex-col gap-0.5 p-[5px]" role="toolbar" aria-label="Graph controls">
      <Tool label="Zoom in" onClick={onZoomIn} disabled={disabled}>
        <IconZoomIn />
      </Tool>
      <Tool label="Zoom out" onClick={onZoomOut} disabled={disabled}>
        <IconZoomOut />
      </Tool>
      <Tool label="Fit to view" onClick={onFit} disabled={disabled}>
        <IconFit />
      </Tool>
      <hr className="mx-0.5 my-1 w-[calc(100%-4px)] border-0 border-t border-hairline" />
      <Tool
        label={frozen ? 'Resume layout' : 'Freeze layout'}
        onClick={() => onFrozenChange(!frozen)}
        pressed={frozen}
        disabled={disabled}
      >
        <IconFreeze />
      </Tool>
      <Tool label="Unpin dragged nodes" onClick={onUnpin} disabled={disabled}>
        <IconUnpin />
      </Tool>
      <hr className="mx-0.5 my-1 w-[calc(100%-4px)] border-0 border-t border-hairline" />
      <Tool
        label={showFlow ? 'Hide flow animation' : 'Show flow animation'}
        onClick={() => onShowFlowChange(!showFlow)}
        pressed={showFlow}
        disabled={disabled}
      >
        <IconFlow />
      </Tool>
      <Tool
        label={LABEL_TITLE[labelMode]}
        onClick={() => onLabelModeChange(LABEL_CYCLE[labelMode])}
        pressed={labelMode === 'all'}
        disabled={disabled}
      >
        <IconLabels />
      </Tool>
      <hr className="mx-0.5 my-1 w-[calc(100%-4px)] border-0 border-t border-hairline" />
      <Tool label="Export PNG" onClick={onExport} disabled={disabled}>
        <IconDownload />
      </Tool>
      <hr className="mx-0.5 my-1 w-[calc(100%-4px)] border-0 border-t border-hairline" />
      {([
        ['select', 'Select (V)', <IconPointer />], ['text', 'Text (T)', <IconText />], ['arrow', 'Arrow (A)', <IconArrow />],
        ['line', 'Line', <IconLine />], ['rectangle', 'Rectangle (R)', <IconRectangle />], ['ellipse', 'Ellipse (O)', <IconEllipse />], ['pencil', 'Pencil (P)', <IconPencil />],
      ] as const).map(([tool, label, icon]) => <Tool key={tool} label={label} onClick={() => { onAnnotationToolChange(tool); setStyleOpen(false); }} pressed={annotationTool === tool} disabled={disabled}>{icon}</Tool>)}
      {annotationTool !== 'select' && <>
        <hr className="mx-0.5 my-1 w-[calc(100%-4px)] border-0 border-t border-hairline" />
        <div className="relative">
          <Tooltip label="Drawing style" side="left" disabled={styleOpen}>
            <Button variant="ghost" icon onClick={() => setStyleOpen((open) => !open)} aria-label="Drawing style" aria-expanded={styleOpen} aria-pressed={styleOpen}>
              <span className="size-3.5 rounded-full border border-[color-mix(in_srgb,var(--text-primary)_35%,transparent)] shadow-[inset_0_0_0_1px_color-mix(in_srgb,#fff_20%,transparent)]" style={{ backgroundColor: annotationStyle.color }} />
            </Button>
          </Tooltip>
          {styleOpen && (
            <Panel className="absolute bottom-0 right-[calc(100%+10px)] w-[210px] p-3 shadow-pop" aria-label="Annotation style">
              <div className="text-[10px] font-semibold uppercase tracking-[.08em] text-text-muted">Style</div>
              <div className="mt-3 text-[10px] font-medium uppercase tracking-[.07em] text-text-muted">Color</div>
              <div className="mt-1.5 flex gap-1.5">
                {['#4f8cff', '#78d7a0', '#ffbf69', '#ff7d8a', '#c89cff', '#f3f5f7'].map((color) => (
                  <button key={color} type="button" aria-label={`Use ${color}`} className="size-5 cursor-pointer rounded-full border-2 border-transparent p-0 outline-none ring-offset-2 focus-visible:ring-2 focus-visible:ring-accent" style={{ backgroundColor: color, borderColor: annotationStyle.color === color ? 'var(--text-primary)' : 'transparent' }} onClick={() => onAnnotationStyleChange({ ...annotationStyle, color })} />
                ))}
                <label className="relative grid size-5 cursor-pointer place-items-center overflow-hidden rounded-full border-0 bg-[conic-gradient(#f44,#fd4,#4d8,#4af,#94f,#f4c,#f44)] p-0 outline-none ring-offset-2 [&:has(input:focus-visible)]:ring-2 [&:has(input:focus-visible)]:ring-accent" title="Custom color">
                  <span className="sr-only">Custom color</span>
                  <input type="color" value={annotationStyle.color} aria-label="Custom drawing color" className="absolute inset-0 size-full cursor-pointer opacity-0" onChange={(event) => onAnnotationStyleChange({ ...annotationStyle, color: event.target.value })} />
                </label>
              </div>
              <label className="mt-3 grid gap-1.5 text-[11px] text-text-secondary">
                <span className="flex justify-between"><span className="uppercase tracking-[.06em] text-text-muted">{annotationTool === 'text' ? 'Text size' : 'Line width'}</span><span className="tabular-nums">{annotationTool === 'text' ? `${annotationStyle.fontSize}px` : `${annotationStyle.strokeWidth}px`}</span></span>
                <input type="range" min={annotationTool === 'text' ? 11 : 1} max={annotationTool === 'text' ? 28 : 6} value={annotationTool === 'text' ? annotationStyle.fontSize : annotationStyle.strokeWidth} className="w-full accent-accent" onChange={(event) => onAnnotationStyleChange(annotationTool === 'text' ? { ...annotationStyle, fontSize: Number(event.target.value) } : { ...annotationStyle, strokeWidth: Number(event.target.value) })} />
              </label>
            </Panel>
          )}
        </div>
      </>}
    </Panel>
    </div>
  );
}

function Tool({
  label,
  onClick,
  pressed,
  disabled,
  children,
}: {
  label: string;
  onClick: () => void;
  pressed?: boolean;
  disabled?: boolean;
  children: ReactNode;
}) {
  return (
    <Tooltip label={label} side="left">
      <Button
        variant="ghost"
        icon
        onClick={onClick}
        aria-label={label}
        aria-pressed={pressed}
        disabled={disabled}
      >
        {children}
      </Button>
    </Tooltip>
  );
}
