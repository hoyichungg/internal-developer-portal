import { useCallback, useEffect, useRef, useState } from "react";
import type { DependencyList } from "react";

type AsyncState<T> = {
  loading: boolean;
  error: Error | null;
  value: T | null;
};

export function useAsyncData<T = unknown>(
  loader: () => Promise<T>,
  deps: DependencyList = []
): [AsyncState<T>, { reload: () => Promise<void> }] {
  const [state, setState] = useState<AsyncState<T>>({ loading: true, error: null, value: null });
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
        setState({
          loading: false,
          error: error instanceof Error ? error : new Error(String(error)),
          value: null
        });
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
