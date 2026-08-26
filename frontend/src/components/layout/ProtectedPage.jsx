"use client";
import { useAuth } from "../../auth/AuthContext";
import { Login } from "../auth/Login";
import { AppShell } from "./AppShell";
export function ProtectedPage({ active, children }) {
  const { token } = useAuth();
  return token ? <AppShell active={active}>{children}</AppShell> : <Login />;
}
