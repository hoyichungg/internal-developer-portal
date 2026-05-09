import { useCallback, useState } from "react";

const TOKEN_KEY = "idp_token";

export function useStoredToken(): [string | null, (nextToken: string | null) => void] {
  const [token, setTokenState] = useState(() => window.localStorage.getItem(TOKEN_KEY));

  const setToken = useCallback((nextToken: string | null) => {
    setTokenState(nextToken);
    if (nextToken) {
      window.localStorage.setItem(TOKEN_KEY, nextToken);
    } else {
      window.localStorage.removeItem(TOKEN_KEY);
    }
  }, []);

  return [token, setToken];
}
