"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { IntegrityPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="integrity">
      <IntegrityPage />
    </ProtectedPage>
  );
}
