"use client";
import { ProtectedPage } from "../../../src/components/layout/ProtectedPage";
import { BlockDetailPage } from "../../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="blocks">
      <BlockDetailPage />
    </ProtectedPage>
  );
}
