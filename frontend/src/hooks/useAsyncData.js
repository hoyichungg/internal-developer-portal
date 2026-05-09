import { useCallback, useEffect, useRef, useState } from "react";

export function useAsyncData(loader, deps = []) {
  const [state, setState] = useState({ loading: true, error: null, value: null });
  const mountedRef = useRef(false);
  const requestIdRef = useRef(0);

  const reload = useCallback(async () => {
    const requestId = requestIdRef.current + 1;
    requestIdRef.current = requestId;

    if (mountedRef.current) {
      setState((current) => ({ ...current, loading: true, error: null }));
    }

    try {
      const value = await loader();
      if (mountedRef.current && requestIdRef.current === requestId) {
        setState({ loading: false, error: null, value });
      }
    } catch (error) {
      if (mountedRef.current && requestIdRef.current === requestId) {
        setState({ loading: false, error, value: null });
      }
    }
  }, deps);

  useEffect(() => {
    mountedRef.current = true;
    reload();

    return () => {
      mountedRef.current = false;
      requestIdRef.current += 1;
    };
  }, [reload]);

  return [state, { reload }];
}
