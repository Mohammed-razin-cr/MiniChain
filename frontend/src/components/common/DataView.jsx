"use client";
import Link from "next/link";
import { Copy } from "lucide-react";
export const short = (v = "") => (v ? `${v.slice(0, 10)}…${v.slice(-7)}` : "—");
export const date = (v) =>
  v
    ? new Intl.DateTimeFormat(undefined, {
        dateStyle: "medium",
        timeStyle: "medium",
      }).format(new Date(v))
    : "—";
export function CopyValue({ value, shorten = true }) {
  return (
    <span className="copy-value">
      <code>{shorten ? short(value) : value || "—"}</code>
      {value && (
        <button
          onClick={() => navigator.clipboard.writeText(value)}
          aria-label="Copy value"
        >
          <Copy size={13} />
        </button>
      )}
    </span>
  );
}
export function PageHeading({ eyebrow, title, detail, actions }) {
  return (
    <div className="page-heading">
      <div>
        <p className="eyebrow">{eyebrow}</p>
        <h1>{title}</h1>
        <p>{detail}</p>
      </div>
      {actions && <div className="heading-actions">{actions}</div>}
    </div>
  );
}
export function Pager({ page, total, limit, onPage }) {
  const pages = Math.max(1, Math.ceil(total / limit));
  return (
    <div className="pager">
      <button disabled={page <= 1} onClick={() => onPage(page - 1)}>
        Previous
      </button>
      <span>
        Page {page} of {pages}
      </span>
      <button disabled={page >= pages} onClick={() => onPage(page + 1)}>
        Next
      </button>
    </div>
  );
}
export function JsonPanel({ value }) {
  return <pre className="json-panel">{JSON.stringify(value, null, 2)}</pre>;
}
export { Link };
