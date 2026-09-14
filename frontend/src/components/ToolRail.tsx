import type { ReactNode } from 'react';

import type { LabelMode } from '../graph/draw';
import {
  IconDownload,
  IconFit,
  IconFlow,
  IconFreeze,
  IconLabels,
  IconUnpin,
  IconZoomIn,
  IconZoomOut,
} from './Icons';

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
}: Props) {
  return (
    <div className="panel overlay toolrail" role="toolbar" aria-label="Graph controls">
      <Tool label="Zoom in" onClick={onZoomIn} disabled={disabled}>
        <IconZoomIn />
      </Tool>
      <Tool label="Zoom out" onClick={onZoomOut} disabled={disabled}>
        <IconZoomOut />
      </Tool>
      <Tool label="Fit to view" onClick={onFit} disabled={disabled}>
        <IconFit />
      </Tool>
      <hr />
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
      <hr />
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
      <hr />
      <Tool label="Export PNG" onClick={onExport} disabled={disabled}>
        <IconDownload />
      </Tool>
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
    <button
      type="button"
      className="btn btn-ghost btn-icon"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
