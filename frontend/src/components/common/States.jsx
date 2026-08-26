export function LoadingState({ label = "Loading node state" }) {
  return (
    <div className="state-panel" role="status">
      <span className="spinner" />
      {label}
    </div>
  );
}
export function ErrorState({ error, onRetry }) {
  return (
    <div className="state-panel error-state" role="alert">
      <strong>{error?.code || "NODE_UNAVAILABLE"}</strong>
      <p>{error?.message || "The MiniChain node could not be reached."}</p>
      {error?.requestId && <small>Request {error.requestId}</small>}
      <button onClick={onRetry}>Retry</button>
    </div>
  );
}
export function EmptyState({ title, detail }) {
  return (
    <div className="empty-state">
      <strong>{title}</strong>
      <p>{detail}</p>
    </div>
  );
}
