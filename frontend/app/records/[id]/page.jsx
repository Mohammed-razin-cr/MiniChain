"use client";
import { ProtectedPage } from "../../../src/components/layout/ProtectedPage";
import { RecordDetailPage } from "../../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="records">
      <RecordDetailPage />
    </ProtectedPage>
  );
}
