export function StatusBadge({ status }) {
  const value = String(status || "unknown").toLowerCase();
  const good = [
    "healthy",
    "synced",
    "valid",
    "ready",
    "active",
    "committed",
    "verified",
  ].includes(value);
  return (
    <span className={`status-badge ${good ? "status-good" : "status-warn"}`}>
      {good ? "✓" : "•"} {value.toUpperCase()}
    </span>
  );
}
