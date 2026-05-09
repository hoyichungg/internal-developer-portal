import { useEffect } from "react";

export function useRefresh(callback) {
  useEffect(() => {
    window.addEventListener("idp-refresh", callback);
    return () => window.removeEventListener("idp-refresh", callback);
  }, [callback]);
}
