export type AnnotationTool = 'select' | 'text' | 'arrow' | 'line' | 'rectangle' | 'ellipse' | 'pencil';
export type AnnotationKind = Exclude<AnnotationTool, 'select'>;

export interface Annotation {
  id: string;
  kind: AnnotationKind;
  x: number;
  y: number;
  x2: number;
  y2: number;
  points?: Array<{ x: number; y: number }>;
  text?: string;
  color?: string;
  strokeWidth?: number;
  fontSize?: number;
}

export interface AnnotationStyle {
  color: string;
  strokeWidth: number;
  fontSize: number;
}

export const DEFAULT_ANNOTATION_STYLE: AnnotationStyle = {
  color: '#4f8cff',
  strokeWidth: 2,
  fontSize: 14,
};

export interface ClusterOverlay {
  kind: 'cluster';
  id: string;
  nodeIds: string[];
  label?: string;
}

export type WorldOverlay = { kind: 'annotation'; annotation: Annotation } | ClusterOverlay;
