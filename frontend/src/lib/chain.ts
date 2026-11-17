import type { ChainFamily } from '../api/types';

const DEFAULT_NATIVE_SYMBOL = 'ETH';
const DEFAULT_NATIVE_DECIMALS = 18;
const CHAIN_KEY = 'ledgerscope:chain:v1';
const ADDRESS_FORM_KEY = 'ledgerscope:address-form:v1';
const TRON_PREFIX = 0x41;
const BASE58 = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
const EVM_ADDRESS_RE = /^0x[0-9a-f]{40}$/;
const TRON_HEX_RE = /^(0x)?41[0-9a-f]{40}$/;
const TRON_BASE58_RE = /^T[1-9A-HJ-NP-Za-km-z]{33}$/;

let native = DEFAULT_NATIVE_SYMBOL;
let decimals = DEFAULT_NATIVE_DECIMALS;
let family: ChainFamily = 'evm';

export type AddressForm = 'auto' | 'hex';
let addressForm: AddressForm = readStoredAddressForm();
const addressFormListeners = new Set<() => void>();

function readStoredAddressForm(): AddressForm {
  try {
    return localStorage.getItem(ADDRESS_FORM_KEY) === 'hex' ? 'hex' : 'auto';
  } catch {
    return 'auto';
  }
}

export function getAddressForm(): AddressForm {
  return addressForm;
}

export function setAddressForm(next: AddressForm): void {
  if (next === addressForm) return;
  addressForm = next;
  try {
    localStorage.setItem(ADDRESS_FORM_KEY, next);
  } catch {
    /* ignore */
  }
  for (const listener of addressFormListeners) listener();
}

export function subscribeAddressForm(listener: () => void): () => void {
  addressFormListeners.add(listener);
  return () => {
    addressFormListeners.delete(listener);
  };
}

export function nativeSymbol(): string {
  return native;
}

export function nativeDecimals(): number {
  return decimals;
}

export function chainFamily(): ChainFamily {
  return family;
}

export function setActiveChain(chain: { native_symbol?: string; native_decimals?: number; family?: ChainFamily } | null): void {
  native = chain?.native_symbol?.trim() || DEFAULT_NATIVE_SYMBOL;
  decimals = chain?.native_decimals ?? DEFAULT_NATIVE_DECIMALS;
  family = chain?.family ?? 'evm';
}

export function displayAddress(address: string): string {
  if (addressForm === 'hex') return address;
  if (family !== 'tron' || !EVM_ADDRESS_RE.test(address)) return address;
  return toTronBase58(address);
}

export function parseAddressInput(raw: string): string | null {
  const typed = raw.trim();
  if (TRON_BASE58_RE.test(typed)) return fromTronBase58(typed);
  const lowered = typed.toLowerCase();
  if (TRON_HEX_RE.test(lowered)) return `0x${lowered.replace(/^0x/, '').slice(2)}`;
  if (EVM_ADDRESS_RE.test(lowered)) return lowered;
  return null;
}

export function parseAddressList(raw: string): { addresses: string[]; invalid: string[] } {
  const addresses: string[] = [];
  const invalid: string[] = [];
  for (const token of raw.split(/[\s,;]+/)) {
    if (!token) continue;
    const parsed = parseAddressInput(token);
    if (parsed === null) {
      if (!invalid.includes(token)) invalid.push(token);
    } else if (!addresses.includes(parsed)) {
      addresses.push(parsed);
    }
  }
  return { addresses, invalid };
}

export function addressHint(): string {
  return family === 'tron' ? 'T… or 0x… address' : '0x… address';
}

export function readStoredChain(): number | null {
  try {
    const stored = Number.parseInt(localStorage.getItem(CHAIN_KEY) ?? '', 10);
    return Number.isFinite(stored) ? stored : null;
  } catch {
    return null;
  }
}

export function storeChain(chainId: number): void {
  try {
    localStorage.setItem(CHAIN_KEY, String(chainId));
  } catch {
    return;
  }
}

function toTronBase58(address: string): string {
  const payload = new Uint8Array(21);
  payload[0] = TRON_PREFIX;
  payload.set(hexToBytes(address.slice(2)), 1);
  const checksum = sha256(sha256(payload)).slice(0, 4);
  const full = new Uint8Array(25);
  full.set(payload);
  full.set(checksum, 21);
  return encodeBase58(full);
}

function fromTronBase58(text: string): string | null {
  const bytes = decodeBase58(text);
  if (!bytes || bytes.length !== 25 || bytes[0] !== TRON_PREFIX) return null;
  const checksum = sha256(sha256(bytes.slice(0, 21))).slice(0, 4);
  if (checksum.some((byte, index) => byte !== bytes[21 + index])) return null;
  return `0x${bytesToHex(bytes.slice(1, 21))}`;
}

function encodeBase58(bytes: Uint8Array): string {
  let value = 0n;
  for (const byte of bytes) value = (value << 8n) | BigInt(byte);
  let out = '';
  while (value > 0n) {
    out = BASE58[Number(value % 58n)] + out;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    out = `1${out}`;
  }
  return out;
}

function decodeBase58(text: string): Uint8Array | null {
  let value = 0n;
  for (const char of text) {
    const digit = BASE58.indexOf(char);
    if (digit < 0) return null;
    value = value * 58n + BigInt(digit);
  }
  const out: number[] = [];
  while (value > 0n) {
    out.unshift(Number(value & 0xffn));
    value >>= 8n;
  }
  for (const char of text) {
    if (char !== '1') break;
    out.unshift(0);
  }
  return Uint8Array.from(out);
}

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i += 1) out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

function sha256(message: Uint8Array): Uint8Array {
  const length = message.length;
  const padded = new Uint8Array(Math.ceil((length + 9) / 64) * 64);
  padded.set(message);
  padded[length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 4, length * 8);
  view.setUint32(padded.length - 8, Math.floor(length / 0x20000000));

  const hash = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const w = new Uint32Array(64);
  const rotr = (x: number, n: number) => (x >>> n) | (x << (32 - n));

  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let i = 0; i < 16; i += 1) w[i] = view.getUint32(offset + i * 4);
    for (let i = 16; i < 64; i += 1) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }

    let [a, b, c, d, e, f, g, h] = hash;
    for (let i = 0; i < 64; i += 1) {
      const t1 = h + (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i];
      const t2 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) + ((a & b) ^ (a & c) ^ (b & c));
      h = g;
      g = f;
      f = e;
      e = (d + t1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (t1 + t2) >>> 0;
    }

    hash[0] += a;
    hash[1] += b;
    hash[2] += c;
    hash[3] += d;
    hash[4] += e;
    hash[5] += f;
    hash[6] += g;
    hash[7] += h;
  }

  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  hash.forEach((word, index) => outView.setUint32(index * 4, word));
  return out;
}
