import { useCallback, useEffect, useState } from 'react';

import { accountApi, type AuthConfig, type Me } from '../api/account';
import { ApiError } from '../api/client';

export interface AccountState {
  config: AuthConfig | null;
  me: Me | null;
  ready: boolean;
  setMe: (me: Me | null) => void;
  refresh: () => Promise<void>;
  logout: () => Promise<void>;
}

export function useAccount(): AccountState {
  const [config, setConfig] = useState<AuthConfig | null>(null);
  const [me, setMe] = useState<Me | null>(null);
  const [ready, setReady] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setMe(await accountApi.me());
    } catch (cause) {
      if (cause instanceof ApiError && (cause.status === 401 || cause.status === 503)) setMe(null);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const next = await accountApi.config();
        if (cancelled) return;
        setConfig(next);
        if (next.enabled) await refresh();
      } catch {
        if (!cancelled) setConfig({ enabled: false, registration_open: false });
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refresh]);

  const logout = useCallback(async () => {
    try {
      await accountApi.logout();
    } finally {
      setMe(null);
    }
  }, []);

  return { config, me, ready, setMe, refresh, logout };
}
