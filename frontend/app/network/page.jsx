"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { NetworkPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="network">
      <NetworkPage />
    </ProtectedPage>
  );
}
