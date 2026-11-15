import type {
  ButtonHTMLAttributes,
  ComponentPropsWithoutRef,
  ElementType,
  HTMLAttributes,
  InputHTMLAttributes,
  LabelHTMLAttributes,
  ReactNode,
} from 'react';

export function cx(...values: Array<string | false | null | undefined>): string {
  return values.filter(Boolean).join(' ');
}

export const focusRing =
  'focus-visible:rounded-ui-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent';

type ButtonVariant = 'default' | 'primary' | 'ghost';

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  icon?: boolean;
}

const buttonVariant: Record<ButtonVariant, string> = {
  default:
    'border-hairline bg-surface-2 hover:not-disabled:border-hairline-strong hover:not-disabled:bg-[color-mix(in_srgb,var(--text-primary)_5%,var(--surface-2))]',
  primary:
    'border-transparent bg-accent font-[560] text-accent-ink hover:not-disabled:bg-[color-mix(in_srgb,#000_12%,var(--accent))]',
  ghost:
    'border-transparent bg-transparent text-text-secondary hover:not-disabled:border-transparent hover:not-disabled:bg-[color-mix(in_srgb,var(--text-primary)_7%,transparent)] hover:not-disabled:text-text-primary',
};

export function Button({
  variant = 'default',
  icon = false,
  className,
  type = 'button',
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={cx(
        'inline-flex cursor-pointer items-center justify-center gap-[6px] whitespace-nowrap rounded-ui-sm border [transition:background_120ms_ease,border-color_120ms_ease,transform_80ms_ease] active:not-disabled:translate-y-px disabled:cursor-not-allowed disabled:opacity-50 aria-pressed:border-[color-mix(in_srgb,var(--accent)_45%,transparent)] aria-pressed:bg-[color-mix(in_srgb,var(--accent)_16%,transparent)] aria-pressed:text-accent aria-pressed:hover:border-[color-mix(in_srgb,var(--accent)_58%,transparent)] aria-pressed:hover:bg-[color-mix(in_srgb,var(--accent)_22%,transparent)] aria-pressed:hover:text-accent',
        focusRing,
        buttonVariant[variant],
        icon ? 'size-8 p-0 [&_svg]:shrink-0' : 'h-[30px] px-3',
        className,
      )}
      {...props}
    />
  );
}

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

export function Input({ invalid = false, className, ...props }: InputProps) {
  return (
    <input
      className={cx(
        'h-[30px] min-w-0 rounded-ui-sm border border-hairline bg-plane px-[10px] text-inherit [transition:border-color_120ms_ease,box-shadow_120ms_ease] hover:border-hairline-strong focus:border-accent focus:shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_22%,transparent)] focus:outline-none',
        focusRing,
        invalid && 'border-critical hover:border-critical',
        className,
      )}
      aria-invalid={invalid || undefined}
      {...props}
    />
  );
}

interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, 'type'> {
  children: ReactNode;
}

export function Checkbox({ children, className, ...props }: CheckboxProps) {
  return (
    <label className="flex cursor-pointer items-center gap-2 py-[3px] text-[13px] text-text-secondary">
      <input
        type="checkbox"
        className={cx('m-0 size-[15px] accent-accent', focusRing, className)}
        {...props}
      />
      {children}
    </label>
  );
}

export function RangeInput({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      type="range"
      className={cx('w-full accent-accent', focusRing, className)}
      {...props}
    />
  );
}

export function Field({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cx('flex flex-col gap-0.5', className)} {...props} />;
}

export function FieldLabel({ className, ...props }: LabelHTMLAttributes<HTMLLabelElement>) {
  return (
    <label
      className={cx('text-[10px] uppercase tracking-[0.07em] text-text-muted', className)}
      {...props}
    />
  );
}

type PanelProps<T extends ElementType> = {
  as?: T;
} & Omit<ComponentPropsWithoutRef<T>, 'as'>;

export function Panel<T extends ElementType = 'div'>({
  as,
  className,
  ...props
}: PanelProps<T>) {
  const Component = as ?? 'div';
  return (
    <Component
      className={cx(
        'rounded-ui-lg border border-hairline bg-glass shadow-panel backdrop-blur-[14px] backdrop-saturate-[1.2]',
        className,
      )}
      {...props}
    />
  );
}

