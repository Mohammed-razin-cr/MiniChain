"use client";
import { ProtectedPage } from "../../src/components/layout/ProtectedPage";
import { ValidatorsPage } from "../../src/pages/ExplorerPages";
export default function Page() {
  return (
    <ProtectedPage active="validators">
      <ValidatorsPage />
    </ProtectedPage>
  );
}
