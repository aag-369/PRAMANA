"""Rank correlation between criticality-based and computed prioritisation."""

from __future__ import annotations

from typing import Any


def _rank(values: list[float]) -> list[float]:
    """Fractional ranks, averaging ties."""
    order = sorted(range(len(values)), key=lambda i: values[i], reverse=True)
    ranks = [0.0] * len(values)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        avg = (i + j) / 2.0 + 1.0
        for k in range(i, j + 1):
            ranks[order[k]] = avg
        i = j + 1
    return ranks


def kendall_tau(a: list[float], b: list[float]) -> float:
    """Kendall's tau-b."""
    n = len(a)
    if n < 2:
        return 0.0
    concordant = discordant = tie_a = tie_b = 0
    for i in range(n):
        for j in range(i + 1, n):
            da, db = a[i] - a[j], b[i] - b[j]
            if da == 0 and db == 0:
                continue
            if da == 0:
                tie_a += 1
            elif db == 0:
                tie_b += 1
            elif (da > 0) == (db > 0):
                concordant += 1
            else:
                discordant += 1
    denom = ((concordant + discordant + tie_a) * (concordant + discordant + tie_b)) ** 0.5
    return (concordant - discordant) / denom if denom else 0.0


def spearman_rho(a: list[float], b: list[float]) -> float:
    """Spearman's rank correlation."""
    n = len(a)
    if n < 2:
        return 0.0
    ra, rb = _rank(a), _rank(b)
    ma, mb = sum(ra) / n, sum(rb) / n
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    den = (sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb)) ** 0.5
    return num / den if den else 0.0


CRITICALITY_WEIGHT = {"low": 1.0, "medium": 2.0, "high": 3.0, "critical": 4.0}
DATA_CLASS_WEIGHT = {"public": 1.0, "internal": 2.0, "confidential": 3.0, "restricted": 4.0}


def criticality_priority(criticality: str | None, data_class: str | None) -> float:
    """The industry-standard ordering: criticality times data class, no quantum content."""
    return CRITICALITY_WEIGHT.get(criticality or "medium", 2.0) * DATA_CLASS_WEIGHT.get(
        data_class or "internal", 2.0
    )


def analyse(rows: list[dict[str, Any]]) -> dict[str, Any]:
    """Compare the two rankings over ``rows`` of asset dicts."""
    if len(rows) < 2:
        return {
            "kendall_tau": 0.0,
            "spearman_rho": 0.0,
            "n": len(rows),
            "biggest_movers": [],
            "interpretation": "at least two assessed assets are needed to compare rankings",
        }

    crit = [criticality_priority(r.get("criticality"), r.get("data_class")) for r in rows]
    computed = [float(r["exposure_score"]) for r in rows]
    tau = kendall_tau(crit, computed)
    rho = spearman_rho(crit, computed)

    rc, rk = _rank(crit), _rank(computed)
    movers = sorted(
        (
            {
                "asset_id": r["asset_id"],
                "label": r.get("label", ""),
                "criticality_rank": int(rc[i]),
                "computed_rank": int(rk[i]),
                "movement": int(rc[i] - rk[i]),
                "exposure_score": r["exposure_score"],
            }
            for i, r in enumerate(rows)
        ),
        key=lambda m: abs(m["movement"]),
        reverse=True,
    )[:10]

    if tau > 0.8:
        interp = (
            "The two orderings largely agree, so criticality-based triage would have "
            "reached similar conclusions on this estate."
        )
    elif tau > 0.4:
        interp = (
            "The orderings partly agree. Assets listed under biggest_movers are those a "
            "criticality-only assessment would have mis-prioritised."
        )
    else:
        interp = (
            "The orderings diverge substantially. Prioritising by criticality alone would "
            "have produced a materially different migration sequence from prioritising by "
            "computed attack cost."
        )

    return {
        "kendall_tau": round(tau, 4),
        "spearman_rho": round(rho, 4),
        "n": len(rows),
        "biggest_movers": movers,
        "interpretation": interp,
    }
