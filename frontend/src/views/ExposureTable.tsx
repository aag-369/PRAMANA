import { useState } from "react";
import type { Asset, Exposure } from "../api";

function label(assets: Asset[], id: string): string {
  const a = assets.find((x) => x.id === id);
  return (a?.subject ?? "").replace(/^CN=/, "") || id.slice(0, 12);
}

/**
 * Computed exposure, highest first.
 *
 * Every number is expandable to its provenance chain, which is what makes the result
 * defensible in front of an auditor: no figure appears without a path back to the circuit
 * that produced it.
 */
export function ExposureTable({ rows, assets }: { rows: Exposure[]; assets: Asset[] }) {
  const [open, setOpen] = useState<string | null>(null);

  if (!rows.length) {
    return <p className="muted">No assessed assets. Run an estimation.</p>;
  }

  return (
    <>
      <h2>Computed exposure</h2>
      <table>
        <thead>
          <tr>
            <th>Asset</th>
            <th>Algorithm</th>
            <th className="num">Logical qubits</th>
            <th className="num">Break p05</th>
            <th className="num">p50</th>
            <th className="num">p95</th>
            <th className="num">Deadline</th>
            <th className="num">P(exposed)</th>
            <th className="num">Score</th>
            <th />
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <>
              <tr key={r.asset_id}>
                <td>{label(assets, r.asset_id)}</td>
                <td className="mono">{r.algorithm}</td>
                <td className="num">{r.logical_qubits.toLocaleString()}</td>
                <td className="num">{r.break_year.p05 ?? "—"}</td>
                <td className="num">{r.break_year.p50 ?? "—"}</td>
                <td className="num">{r.break_year.p95 ?? "—"}</td>
                <td className="num">{Math.round(r.deadline_year)}</td>
                <td className="num">{r.probability_exposed.toFixed(3)}</td>
                <td className="num">
                  <strong>{r.exposure_score.toFixed(1)}</strong>
                  <div className="bar" title={`95% interval ${r.exposure_lower.toFixed(1)}–${r.exposure_upper.toFixed(1)}`}>
                    <i style={{ width: `${Math.min(100, r.exposure_score)}%` }} />
                  </div>
                </td>
                <td>
                  <button
                    onClick={() => setOpen(open === r.asset_id ? null : r.asset_id)}
                    style={{ border: "none", background: "none", cursor: "pointer", color: "inherit" }}
                    aria-label="Show provenance"
                  >
                    {open === r.asset_id ? "▾" : "▸"}
                  </button>
                </td>
              </tr>
              {open === r.asset_id && (
                <tr key={`${r.asset_id}-detail`}>
                  <td colSpan={10}>
                    <div className="panel">
                      <strong>How this number was produced</strong>
                      <div className="sub">{r.exposure_formula}</div>
                      <table style={{ marginTop: 8 }}>
                        <tbody>
                          <tr>
                            <td className="muted">Circuit</td>
                            <td className="mono">{r.pipeline}</td>
                          </tr>
                          {Object.entries(r.provenance).map(([k, v]) => (
                            <tr key={k}>
                              <td className="muted">{k.replace(/_/g, " ")}</td>
                              <td className="mono">{String(v)}</td>
                            </tr>
                          ))}
                          <tr>
                            <td className="muted">break year by architecture</td>
                            <td className="mono">
                              {Object.entries(r.break_year_by_architecture)
                                .map(([a, y]) => `${a}: ${y ?? "never"}`)
                                .join("  ·  ")}
                            </td>
                          </tr>
                          {r.harvest_now_decrypt_later_year && (
                            <tr>
                              <td className="muted">harvest-now-decrypt-later date</td>
                              <td className="mono">
                                {r.harvest_now_decrypt_later_year} — data captured today is
                                at risk from here
                              </td>
                            </tr>
                          )}
                        </tbody>
                      </table>
                      {r.warnings.length > 0 && (
                        <ul className="sub" style={{ marginBottom: 0 }}>
                          {r.warnings.map((w, i) => (
                            <li key={i}>{w}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  </td>
                </tr>
              )}
            </>
          ))}
        </tbody>
      </table>
    </>
  );
}
