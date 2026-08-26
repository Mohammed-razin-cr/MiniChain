"use client";
import { useState } from "react";
import { Activity, ArrowRight, LockKeyhole } from "lucide-react";
import { useAuth } from "../../auth/AuthContext";
import { createApiClient, DEFAULT_BASE_URL } from "../../api/client";
export function Login() {
  const { signIn } = useAuth();
  const [apiUrl, setApiUrl] = useState(DEFAULT_BASE_URL);
  const [token, setToken] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function submit(event) {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const identity = await createApiClient({ token, baseUrl: apiUrl }).get(
        "/auth/whoami",
      );
      signIn({
        token,
        role: identity.role,
        identity: identity.identity,
        apiUrl,
      });
    } catch (e) {
      setError(e.message || "Connection failed");
    } finally {
      setBusy(false);
    }
  }
  return (
    <main className="login-shell">
      <section className="login-context">
        <div className="brand-mark">
          <Activity size={18} />
        </div>
        <p className="eyebrow">Permissioned infrastructure</p>
        <h1>
          MiniChain
          <br />
          Control Plane
        </h1>
        <p>
          Inspect chain integrity, network convergence, and institutional record
          history from one precise operational surface.
        </p>
        <div className="system-note">
          <span />
          Credentials stay in this browser tab and are never written to storage.
        </div>
      </section>
      <form className="login-panel" onSubmit={submit}>
        <div className="panel-heading">
          <LockKeyhole size={18} />
          <div>
            <p className="eyebrow">Operator access</p>
            <h2>Connect to a node</h2>
          </div>
        </div>
        <label>
          API endpoint
          <input
            value={apiUrl}
            onChange={(e) => setApiUrl(e.target.value)}
            type="url"
            required
          />
        </label>
        <label>
          Bearer token
          <input
            value={token}
            onChange={(e) => setToken(e.target.value)}
            type="password"
            autoComplete="off"
            required
            placeholder="Enter node API token"
          />
        </label>
        {error && (
          <p className="inline-error" role="alert">
            {error}
          </p>
        )}
        <button disabled={busy}>
          {busy ? "Verifying access…" : "Open control plane"}{" "}
          {!busy && <ArrowRight size={16} />}
        </button>
        <p className="form-note">
          Your identity and role are read from the node after it verifies this
          credential.
        </p>
      </form>
    </main>
  );
}
