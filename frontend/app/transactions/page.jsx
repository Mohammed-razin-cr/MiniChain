"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { TransactionsPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="transactions">
      <TransactionsPage />
    </ProtectedPage>
  );
}
