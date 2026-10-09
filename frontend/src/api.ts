/**
 * Typed client for the PRAMANA API.
 *
 * Hand-written rather than generated, because the surface is small and the types are the
 * documentation a reader of this app most needs.
 */

export interface BreakYear {
  p05: number | null;
  p50: number | null;
  p95: number | null;
}

export interface Asset {
  id: string;
  fingerprint: string;
  kind: string;
  algorithm_family: string;
  key_bits: number;
  curve: string | null;
  purpose: string;
  threat_mode: string;
  subject: string | null;
  issuer: string | null;
  not_after: string | null;
  is_ca: boolean;
  secrecy_lifetime_years: number | null;
  criticality: string | null;
  data_class: string | null;
  /** Present when PRAMANA declined to cost this asset rather than guessing. */
  indeterminate_reason: string | null;
}

export interface Exposure {
  asset_id: string;
  kind: string;
  algorithm: string;
  pipeline: string;
  logical_qubits: number;
  break_year: BreakYear;
  break_year_by_architecture: Record<string, number | null>;
  deadline_year: number;
  probability_exposed: number;
  harvest_now_decrypt_later_year: number | null;
  exposure_score: number;
  exposure_lower: number;
  exposure_upper: number;
  exposure_formula: string;
  warnings: string[];
  provenance: Record<string, unknown>;
}

export interface Divergence {
  kendall_tau: number;
  spearman_rho: number;
  n: number;
  biggest_movers: Array<{
    asset_id: string;
    label: string;
    criticality_rank: number;
    computed_rank: number;
    movement: number;
    exposure_score: number;
  }>;
  interpretation: string;
}

export interface Trajectory {
  growth_per_year: number;
  residual_sd_dex: number;
  points: number;
  first_year: number;
  last_year: number;
  slip_mean_years: number;
  slip_observed: number;
  slip_censored: number;
  slip_warnings: string[];
  physical_per_logical: Record<
    string,
    { geometric_mean: number; min: number; max: number; count: number }
  >;
  median_logical_qubits: Record<string, number>;
}

export interface Inventory {
  id: string;
  name: string;
  organisation: string;
  asset_count: number;
}

const BASE = "/api/v1";

async function get<T>(path: string): Promise<T> {
  const res = await fetch(`${BASE}${path}`);
  if (!res.ok) throw new Error(`${res.status} ${res.statusText}: ${await res.text()}`);
  return (await res.json()) as T;
}

export const api = {
  inventories: () => get<Inventory[]>("/inventories"),
  assets: (id: string) => get<Asset[]>(`/inventories/${id}/assets`),
  exposure: (id: string) => get<Exposure[]>(`/inventories/${id}/exposure`),
  divergence: (id: string) => get<Divergence>(`/inventories/${id}/divergence`),
  trajectory: () => get<Trajectory>("/trajectory"),
  architectures: () => get<Array<Record<string, unknown>>>("/architectures"),
  timelineSvgUrl: (id: string) => `${BASE}/inventories/${id}/timeline.svg`,
  recommendation: (assetId: string) =>
    get<Record<string, unknown>>(`/assets/${assetId}/recommendation`),
  agility: (assetId: string) => get<Record<string, unknown>>(`/assets/${assetId}/agility`),
};
