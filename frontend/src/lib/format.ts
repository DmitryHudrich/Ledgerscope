const PICO_ETH_PER_WEI = 10n ** 6n;

export function weiToEth(wei: bigint): number {
  const negative = wei < 0n;
  const pico = (negative ? -wei : wei) / PICO_ETH_PER_WEI;
  const value = Number(pico) / 1e12;
  return negative ? -value : value;
}

export function parseWei(amount: string): bigint {
  try {
    return BigInt(amount);
  } catch {
    return 0n;
  }
}

export function formatEth(wei: bigint): string {
  const value = weiToEth(wei);
  if (value === 0) return '0';
  if (value < 0.0001) return '<0.0001';
  if (value < 1) return value.toFixed(4);
  if (value < 1000) return value.toFixed(3);
  return formatCount(Math.round(value));
}

export function unitsToNumber(raw: bigint, decimals: number): number {
  if (decimals <= 0) return Number(raw);
  const base = 10n ** BigInt(decimals);
  const whole = raw / base;
  const fraction = raw % base;
  return Number(whole) + Number(fraction) / Number(base);
}

export function formatUnits(raw: bigint, decimals: number): string {
  const negative = raw < 0n;
  const value = negative ? -raw : raw;
  const sign = negative ? '−' : '';

  if (value === 0n) return '0';

  const base = decimals > 0 ? 10n ** BigInt(decimals) : 1n;
  const whole = value / base;
  const fraction = value % base;

  if (whole >= 1000n) return sign + formatCount(Number(whole));

  if (whole > 0n) {
    const digits = fraction.toString().padStart(decimals, '0').slice(0, 4).replace(/0+$/, '');
    return `${sign}${whole}${digits ? `.${digits}` : ''}`;
  }

  const digits = fraction.toString().padStart(decimals, '0');
  const firstSignificant = digits.search(/[1-9]/);
  if (firstSignificant < 0) return '0';
  if (firstSignificant >= 6) return `${sign}<0.000001`;

  const trimmed = digits.slice(0, firstSignificant + 4).replace(/0+$/, '');
  return `${sign}0.${trimmed}`;
}

export function formatAmount(raw: bigint, decimals: number, symbol: string): string {
  return `${formatUnits(raw, decimals)} ${symbol}`;
}

export function formatCount(value: number): string {
  return value.toLocaleString('en-US');
}

export function formatCompact(value: number): string {
  if (value < 1000) return Number.isInteger(value) ? String(value) : value.toFixed(1);
  return value.toLocaleString('en-US', { notation: 'compact', maximumFractionDigits: 1 });
}

export function shortAddress(address: string, lead = 6, tail = 4): string {
  if (address.length <= lead + tail + 2) return address;
  return `${address.slice(0, lead)}…${address.slice(-tail)}`;
}

export function shortHash(hash: string): string {
  return shortAddress(hash, 10, 6);
}

export function formatTimestamp(seconds: number): string {
  if (!seconds) return '—';
  return new Date(seconds * 1000).toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatRelative(seconds: number): string {
  if (!seconds) return '—';
  const delta = Date.now() / 1000 - seconds;
  const units: Array<[number, Intl.RelativeTimeFormatUnit]> = [
    [60, 'second'],
    [3600, 'minute'],
    [86400, 'hour'],
    [86400 * 30, 'day'],
    [86400 * 365, 'month'],
    [Infinity, 'year'],
  ];
  const divisors = [1, 60, 3600, 86400, 86400 * 30, 86400 * 365];
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  for (let i = 0; i < units.length; i += 1) {
    if (Math.abs(delta) < units[i][0]) {
      return formatter.format(-Math.round(delta / divisors[i]), units[i][1]);
    }
  }
  return formatTimestamp(seconds);
}
