import type { Asset } from "../api";

/**
 * The inventory.
 *
 * Assets PRAMANA declined to cost are shown first and marked, never hidden. An asset the
 * tool cannot assess is more useful surfaced than silently assigned a plausible number.
 */
export function AssetTable({ assets }: { assets: Asset[] }) {
  const indeterminate = assets.filter((a) => a.indeterminate_reason);
  const rest = assets.filter((a) => !a.indeterminate_reason);

  return (
    <>
      {indeterminate.length > 0 && (
        <>
          <h2 className="warn">Indeterminate ({indeterminate.length})</h2>
          <p className="sub">
            PRAMANA declined to cost these rather than substituting a default.
          </p>
          <table>
            <thead>
              <tr>
                <th>Subject</th>
                <th>Algorithm</th>
                <th>Why</th>
              </tr>
            </thead>
            <tbody>
              {indeterminate.map((a) => (
                <tr key={a.id}>
                  <td>{(a.subject ?? "").replace(/^CN=/, "")}</td>
                  <td className="mono">
                    {a.algorithm_family}
                    {a.curve ? ` (${a.curve})` : ""}
                  </td>
                  <td className="sub">{a.indeterminate_reason}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}

      <h2>Inventory ({rest.length})</h2>
      <table>
        <thead>
          <tr>
            <th>Subject</th>
            <th>Kind</th>
            <th>Algorithm</th>
            <th>Threat mode</th>
            <th className="num">Secrecy (yr)</th>
            <th>Criticality</th>
            <th>Expires</th>
          </tr>
        </thead>
        <tbody>
          {rest.map((a) => (
            <tr key={a.id}>
              <td>{(a.subject ?? "").replace(/^CN=/, "")}</td>
              <td>
                <span className="chip">{a.kind}</span>
              </td>
              <td className="mono">
                {a.algorithm_family}-{a.key_bits}
              </td>
              <td>{a.threat_mode}</td>
              <td className={`num ${a.secrecy_lifetime_years === null ? "warn" : ""}`}>
                {a.secrecy_lifetime_years ?? "unset"}
              </td>
              <td>{a.criticality ?? <span className="muted">unset</span>}</td>
              <td className="mono">{a.not_after?.slice(0, 10) ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </>
  );
}
