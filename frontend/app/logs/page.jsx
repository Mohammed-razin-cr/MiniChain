"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { LogsPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="logs">
      <LogsPage />
    </ProtectedPage>
  );
}
