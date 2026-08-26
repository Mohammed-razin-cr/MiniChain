"use client";
import { ProtectedPage } from "../../../src/components/layout/ProtectedPage";
import { TransactionDetailPage } from "../../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="transactions">
      <TransactionDetailPage />
    </ProtectedPage>
  );
}
