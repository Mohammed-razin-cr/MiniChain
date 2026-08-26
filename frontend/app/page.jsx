"use client";
import { useAuth } from "../src/auth/AuthContext";
import { Login } from "../src/components/auth/Login";
import { AppShell } from "../src/components/layout/AppShell";
import { Dashboard } from "../src/pages/Dashboard";

function Surface() {
  const { token } = useAuth();
  return token ? (
    <AppShell active="dashboard">
      <Dashboard />
    </AppShell>
  ) : (
    <Login />
  );
}
export default function Home() {
  return <Surface />;
}
