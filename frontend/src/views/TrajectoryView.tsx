import type { Trajectory } from "../api";

/**
 * The hardware capability curve and its provenance.
 *
 * Shown because the break-year distribution is only as good as this fit, and because the
 * physical-to-logical spread is the reason the fit uses verified logical qubits rather than
 * physical qubit counts.
 */
export function TrajectoryView({ data }: { data: Trajectory | null }) {
  if (!data) return <p className="muted">Trajectory unavailable.</p>;
  return (
    <>
      <h2>Hardware capability trajectory</h2>
      <p className="sub">
        Fitted on independently verified logical-qubit demonstrations, blended with vendor
        roadmap targets shifted later by the fitted schedule-slip prior. Announced targets
        never enter the fit unshifted.
      </p>
      <div className="grid">
        <div className="panel">
          <div className="muted">Growth per year</div>
          <div className="stat">{data.growth_per_year.toFixed(2)}×</div>
        </div>
        <div className="panel">
          <div className="muted">Fitted points</div>
          <div className="stat">{data.points}</div>
          <div className="sub">{data.first_year}–{data.last_year}</div>
        </div>
        <div className="panel">
          <div className="muted">Mean schedule slip</div>
          <div className="stat">{data.slip_mean_years.toFixed(2)} yr</div>
          <div className="sub">
            {data.slip_observed} observed, {data.slip_censored} censored
          </div>
        </div>
        <div className="panel">
          <div className="muted">Residual spread</div>
          <div className="stat">{data.residual_sd_dex.toFixed(3)}</div>
          <div className="sub">decades, log10</div>
        </div>
      </div>

      <h2>Physical qubits per logical qubit, by modality</h2>
      <p className="sub">
        This spread is why the fit uses logical qubits. A physical-qubit fit would rank a
        large, inefficient machine above a smaller one delivering more logical qubits.
      </p>
      <table>
        <thead>
          <tr>
            <th>Modality</th>
            <th className="num">Geometric mean</th>
            <th className="num">Min</th>
            <th className="num">Max</th>
            <th className="num">n</th>
          </tr>
        </thead>
        <tbody>
          {Object.entries(data.physical_per_logical).map(([m, s]) => (
            <tr key={m}>
              <td>{m}</td>
              <td className="num">{s.geometric_mean.toFixed(1)}:1</td>
              <td className="num">{s.min.toFixed(1)}</td>
              <td className="num">{s.max.toFixed(1)}</td>
              <td className="num">{s.count}</td>
            </tr>
          ))}
        </tbody>
      </table>

      <h2>Projected logical qubits</h2>
      <table>
        <thead>
          <tr>
            <th>Year</th>
            <th className="num">Median logical qubits</th>
          </tr>
        </thead>
        <tbody>
          {Object.entries(data.median_logical_qubits).map(([y, v]) => (
            <tr key={y}>
              <td>{y}</td>
              <td className="num">{Math.round(v).toLocaleString()}</td>
            </tr>
          ))}
        </tbody>
      </table>

      {data.slip_warnings.length > 0 && (
        <details style={{ marginTop: 16 }}>
          <summary>Caveats on the slip prior ({data.slip_warnings.length})</summary>
          <ul className="sub">
            {data.slip_warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}
