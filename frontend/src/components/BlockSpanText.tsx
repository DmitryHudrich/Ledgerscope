import { formatCount } from '../lib/format';
import { cx } from './ui';

function sharedPrefix(left: string, right: string): number {
  if (left.length !== right.length) return 0;
  let index = 0;
  while (index < left.length - 1 && left[index] === right[index]) index += 1;
  return index;
}

function Split({ text, cut }: { text: string; cut: number }) {
  return (
    <span>
      {cut > 0 && <span className="opacity-45">{text.slice(0, cut)}</span>}
      <span className="font-semibold">{text.slice(cut)}</span>
    </span>
  );
}

export function BlockSpanText({ from, to, className }: { from: number; to: number; className?: string }) {
  const low = formatCount(Math.min(from, to));
  const high = formatCount(Math.max(from, to));

  if (low === high) {
    return <span className={cx('tabular-nums', className)}><Split text={low} cut={0} /></span>;
  }

  const cut = sharedPrefix(low, high);
  return (
    <span className={cx('whitespace-nowrap tabular-nums', className)} title={`${low} – ${high}`}>
      <Split text={low} cut={cut} />
      <span className="opacity-60"> – </span>
      <Split text={high} cut={cut} />
    </span>
  );
}
