"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { SettingsPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="settings">
      <SettingsPage />
    </ProtectedPage>
  );
}
