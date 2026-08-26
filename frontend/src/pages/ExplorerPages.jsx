"use client";
import { useState } from "react";
import { useParams } from "next/navigation";
import { Background, Controls, ReactFlow } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { useApi } from "../hooks/useApi";
import { useAuth } from "../auth/AuthContext";
import { useRealtime } from "../realtime/RealtimeContext";
import {
  ErrorState,
  LoadingState,
  EmptyState,
} from "../components/common/States";
import { StatusBadge } from "../components/common/StatusBadge";
import {
  CopyValue,
  JsonPanel,
  Link,
  PageHeading,
  Pager,
  date,
  short,
} from "../components/common/DataView";
const LIMIT = 20;
function ApiSurface({ state, children }) {
  if (state.loading)
    return <LoadingState label="Reading authoritative node state" />;
  if (state.error)
    return <ErrorState error={state.error} onRetry={state.reload} />;
  return children(state.data);
}
export function BlocksPage() {
  const [page, setPage] = useState(1);
  const state = useApi(
    (c) => c.get(`/blocks?page=${page}&limit=${LIMIT}`),
    [page],
  );
  return (
    <>
      <PageHeading
        eyebrow="Chain explorer"
        title="Blocks"
        detail="Committed blocks returned by the connected node."
      />
      <ApiSurface state={state}>
        {(data) =>
          data.items.length ? (
            <div className="panel">
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Height</th>
                      <th>Hash</th>
                      <th>Previous</th>
                      <th>Validator</th>
                      <th>Transactions</th>
                      <th>Time</th>
                    </tr>
                  </thead>
                  <tbody>
                    {[...data.items].reverse().map((b) => (
                      <tr key={b.hash}>
                        <td>
                          <Link
                            className="table-link"
                            href={`/blocks/${b.index}`}
                          >
                            #{b.index}
                          </Link>
                        </td>
                        <td>
                          <CopyValue value={b.hash} />
                        </td>
                        <td>
                          <CopyValue value={b.previous_hash} />
                        </td>
                        <td>{b.validator}</td>
                        <td>{b.transaction_count}</td>
                        <td>{date(b.timestamp)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <Pager {...data} onPage={setPage} />
            </div>
          ) : (
            <EmptyState
              title="No committed blocks"
              detail="The selected page is empty."
            />
          )
        }
      </ApiSurface>
    </>
  );
}
export function BlockDetailPage({ byHash = false }) {
  const p = useParams();
  const value = byHash ? p.hash : p.height;
  const state = useApi(
    (c) => c.get(byHash ? `/blocks/hash/${value}` : `/blocks/${value}`),
    [value, byHash],
  );
  return (
    <>
      <PageHeading
        eyebrow="Block detail"
        title={byHash ? short(value) : `Block #${value}`}
        detail="Header, signature presence, and committed transaction contents."
      />
      <ApiSurface state={state}>
        {(b) => (
          <div className="detail-grid">
            <section className="panel detail-card">
              <dl>
                <Row k="Height" v={`#${b.index}`} />
                <Row
                  k="Hash"
                  node={<CopyValue value={b.hash} shorten={false} />}
                />
                <Row
                  k="Previous hash"
                  node={
                    b.index ? (
                      <Link href={`/blocks/hash/${b.previous_hash}`}>
                        <CopyValue value={b.previous_hash} shorten={false} />
                      </Link>
                    ) : (
                      <CopyValue value={b.previous_hash} shorten={false} />
                    )
                  }
                />
                <Row
                  k="Merkle root"
                  node={<CopyValue value={b.merkle_root} shorten={false} />}
                />
                <Row k="Validator" v={b.validator} />
                <Row
                  k="Signature"
                  node={<StatusBadge status={b.validator_signature_status} />}
                />
                <Row k="Timestamp" v={date(b.timestamp)} />
              </dl>
            </section>
            <section className="panel">
              <div className="panel-title">
                <h2>Transactions ({b.transaction_count})</h2>
              </div>
              {b.transactions?.length ? (
                <div className="list-stack">
                  {b.transactions.map((t) => (
                    <Link key={t.id} href={`/transactions/${t.id}`}>
                      <CopyValue value={t.id} />
                      <span>
                        {t.operation?.type || t.operation_type || "operation"}
                      </span>
                    </Link>
                  ))}
                </div>
              ) : (
                <EmptyState
                  title="No transactions"
                  detail="This block contains no transactions."
                />
              )}
            </section>
          </div>
        )}
      </ApiSurface>
    </>
  );
}
export function TransactionsPage() {
  const [page, setPage] = useState(1);
  const state = useApi(
    (c) => c.get(`/transactions?page=${page}&limit=${LIMIT}`),
    [page],
  );
  return (
    <>
      <PageHeading
        eyebrow="Chain explorer"
        title="Transactions"
        detail="Committed operations and their block placement."
      />
      <ApiSurface state={state}>
        {(data) =>
          data.items.length ? (
            <div className="panel">
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>ID</th>
                      <th>Operation</th>
                      <th>Status</th>
                      <th>Block</th>
                      <th>Submitted</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.items.map((t) => (
                      <tr key={t.transaction_id}>
                        <td>
                          <Link
                            className="table-link"
                            href={`/transactions/${t.transaction_id}`}
                          >
                            {short(t.transaction_id)}
                          </Link>
                        </td>
                        <td>{operationName(t)}</td>
                        <td>
                          <StatusBadge status="committed" />
                        </td>
                        <td>{t.block_height}</td>
                        <td>{date(t.timestamp)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <Pager {...data} onPage={setPage} />
            </div>
          ) : (
            <EmptyState
              title="No transactions"
              detail="No transactions were returned for this page."
            />
          )
        }
      </ApiSurface>
    </>
  );
}
export function TransactionDetailPage() {
  const { id } = useParams();
  const state = useApi(
    async (c) => {
      const [transaction, status] = await Promise.all([
        c.get(`/transactions/${id}`),
        c.get(`/transactions/${id}/status`),
      ]);
      return { transaction, status };
    },
    [id],
  );
  return (
    <>
      <PageHeading
        eyebrow="Transaction detail"
        title={short(id)}
        detail="The stored transaction and its current lifecycle status."
      />
      <ApiSurface state={state}>
        {(d) => (
          <div className="detail-grid">
            <section className="panel detail-card">
              <dl>
                <Row k="ID" node={<CopyValue value={id} shorten={false} />} />
                <Row
                  k="Status"
                  node={<StatusBadge status={d.status.status} />}
                />
                <Row
                  k="Block"
                  node={
                    d.status.block_height != null ? (
                      <Link
                        className="table-link"
                        href={`/blocks/${d.status.block_height}`}
                      >
                        #{d.status.block_height}
                      </Link>
                    ) : (
                      <>Pending</>
                    )
                  }
                />
                <Row k="Operation" v={operationName(d.transaction)} />
              </dl>
            </section>
            <section className="panel">
              <div className="panel-title">
                <h2>Canonical payload</h2>
              </div>
              <JsonPanel value={d.transaction} />
            </section>
          </div>
        )}
      </ApiSurface>
    </>
  );
}
export function RecordsPage() {
  const [id, setId] = useState("");
  return (
    <>
      <PageHeading
        eyebrow="Record verification"
        title="Records"
        detail="Open a record by its exact identifier; MiniChain does not expose an unbounded record scan."
      />
      <form
        className="lookup-panel panel"
        onSubmit={(e) => {
          e.preventDefault();
          if (id.trim())
            location.href = `/records/${encodeURIComponent(id.trim())}`;
        }}
      >
        <label>
          Record identifier
          <input
            value={id}
            onChange={(e) => setId(e.target.value)}
            placeholder="Enter an exact record ID"
            required
          />
        </label>
        <button>Inspect record</button>
      </form>
    </>
  );
}
export function RecordDetailPage() {
  const { id } = useParams();
  const state = useApi(
    async (c) => {
      const [record, history, verification] = await Promise.all([
        c.get(`/records/${encodeURIComponent(id)}`),
        c.get(`/records/${encodeURIComponent(id)}/history`),
        c.post(`/records/${encodeURIComponent(id)}/verify`),
      ]);
      return { record, history, verification };
    },
    [id],
  );
  return (
    <>
      <PageHeading
        eyebrow="Record detail"
        title={id}
        detail="Current indexed state, full mutation history, and cryptographic verification."
      />
      <ApiSurface state={state}>
        {(d) => (
          <div className="detail-grid">
            <section className="panel detail-card">
              <dl>
                <Row
                  k="Record"
                  node={<CopyValue value={id} shorten={false} />}
                />
                <Row
                  k="Status"
                  node={<StatusBadge status={d.record.status} />}
                />
                <Row
                  k="Verified"
                  node={
                    <StatusBadge
                      status={
                        d.verification.cryptographically_verified &&
                        d.verification.merkle_proof_valid &&
                        d.verification.chain_valid
                          ? "verified"
                          : "failed"
                      }
                    />
                  }
                />
                <Row
                  k="Block"
                  node={
                    <Link
                      className="table-link"
                      href={`/blocks/${d.record.block_height}`}
                    >
                      #{d.record.block_height}
                    </Link>
                  }
                />
              </dl>
              <JsonPanel value={d.record} />
            </section>
            <section className="panel">
              <div className="panel-title">
                <h2>History</h2>
              </div>
              {d.history.items.length ? (
                <div className="timeline">
                  {d.history.items.map((item, i) => (
                    <div key={item.transaction_id || i}>
                      <span />
                      <div>
                        <strong>{operationName(item)}</strong>
                        <p>{date(item.timestamp)}</p>
                        <CopyValue value={item.transaction_id} />
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <EmptyState
                  title="No history"
                  detail="No record mutations were returned."
                />
              )}
            </section>
          </div>
        )}
      </ApiSurface>
    </>
  );
}
export function ValidatorsPage() {
  const state = useApi((c) => c.get("/validators"));
  return (
    <>
      <PageHeading
        eyebrow="Consensus identities"
        title="Validators"
        detail="Configured signing identities observed in committed chain state."
      />
      <ApiSurface state={state}>
        {(data) =>
          data.items.length ? (
            <div className="card-grid">
              {data.items.map((v) => (
                <article className="panel entity-card" key={v.validator_id}>
                  <div>
                    <StatusBadge status={v.active ? "active" : "inactive"} />
                    <h2>{v.validator_id}</h2>
                  </div>
                  <CopyValue value={v.public_key.join(":")} shorten={false} />
                  <JsonPanel value={v} />
                </article>
              ))}
            </div>
          ) : (
            <EmptyState
              title="No validators"
              detail="The node returned no validator identities."
            />
          )
        }
      </ApiSurface>
    </>
  );
}
export function NetworkPage() {
  const state = useApi(async (c) => {
    const [local, peers, consistency] = await Promise.all([
      c.get("/network/status"),
      c.get("/network/peers"),
      c.get("/network/consistency"),
    ]);
    return { local, peers, consistency };
  });
  return (
    <>
      <PageHeading
        eyebrow="Topology"
        title="Network"
        detail="The connected node and every peer it currently observes."
      />
      <ApiSurface state={state}>
        {(d) => {
          const peers = d.peers.items || d.peers;
          const nodes = [
            {
              id: d.local.node_id,
              position: { x: 50, y: 120 },
              data: { label: `${d.local.node_id} · #${d.local.height}` },
              className: "flow-local",
            },
            ...peers.map((p, i) => ({
              id: p.id,
              position: {
                x: 350 + (i % 3) * 230,
                y: 30 + Math.floor(i / 3) * 130,
              },
              data: { label: `${p.id} · #${p.height}` },
              className: `flow-${p.state}`,
            })),
          ];
          const edges = peers.map((p) => ({
            id: `${d.local.node_id}-${p.id}`,
            source: d.local.node_id,
            target: p.id,
            animated: p.state === "healthy",
          }));
          return (
            <>
              <section className="metrics-grid compact">
                <Metric label="Local height" value={d.local.height} />
                <Metric label="Peers" value={peers.length} />
                <Metric
                  label="Consistency"
                  value={d.consistency.consistent ? "Consistent" : "Diverged"}
                />
              </section>
              <div className="panel network-canvas">
                <ReactFlow
                  nodes={nodes}
                  edges={edges}
                  fitView
                  nodesDraggable={false}
                >
                  <Background />
                  <Controls />
                </ReactFlow>
              </div>
            </>
          );
        }}
      </ApiSurface>
    </>
  );
}
export function IntegrityPage() {
  const { client, role } = useAuth();
  const state = useApi(async (c) => ({
    storage: await c.get("/storage/stats"),
    consistency: await c.get("/network/consistency"),
  }));
  const [result, setResult] = useState(null);
  const [busy, setBusy] = useState(false);
  async function validate() {
    setBusy(true);
    try {
      setResult(await client.post("/blockchain/validate"));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <PageHeading
        eyebrow="Cryptographic assurance"
        title="Integrity"
        detail="On-demand validation and cross-peer consistency from the node."
        actions={
          <button
            className="primary"
            disabled={busy || role === "viewer"}
            onClick={validate}
          >
            {busy ? "Validating…" : "Validate full chain"}
          </button>
        }
      />
      <ApiSurface state={state}>
        {(d) => (
          <div className="detail-grid">
            <section className="panel detail-card">
              <dl>
                <Row k="Blocks persisted" v={d.storage.blocks} />
                <Row k="Transactions" v={d.storage.transactions} />
                <Row k="Indexed records" v={d.storage.records} />
                <Row
                  k="Peer consistency"
                  node={
                    <StatusBadge
                      status={
                        d.consistency.consistent ? "verified" : "diverged"
                      }
                    />
                  }
                />
              </dl>
            </section>
            <section className="panel">
              <div className="panel-title">
                <h2>Latest validation</h2>
              </div>
              {result ? (
                <JsonPanel value={result} />
              ) : (
                <EmptyState
                  title="No validation run in this session"
                  detail={
                    role === "viewer"
                      ? "Operator access is required to start validation."
                      : "Run validation to inspect every persisted block."
                  }
                />
              )}
            </section>
          </div>
        )}
      </ApiSurface>
    </>
  );
}
export function LogsPage() {
  const { events, status, clear } = useRealtime();
  return (
    <>
      <PageHeading
        eyebrow="Session event stream"
        title="Live logs"
        detail="Ephemeral events received since this tab connected. These are not persisted audit logs."
        actions={<button onClick={clear}>Clear view</button>}
      />
      <div className="panel event-head">
        <StatusBadge status={status} />
        <span>{events.length} events retained in memory</span>
      </div>
      {events.length ? (
        <div className="timeline event-list">
          {events.map((item, i) => (
            <div key={`${item.timestamp}-${i}`}>
              <span />
              <div>
                <strong>
                  {item.event?.type || item.type || "network_event"}
                </strong>
                <p>{date(item.timestamp)}</p>
                <JsonPanel value={item.event || item} />
              </div>
            </div>
          ))}
        </div>
      ) : (
        <EmptyState
          title="Waiting for node events"
          detail="New blocks, transactions, peer health, and synchronization activity will appear here."
        />
      )}
    </>
  );
}
export function SettingsPage() {
  const { baseUrl, identity, role, signOut } = useAuth();
  const { status } = useRealtime();
  return (
    <>
      <PageHeading
        eyebrow="Connection"
        title="Settings"
        detail="Session-scoped node access. Secrets are intentionally never displayed or persisted."
      />
      <section className="panel detail-card settings-card">
        <dl>
          <Row
            k="API endpoint"
            node={<CopyValue value={baseUrl} shorten={false} />}
          />
          <Row k="Identity" v={identity} />
          <Row k="Role" node={<StatusBadge status={role} />} />
          <Row k="Live stream" node={<StatusBadge status={status} />} />
        </dl>
        <button className="danger-button" onClick={signOut}>
          Disconnect and clear credential
        </button>
      </section>
    </>
  );
}
function Row({ k, v, node }) {
  return (
    <div>
      <dt>{k}</dt>
      <dd>{node ?? v ?? "—"}</dd>
    </div>
  );
}
function Metric({ label, value }) {
  return (
    <article className="metric">
      <div className="metric-top">
        <span>{label}</span>
      </div>
      <strong>{String(value)}</strong>
    </article>
  );
}
function operationName(t) {
  const op = t.operation || t.transaction?.operation;
  return op?.type || op?.kind || t.operation_type || t.status || "operation";
}
