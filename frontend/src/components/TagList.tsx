import type { GraphTag, TagSourceKind } from '../api/types';
import { shortAddress } from '../lib/format';
import { PanelSection, PanelTitle, cx } from './ui';

const SOURCE_TONE: Record<TagSourceKind, string> = {
  heuristic: 'bg-[color-mix(in_srgb,var(--accent)_10%,transparent)] text-accent',
  provider: 'bg-[color-mix(in_srgb,var(--series-contract)_14%,transparent)] text-series-contract',
  user: 'bg-[color-mix(in_srgb,var(--good)_14%,transparent)] text-good',
  agent: 'bg-[color-mix(in_srgb,var(--series-token)_14%,transparent)] text-series-token',
};

export function tagTitle(key: string): string {
  const [, ...rest] = key.split('.');
  const words = (rest.length > 0 ? rest.join(' ') : key).replace(/_/g, ' ');
  return words.charAt(0).toUpperCase() + words.slice(1);
}

function distinct(tags: GraphTag[]): GraphTag[] {
  const seen = new Map<string, GraphTag>();
  for (const tag of tags) {
    const id = `${tag.subject}|${tag.key}|${tag.source.kind}:${tag.source.name}`;
    if (!seen.has(id)) seen.set(id, tag);
  }
  return [...seen.values()].sort((a, b) => b.confidence - a.confidence);
}

export function TagList({ tags, title = 'Tags', onPickTag }: {
  tags: GraphTag[];
  title?: string;
  onPickTag?: (key: string) => void;
}) {
  const shown = distinct(tags);
  if (shown.length === 0) return null;

  return (
    <PanelSection>
      <PanelTitle>{title} · {shown.length}</PanelTitle>
      <ul className="m-0 grid list-none gap-2 p-0">
        {shown.map((tag) => (
          <li key={`${tag.subject}|${tag.key}|${tag.source.name}`} className="grid gap-1 rounded-ui-sm border border-hairline px-2.5 py-2">
            <div className="flex items-center gap-2">
              <button
                type="button"
                className="min-w-0 cursor-pointer truncate border-0 bg-transparent p-0 text-left text-xs font-semibold text-text-primary hover:text-accent disabled:cursor-default disabled:hover:text-text-primary"
                disabled={!onPickTag}
                title={onPickTag ? `Show only flows tagged ${tag.key}` : tag.key}
                onClick={() => onPickTag?.(tag.key)}
              >
                {tagTitle(tag.key)}
              </button>
              <span className="text-[10px] uppercase tracking-[0.07em] text-text-muted">{tag.category}</span>
              <span className="ml-auto tabular-nums text-[11px] text-text-secondary" title="Confidence">{Math.round(tag.confidence * 100)}%</span>
            </div>
            {tag.note && <p className="m-0 break-words text-[11px] leading-snug text-text-secondary">{tag.note}</p>}
            <div className="flex flex-wrap items-center gap-1.5 text-[10px] text-text-muted">
              <span className={cx('rounded-full px-1.5 py-px font-semibold uppercase tracking-wide', SOURCE_TONE[tag.source.kind])}>{tag.source.kind}</span>
              <span className="font-mono-ui">{tag.source.name} v{tag.source.version}</span>
              {tag.evidence.length > 0 && (
                <span className="ml-auto" title={tag.evidence.join('\n')}>
                  {tag.evidence.length} evidence tx · {shortAddress(tag.evidence[0], 6, 4)}
                </span>
              )}
            </div>
          </li>
        ))}
      </ul>
    </PanelSection>
  );
}
