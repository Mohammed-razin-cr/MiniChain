"use client";
import { AuthProvider } from "../src/auth/AuthContext";
import { RealtimeProvider } from "../src/realtime/RealtimeContext";
export function Providers({ children }) {
  return (
    <AuthProvider>
      <RealtimeProvider>{children}</RealtimeProvider>
    </AuthProvider>
  );
}
