import { useCallback, useEffect, useState } from "react";

const TOKEN_KEY = "idp_token";
const TOKEN_EXPIRES_AT_KEY = "idp_token_expires_at";

type StoredToken = {
  token: string | null;
  expiresAt: string | null;
};

type SetStoredToken = (nextToken: string | null, expiresAt?: string | null) => void;

function readStoredToken(): StoredToken {
  return {
    token: window.localStorage.getItem(TOKEN_KEY),
    expiresAt: window.localStorage.getItem(TOKEN_EXPIRES_AT_KEY)
  };
}

export function useStoredToken(): [string | null, SetStoredToken, string | null] {
  const [storedToken, setStoredTokenState] = useState(readStoredToken);

  const setToken = useCallback<SetStoredToken>((nextToken, expiresAt = null) => {
    if (nextToken) {
      if (expiresAt) {
        window.localStorage.setItem(TOKEN_EXPIRES_AT_KEY, expiresAt);
      } else {
        window.localStorage.removeItem(TOKEN_EXPIRES_AT_KEY);
      }
      window.localStorage.setItem(TOKEN_KEY, nextToken);
    } else {
      window.localStorage.removeItem(TOKEN_KEY);
      window.localStorage.removeItem(TOKEN_EXPIRES_AT_KEY);
    }

    setStoredTokenState((current) => {
      const next = { token: nextToken, expiresAt: nextToken ? expiresAt : null };
      return current.token === next.token && current.expiresAt === next.expiresAt ? current : next;
    });
  }, []);

  useEffect(() => {
    function syncFromStorage(event: StorageEvent) {
      if (event.storageArea && event.storageArea !== window.localStorage) {
        return;
      }
      if (event.key !== TOKEN_KEY && event.key !== TOKEN_EXPIRES_AT_KEY && event.key !== null) {
        return;
      }

      const next = readStoredToken();
      setStoredTokenState((current) =>
        current.token === next.token && current.expiresAt === next.expiresAt ? current : next
      );
    }

    window.addEventListener("storage", syncFromStorage);
    return () => window.removeEventListener("storage", syncFromStorage);
  }, []);

  return [storedToken.token, setToken, storedToken.expiresAt];
}
