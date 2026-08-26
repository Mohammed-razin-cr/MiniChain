"use client";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Blocks,
  Database,
  FileCheck2,
  Gauge,
  Network,
  ScrollText,
  Settings,
  ShieldCheck,
  Users,
  Menu,
  X,
  Search,
} from "lucide-react";
import { useState } from "react";
import { useAuth } from "../../auth/AuthContext";
import { useRealtime } from "../../realtime/RealtimeContext";
import { useApi } from "../../hooks/useApi";
const nav = [
  ["dashboard", "Dashboard", Gauge, "/"],
  ["blocks", "Blockchain", Blocks, "/blocks"],
  ["transactions", "Transactions", ScrollText, "/transactions"],
  ["records", "Records", Database, "/records"],
  ["validators", "Validators", Users, "/validators"],
  ["network", "Network", Network, "/network"],
  ["integrity", "Integrity", ShieldCheck, "/integrity"],
  ["logs", "Logs", FileCheck2, "/logs"],
  ["settings", "Settings", Settings, "/settings"],
];
export function AppShell({ children, active }) {
  const { role, identity, signOut } = useAuth();
  const { status } = useRealtime();
  const node = useApi((c) => c.get("/network/status"));
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const router = useRouter();
  function submit(e) {
    e.preventDefault();
    const value = search.trim();
    if (!value) return;
    if (/^\d+$/.test(value)) router.push(`/blocks/${value}`);
    else if (/^[a-f\d]{64}$/i.test(value)) router.push(`/blocks/hash/${value}`);
    else if (/^[a-f\d]{8}-[a-f\d-]{27}$/i.test(value))
      router.push(`/transactions/${value}`);
    else router.push(`/records/${encodeURIComponent(value)}`);
    setSearch("");
  }
  return (
    <div className="app-frame">
      <aside className={`sidebar ${open ? "sidebar-open" : ""}`}>
        <div className="brand">
          <div className="brand-mark">
            <Network size={18} />
          </div>
          <div>
            <strong>MiniChain</strong>
            <span>Control Plane</span>
          </div>
        </div>
        <nav aria-label="Primary navigation">
          {nav.map(([id, label, Icon, href]) => (
            <Link
              key={id}
              className={active === id ? "active" : ""}
              href={href}
              onClick={() => setOpen(false)}
            >
              <Icon size={17} />
              <span>{label}</span>
            </Link>
          ))}
        </nav>
        <div className="sidebar-account">
          <span>{identity || "operator"}</span>
          <small>{role}</small>
          <button onClick={signOut}>Disconnect</button>
        </div>
      </aside>
      <div className="app-main">
        <header className="topbar">
          <button
            className="mobile-menu"
            onClick={() => setOpen(!open)}
            aria-label="Toggle navigation"
          >
            {open ? <X /> : <Menu />}
          </button>
          <form className="global-search" onSubmit={submit}>
            <Search size={15} />
            <input
              aria-label="Search chain"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Height, hash, transaction, or record"
            />
          </form>
          {node.data && (
            <div className="node-summary">
              <b>{node.data.node_id}</b>
              <span>{node.data.sync_state}</span>
              <span>Height {node.data.height}</span>
              <span>{node.data.peer_count} peers</span>
            </div>
          )}
          <div className="top-status">
            <span
              className={`connection-dot ${status !== "connected" ? "dot-warn" : ""}`}
            />
            {status === "connected" ? "Live" : status}
            <span className="role-chip">{role}</span>
          </div>
        </header>
        <main className="content">{children}</main>
      </div>
    </div>
  );
}
