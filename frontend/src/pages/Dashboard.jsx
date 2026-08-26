"use client";
import Link from "next/link";
import { motion } from "framer-motion";
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { ArrowUpRight, Boxes, Database, Network } from "lucide-react";
import { useApi } from "../hooks/useApi";
import { useRealtime } from "../realtime/RealtimeContext";
import { StatusBadge } from "../components/common/StatusBadge";
import {
  EmptyState,
  ErrorState,
  LoadingState,
} from "../components/common/States";
export function Dashboard() {
  const { status } = useRealtime();
  const state = useApi(async (client) => {
    const [health, network, storage, blocks] = await Promise.all([
      client.get("/health"),
      client.get("/network/status"),
      client.get("/storage/stats"),
      client.get("/blocks?from=0&limit=12"),
    ]);
    return { health, network, storage, blocks };
  });
  if (state.loading)
    return <LoadingState label="Reading node and chain state" />;
  if (state.error)
    return <ErrorState error={state.error} onRetry={state.reload} />;
  const { health, network, storage, blocks } = state.data;
  const metrics = [
    [
      "Block height",
      network.height,
      Boxes,
      `Head ${short(network.latest_hash)}`,
    ],
    [
      "Transactions",
      storage.transactions,
      Database,
      `${storage.records} indexed records`,
    ],
    [
      "Network",
      network.sync_state,
      Network,
      `${network.healthy_peers} / ${network.peer_count} healthy peers`,
    ],
    ["Storage", "Indexed", Database, `${storage.blocks} blocks persisted`],
  ];
  const chart = blocks.items.map((b) => ({
    height: `#${b.index}`,
    transactions: b.transaction_count,
  }));
  return (
    <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}>
      <div className="page-heading">
        <div>
          <p className="eyebrow">Operational overview</p>
          <h1>{health.node_id}</h1>
          <p>
            {status === "connected"
              ? "Authoritative state from the connected MiniChain node."
              : "Last successful state; live connection is unavailable."}
          </p>
        </div>
        <div className="heading-status">
          <StatusBadge status={network.sync_state} />
          <span>Protocol v{network.protocol_version}</span>
        </div>
      </div>
      <section className="metrics-grid">
        {metrics.map(([label, value, Icon, detail]) => (
          <article className="metric" key={label}>
            <div className="metric-top">
              <span>{label}</span>
              <Icon size={16} />
            </div>
            <strong>{value}</strong>
            <small>{detail}</small>
          </article>
        ))}
      </section>
      <section className="dashboard-grid">
        <article className="panel">
          <div className="panel-title">
            <div>
              <p className="eyebrow">Chain activity</p>
              <h2>Transactions per block</h2>
            </div>
            <Link href="/blocks">
              Open explorer <ArrowUpRight size={15} />
            </Link>
          </div>
          {chart.length ? (
            <div className="chart-wrap">
              <ResponsiveContainer width="100%" height="100%">
                <BarChart data={chart}>
                  <CartesianGrid stroke="#252c34" vertical={false} />
                  <XAxis dataKey="height" stroke="#74808c" fontSize={10} />
                  <YAxis stroke="#74808c" fontSize={10} allowDecimals={false} />
                  <Tooltip
                    contentStyle={{
                      background: "#101419",
                      border: "1px solid #252c34",
                    }}
                  />
                  <Bar
                    dataKey="transactions"
                    fill="#6fc4a4"
                    radius={[2, 2, 0, 0]}
                  />
                </BarChart>
              </ResponsiveContainer>
            </div>
          ) : (
            <EmptyState
              title="No blocks available"
              detail="The node returned an empty block page."
            />
          )}
        </article>
        <aside className="panel node-panel">
          <p className="eyebrow">Current node</p>
          <h2>System state</h2>
          <dl>
            <div>
              <dt>Node</dt>
              <dd>{network.node_id}</dd>
            </div>
            <div>
              <dt>Connection</dt>
              <dd>
                <StatusBadge status={status} />
              </dd>
            </div>
            <div>
              <dt>Mempool</dt>
              <dd>{network.mempool_size}</dd>
            </div>
            <div>
              <dt>Peers</dt>
              <dd>{network.peer_count}</dd>
            </div>
            <div>
              <dt>Database</dt>
              <dd>{formatBytes(storage.database_bytes)}</dd>
            </div>
          </dl>
        </aside>
      </section>
      <section className="panel recent-panel">
        <div className="panel-title">
          <div>
            <p className="eyebrow">Canonical ledger</p>
            <h2>Latest blocks</h2>
          </div>
        </div>
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Height</th>
                <th>Hash</th>
                <th>Validator</th>
                <th>Transactions</th>
                <th>Timestamp</th>
              </tr>
            </thead>
            <tbody>
              {[...blocks.items]
                .reverse()
                .slice(0, 6)
                .map((block) => (
                  <tr key={block.hash}>
                    <td>
                      <Link
                        className="table-link"
                        href={`/blocks/${block.index}`}
                      >
                        #{block.index}
                      </Link>
                    </td>
                    <td className="mono">{short(block.hash)}</td>
                    <td>{block.validator}</td>
                    <td>{block.transaction_count}</td>
                    <td>{formatDate(block.timestamp)}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      </section>
    </motion.div>
  );
}
function short(v = "") {
  return v ? `${v.slice(0, 8)}…${v.slice(-6)}` : "—";
}
function formatDate(v) {
  return v
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "short",
        timeStyle: "medium",
      }).format(new Date(v))
    : "—";
}
function formatBytes(v) {
  return v ? `${(v / 1024).toFixed(1)} KiB` : "—";
}
