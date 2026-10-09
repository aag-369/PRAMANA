import { useEffect, useState } from "react";
import { api, type Divergence, type Exposure, type Asset, type Inventory, type Trajectory } from "./api";
import { ExposureTable } from "./views/ExposureTable";
import { TimelineFigure } from "./views/TimelineFigure";
import { DivergenceView } from "./views/DivergenceView";
import { TrajectoryView } from "./views/TrajectoryView";
import { AssetTable } from "./views/AssetTable";

type Tab = "timeline" | "exposure" | "divergence" | "assets" | "trajectory";

const TABS: Array<[Tab, string]> = [
  ["timeline", "Exposure timeline"],
  ["exposure", "Computed exposure"],
  ["divergence", "Prioritisation divergence"],
  ["assets", "Inventory"],
  ["trajectory", "Hardware trajectory"],
];

export default function App() {
  const [inventories, setInventories] = useState<Inventory[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("timeline");
  const [exposure, setExposure] = useState<Exposure[]>([]);
  const [assets, setAssets] = useState<Asset[]>([]);
  const [divergence, setDivergence] = useState<Divergence | null>(null);
  const [trajectory, setTrajectory] = useState<Trajectory | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .inventories()
      .then((inv) => {
        setInventories(inv);
        if (inv.length && !selected) setSelected(inv[0].id);
      })
      .catch((e) => setError(String(e)));
    api.trajectory().then(setTrajectory).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!selected) return;
    Promise.all([api.exposure(selected), api.assets(selected), api.divergence(selected)])
      .then(([e, a, d]) => {
        setExposure(e);
        setAssets(a);
        setDivergence(d);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [selected]);

  return (
    <>
      <header>
        <h1>PRAMANA</h1>
        <div className="sub">
          Per-asset quantum exposure, computed from synthesised attack circuits and fitted
          hardware trajectories. No fixed Q-Day is assumed anywhere.
        </div>
      </header>
      <main>
        {error && <div className="err">{error}</div>}

        {inventories.length > 1 && (
          <div className="panel">
            <label className="muted" htmlFor="inv">Inventory </label>
            <select
              id="inv"
              value={selected ?? ""}
              onChange={(e) => setSelected(e.target.value)}
            >
              {inventories.map((i) => (
                <option key={i.id} value={i.id}>
                  {i.name} ({i.asset_count} assets)
                </option>
              ))}
            </select>
          </div>
        )}

        <nav role="tablist">
          {TABS.map(([id, label]) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </nav>

        {!selected && <p className="muted">No inventory yet. Ingest certificates to begin.</p>}

        {selected && tab === "timeline" && <TimelineFigure inventoryId={selected} />}
        {selected && tab === "exposure" && <ExposureTable rows={exposure} assets={assets} />}
        {selected && tab === "divergence" && <DivergenceView data={divergence} />}
        {selected && tab === "assets" && <AssetTable assets={assets} />}
        {tab === "trajectory" && <TrajectoryView data={trajectory} />}
      </main>
    </>
  );
}
