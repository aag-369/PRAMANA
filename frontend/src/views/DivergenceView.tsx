import type { Divergence } from "../api";

/**
 * Criticality-based ranking against computed-exposure ranking.
 *
 * This view exists because it is the project's empirical claim: prioritisations derived
 * from asset criticality alone diverge from those derived from computed attack cost. A
 * rank correlation near zero means a criticality-only assessment would have produced a
 * materially different migration sequence.
 */
export function DivergenceView({ data }: { data: Divergence | null }) {
  if (!data) return <p className="muted">Not computed yet.</p>;
  return (
    <>
      <h2>Prioritisation divergence</h2>
      <div className="grid">
        <div className="panel">
          <div className="muted">Kendall tau</div>
          <div className="stat">{data.kendall_tau.toFixed(3)}</div>
        </div>
        <div className="panel">
          <div className="muted">Spearman rho</div>
          <div className="stat">{data.spearman_rho.toFixed(3)}</div>
        </div>
        <div className="panel">
          <div className="muted">Assets compared</div>
          <div className="stat">{data.n}</div>
        </div>
      </div>
      <p className="sub">{data.interpretation}</p>

      {data.biggest_movers.length > 0 && (
        <>
          <h2>Assets a criticality-only assessment would misplace</h2>
          <table>
            <thead>
              <tr>
                <th>Asset</th>
                <th className="num">Criticality rank</th>
                <th className="num">Computed rank</th>
                <th className="num">Movement</th>
                <th className="num">Score</th>
              </tr>
            </thead>
            <tbody>
              {data.biggest_movers.map((m) => (
                <tr key={m.asset_id}>
                  <td>{m.label.replace(/^CN=/, "")}</td>
                  <td className="num">{m.criticality_rank}</td>
                  <td className="num">{m.computed_rank}</td>
                  <td className={`num ${Math.abs(m.movement) > 1 ? "warn" : ""}`}>
                    {m.movement > 0 ? `↓ ${m.movement}` : m.movement < 0 ? `↑ ${-m.movement}` : "—"}
                  </td>
                  <td className="num">{m.exposure_score.toFixed(1)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}
