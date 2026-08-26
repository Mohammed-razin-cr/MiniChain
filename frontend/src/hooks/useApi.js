"use client";
import { useEffect, useEffectEvent, useState } from "react";
import { useAuth } from "../auth/AuthContext";
import { useRealtime } from "../realtime/RealtimeContext";
export function useApi(loader, deps = []) {
  const { client } = useAuth();
  const { generation } = useRealtime();
  const dependencyKey = JSON.stringify(deps);
  const [reloadKey, setReloadKey] = useState(0);
  const [state, setState] = useState({
    data: null,
    loading: true,
    error: null,
  });
  const load = useEffectEvent(async (cancelled) => {
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const data = await loader(client);
      if (!cancelled()) setState({ data, loading: false, error: null });
    } catch (error) {
      if (!cancelled()) setState({ data: null, loading: false, error });
    }
  });
  useEffect(() => {
    let cancelled = false;
    Promise.resolve().then(() => load(() => cancelled));
    return () => {
      cancelled = true;
    };
  }, [client, generation, dependencyKey, reloadKey]);
  return { ...state, reload: () => setReloadKey((v) => v + 1) };
}
