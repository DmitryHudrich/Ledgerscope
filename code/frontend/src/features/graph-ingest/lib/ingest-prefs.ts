/**
 * Remembers the last block range / limits an analyst typed into the ingest
 * dialog, keyed per case, so re-ingesting the same case doesn't mean retyping
 * the same bounds every time. Addresses are intentionally not persisted — they
 * always come from the case's own address list.
 */
export type IngestPrefs = {
  fromBlock: string
  toBlock: string
  maxDepth: string
  maxNodes: string
}

const STORAGE_KEY = 'ledgerscope.ingest.prefs'

type PrefsByCase = Record<string, IngestPrefs>

function readAll(): PrefsByCase {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? (JSON.parse(raw) as PrefsByCase) : {}
  } catch {
    return {}
  }
}

export function loadIngestPrefs(caseId: string): Partial<IngestPrefs> {
  return readAll()[caseId] ?? {}
}

export function saveIngestPrefs(caseId: string, prefs: IngestPrefs): void {
  try {
    const all = readAll()
    all[caseId] = prefs
    localStorage.setItem(STORAGE_KEY, JSON.stringify(all))
  } catch {
    // ignore storage failures (private mode, quota)
  }
}
