import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { monitorApi, openMonitorStream, type BuildJob, type RpcCall, type ScanJob } from '../api/monitor';

const KEPT_CALLS = 1500;
const FLUSH_MS = 250;
const CLOCK_MS = 5_000;
const HEALTH_WINDOW_MS = 60_000;

export type Health = 'idle' | 'good' | 'degraded' | 'failing' | 'offline';

export interface MonitorState {
  connected: boolean;
  calls: RpcCall[];
  scans: ScanJob[];
  builds: BuildJob[];
  lagged: number;
  paused: boolean;
  setPaused: (paused: boolean) => void;
  clear: () => void;
  health: Health;
  recent: { calls: number; problems: number };
}

export function useMonitor(enabled: boolean): MonitorState {
  const [connected, setConnected] = useState(false);
  const [calls, setCalls] = useState<RpcCall[]>([]);
  const [scans, setScans] = useState<ScanJob[]>([]);
  const [builds, setBuilds] = useState<BuildJob[]>([]);
  const [lagged, setLagged] = useState(0);
  const [paused, setPaused] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const pending = useRef<RpcCall[]>([]);
  const pausedRef = useRef(paused);
  pausedRef.current = paused;

  useEffect(() => {
    if (!enabled) return;
    let alive = true;

    void Promise.all([monitorApi.calls(null, 500), monitorApi.scans(null)])
      .then(([initialCalls, initialScans]) => {
        if (!alive) return;
        setCalls(initialCalls);
        setScans(initialScans);
      })
      .catch(() => undefined);

    const close = openMonitorStream({
      onCall: (call) => {
        if (!pausedRef.current) pending.current.push(call);
      },
      onScan: (job) => setScans((current) => {
        const next = current.filter((candidate) => candidate.id !== job.id);
        return [job, ...next].sort((a, b) => b.id - a.id).slice(0, 50);
      }),
      onBuild: (job) => setBuilds((current) => {
        const next = current.filter((candidate) => candidate.id !== job.id);
        return [job, ...next].sort((a, b) => b.id - a.id).slice(0, 20);
      }),
      onLagged: (missed) => setLagged((count) => count + missed),
      onState: setConnected,
    });

    const clock = window.setInterval(() => setNow(Date.now()), CLOCK_MS);
    const flush = window.setInterval(() => {
      if (pending.current.length === 0) return;
      setNow(Date.now());
      const fresh = pending.current;
      pending.current = [];
      setCalls((current) => {
        const known = current.length > 0 ? current[current.length - 1].id : 0;
        const merged = current.concat(fresh.filter((call) => call.id > known));
        return merged.length > KEPT_CALLS ? merged.slice(merged.length - KEPT_CALLS) : merged;
      });
    }, FLUSH_MS);

    return () => {
      alive = false;
      close();
      window.clearInterval(flush);
      window.clearInterval(clock);
    };
  }, [enabled]);

  const clear = useCallback(() => {
    pending.current = [];
    setCalls([]);
    setLagged(0);
  }, []);

  const recent = useMemo(() => {
    const since = now - HEALTH_WINDOW_MS;
    let total = 0;
    let problems = 0;
    for (let index = calls.length - 1; index >= 0 && calls[index].at_ms >= since; index -= 1) {
      total += 1;
      if (calls[index].outcome !== 'ok') problems += 1;
    }
    return { calls: total, problems };
  }, [calls, now]);

  const health: Health = !enabled || !connected
    ? 'offline'
    : recent.calls === 0
      ? 'idle'
      : recent.problems / recent.calls > 0.3
        ? 'failing'
        : recent.problems > 0
          ? 'degraded'
          : 'good';

  return { connected, calls, scans, builds, lagged, paused, setPaused, clear, health, recent };
}
