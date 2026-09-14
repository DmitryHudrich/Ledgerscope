import type { SVGProps } from 'react';

type IconProps = SVGProps<SVGSVGElement> & { size?: number };

function Icon({ size = 16, children, ...rest }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {children}
    </svg>
  );
}

export const IconSearch = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="7" cy="7" r="4.25" />
    <path d="M10.2 10.2 13.5 13.5" />
  </Icon>
);

export const IconZoomIn = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="7" cy="7" r="4.25" />
    <path d="M7 5.2v3.6M5.2 7h3.6M10.2 10.2 13.5 13.5" />
  </Icon>
);

export const IconZoomOut = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="7" cy="7" r="4.25" />
    <path d="M5.2 7h3.6M10.2 10.2 13.5 13.5" />
  </Icon>
);

export const IconFit = (p: IconProps) => (
  <Icon {...p}>
    <path d="M2.5 6V3.5a1 1 0 0 1 1-1H6M10 2.5h2.5a1 1 0 0 1 1 1V6M13.5 10v2.5a1 1 0 0 1-1 1H10M6 13.5H3.5a1 1 0 0 1-1-1V10" />
  </Icon>
);

export const IconFreeze = (p: IconProps) => (
  <Icon {...p}>
    <path d="M8 2v12M3.2 4.8l9.6 6.4M12.8 4.8l-9.6 6.4" />
  </Icon>
);

export const IconFlow = (p: IconProps) => (
  <Icon {...p}>
    <path d="M2 8h11" />
    <path d="m9.8 5 3 3-3 3" />
  </Icon>
);

export const IconLabels = (p: IconProps) => (
  <Icon {...p}>
    <path d="M2.5 4.5h11M2.5 8h7M2.5 11.5h9" />
  </Icon>
);

export const IconUnpin = (p: IconProps) => (
  <Icon {...p}>
    <path d="M6 2.5h4l-.6 4 2.1 2.2H4.5L6.6 6.5z" />
    <path d="M8 8.7V13.5" />
  </Icon>
);

export const IconDownload = (p: IconProps) => (
  <Icon {...p}>
    <path d="M8 2.5v7.5M5.2 7.3 8 10.1l2.8-2.8M3 12.5h10" />
  </Icon>
);

export const IconClose = (p: IconProps) => (
  <Icon {...p}>
    <path d="m4 4 8 8M12 4l-8 8" />
  </Icon>
);

export const IconChevron = (p: IconProps) => (
  <Icon {...p}>
    <path d="m5 6.5 3 3 3-3" />
  </Icon>
);

export const IconSun = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1.5v1.3M8 13.2v1.3M14.5 8h-1.3M2.8 8H1.5M12.6 3.4l-.9.9M4.3 11.7l-.9.9M12.6 12.6l-.9-.9M4.3 4.3l-.9-.9" />
  </Icon>
);

export const IconMoon = (p: IconProps) => (
  <Icon {...p}>
    <path d="M13 9.6A5.6 5.6 0 0 1 6.4 3 5.6 5.6 0 1 0 13 9.6Z" />
  </Icon>
);

export const IconTable = (p: IconProps) => (
  <Icon {...p}>
    <rect x="2.5" y="3" width="11" height="10" rx="1" />
    <path d="M2.5 6.5h11M6.5 6.5V13" />
  </Icon>
);

export const IconCopy = (p: IconProps) => (
  <Icon {...p}>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.2" />
    <path d="M10.5 5.5v-1a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h1" />
  </Icon>
);

export const IconTarget = (p: IconProps) => (
  <Icon {...p}>
    <circle cx="8" cy="8" r="5.2" />
    <circle cx="8" cy="8" r="1.6" />
  </Icon>
);

export const IconLogo = (p: IconProps) => (
  <Icon {...p} size={p.size ?? 20} strokeWidth={1.4}>
    <circle cx="4.2" cy="4.6" r="1.9" />
    <circle cx="12" cy="3.6" r="1.4" />
    <circle cx="11.4" cy="11.8" r="2.2" />
    <circle cx="3.4" cy="11.4" r="1.3" />
    <path d="M5.9 5.2 9.8 10M5.7 4.2l4.9-.4M4 6.5v3.6M5 11.5l4.2.2" />
  </Icon>
);
