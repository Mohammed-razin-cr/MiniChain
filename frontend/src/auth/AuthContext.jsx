"use client";
import { createContext, useContext, useMemo, useState } from "react";
import { createApiClient, DEFAULT_BASE_URL } from "../api/client";
const Context = createContext(null);
export function AuthProvider({ children }) {
  const [session, setSession] = useState({ token: "", role: "", identity: "" });
  const [baseUrl, setBaseUrl] = useState(DEFAULT_BASE_URL);
  const client = useMemo(
    () => createApiClient({ token: session.token, baseUrl }),
    [session.token, baseUrl],
  );
  const value = useMemo(
    () => ({
      ...session,
      baseUrl,
      client,
      signIn: ({ token, role, identity, apiUrl }) => {
        setBaseUrl((apiUrl || DEFAULT_BASE_URL).replace(/\/$/, ""));
        setSession({ token, role, identity });
      },
      signOut: () => setSession({ token: "", role: "", identity: "" }),
    }),
    [session, baseUrl, client],
  );
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export function useAuth() {
  const value = useContext(Context);
  if (!value) throw new Error("AuthProvider required");
  return value;
}