export function PanelSection({ className, ...props }: HTMLAttributes<HTMLElement>) {
  return (
    <section
      className={cx('border-b border-hairline p-[14px] last:border-b-0', className)}
      {...props}
    />
  );
}

export function PanelTitle({ className, ...props }: HTMLAttributes<HTMLHeadingElement>) {
  return (
    <h2
      className={cx(
        'mb-[10px] mt-0 text-[10px] font-semibold uppercase tracking-[0.09em] text-text-muted',
        className,
      )}
      {...props}
    />
  );
}

export function StatGrid({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cx(
        'grid grid-cols-2 gap-px overflow-hidden rounded-ui border border-hairline bg-hairline',
        className,
      )}
      {...props}
    />
  );
}

export function Stat({ label, value, unit }: { label: string; value: string; unit?: string }) {
  return (
    <div className="bg-surface-1 px-[11px] py-[9px]">
      <div className="text-[10px] uppercase tracking-[0.06em] text-text-muted">{label}</div>
      <div className="text-[19px] font-semibold leading-[1.25] tracking-[-0.02em]">
        {value}
        {unit && (
          <span className="ml-[3px] text-[11px] font-medium text-text-muted">{unit}</span>
        )}
      </div>
    </div>
  );
}

export function MetaList({ className, ...props }: HTMLAttributes<HTMLDListElement>) {
  return (
    <dl
      className={cx(
        'mt-[10px] grid gap-[5px] text-xs [&>div]:flex [&>div]:justify-between [&>div]:gap-[10px] [&_dd]:m-0 [&_dd]:text-right [&_dd]:tabular-nums [&_dt]:text-text-muted',
        className,
      )}
      {...props}
    />
  );
}

export type DotTone = 'eoa' | 'contract' | 'focus' | 'token';

const dotTone: Record<DotTone, string> = {
  eoa: 'bg-series-eoa text-series-eoa',
  contract: 'bg-series-contract text-series-contract',
  focus: 'bg-series-focus text-series-focus',
  token: 'bg-series-token text-series-token',
};

export function Dot({ tone, className }: { tone: DotTone; className?: string }) {
  return (
    <span
      className={cx('inline-block size-2 flex-none rounded-full', dotTone[tone], className)}
      aria-hidden="true"
    />
  );
}

export function Tag({ className, ...props }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cx('inline-flex items-center gap-[5px] text-[11px] text-text-secondary', className)}
      {...props}
    />
  );
}

export function Badge({ className, ...props }: HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cx(
        'rounded-full border border-hairline px-[7px] py-px text-[11px] tabular-nums text-text-muted',
        className,
      )}
      {...props}
    />
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <span className="rounded-[4px] border border-b-2 border-hairline-strong px-[5px] py-px font-mono-ui text-[11px] text-text-secondary">
      {children}
    </span>
  );
}

type TooltipSide = 'left' | 'right';

const tooltipPosition: Record<TooltipSide, string> = {
  left: 'right-full mr-3 -translate-x-1 group-hover/tooltip:translate-x-0',
  right: 'left-full ml-3 translate-x-1 group-hover/tooltip:translate-x-0',
};

const tooltipArrow: Record<TooltipSide, string> = {
  left: '-right-1 border-r border-t',
  right: '-left-1 border-b border-l',
};

export function Tooltip({
  label,
  side = 'right',
  disabled = false,
  children,
}: {
  label: string;
  side?: TooltipSide;
  disabled?: boolean;
  children: ReactNode;
}) {
  if (disabled) {
    return <span className="relative inline-flex">{children}</span>;
  }

  return (
    <span className="group/tooltip relative inline-flex">
      {children}
      <span
        role="tooltip"
        className={cx(
          'pointer-events-none absolute top-1/2 z-50 -translate-y-1/2 whitespace-nowrap rounded-ui-sm border border-hairline bg-surface-2 px-3 py-2 text-xs font-medium leading-none text-text-primary opacity-0 shadow-pop [transition:opacity_120ms_ease,transform_120ms_ease] delay-0 group-hover/tooltip:visible group-hover/tooltip:opacity-100 group-hover/tooltip:delay-[400ms] invisible',
          tooltipPosition[side],
        )}
      >
        {label}
        <span
          aria-hidden="true"
          className={cx(
            'absolute top-1/2 size-2 -translate-y-1/2 rotate-45 border-hairline bg-surface-2',
            tooltipArrow[side],
          )}
        />
      </span>
    </span>
  );
}
