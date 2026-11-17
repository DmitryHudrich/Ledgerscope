import { useEffect, useState, type MouseEvent } from 'react';

import { displayAddress } from '../lib/chain';
import { IconCopy } from './Icons';
import { cx, focusRing } from './ui';

const COPIED_MS = 1200;

export function CopyAddress({ address, className }: { address: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  const shown = displayAddress(address);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), COPIED_MS);
    return () => window.clearTimeout(timer);
  }, [copied]);

  const copy = async (event: MouseEvent) => {
    event.stopPropagation();
    event.preventDefault();
    try {
      await navigator.clipboard.writeText(shown);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <button
      type="button"
      className={cx(
        'inline-grid size-5 flex-none cursor-pointer place-items-center rounded-[4px] border-0 bg-transparent align-middle text-text-muted hover:bg-[color-mix(in_srgb,var(--text-primary)_8%,transparent)] hover:text-text-primary',
        copied && 'text-good hover:text-good',
        focusRing,
        className,
      )}
      title={copied ? 'Copied' : `Copy ${shown}`}
      aria-label={copied ? 'Address copied' : `Copy address ${shown}`}
      onClick={(event) => void copy(event)}
    >
      {copied ? (
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="m3.5 8.5 3 3 6-7" />
        </svg>
      ) : (
        <IconCopy size={12} />
      )}
    </button>
  );
}
