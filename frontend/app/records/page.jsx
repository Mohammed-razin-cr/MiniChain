"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { RecordsPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="records">
      <RecordsPage />
    </ProtectedPage>
  );
}
