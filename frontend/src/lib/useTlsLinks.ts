import { useCallback, useEffect, useMemo, useState } from 'react';

import { tlsApi, type IpWalletLink } from '../api/tls';

export function useTlsLinks(chainId: number | null, enabled: boolean) {
  const [links, setLinks] = useState<IpWalletLink[]>([]);

  const refresh = useCallback(async () => {
    if (!enabled || chainId == null) {
      setLinks([]);
      return;
    }
    try {
      setLinks(await tlsApi.links(chainId));
    } catch {
      setLinks([]);
    }
  }, [chainId, enabled]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const byAddress = useMemo(() => {
    const map = new Map<string, IpWalletLink[]>();
    for (const link of links) {
      const key = link.address.toLowerCase();
      const bucket = map.get(key);
      if (bucket) bucket.push(link);
      else map.set(key, [link]);
    }
    for (const bucket of map.values()) {
      bucket.sort((a, b) => b.score - a.score);
    }
    return map;
  }, [links]);

  const score = useMemo(() => {
    const map = new Map<string, number>();
    for (const [key, bucket] of byAddress) {
      map.set(key, Math.max(...bucket.map((link) => link.score)));
    }
    return map;
  }, [byAddress]);

  return { links, byAddress, score, refresh };
}
