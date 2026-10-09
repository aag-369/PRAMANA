"""The exposure timeline, rendered as publication-quality SVG.

This is the project's hero view and Figure 1 of the paper, so it is hand-composed rather
than produced by a charting library: the shape being drawn is a ridgeline of break-year
*densities* with deadline markers overlaid, which no general-purpose chart type expresses.

Design constraints, all of them deliberate:

- **Densities, not error bars.** Each asset's row is a filled ridge whose height at a year
  is proportional to the number of Monte Carlo samples landing there. An error bar would
  reduce the distribution back to the point estimate the project exists to avoid.
- **Greyscale-safe and WCAG AA.** The fill ramp is a viridis approximation, which is
  perceptually uniform and monotone in lightness, so it survives being printed in black and
  white and keeps text contrast above 4.5:1.
- **The overlap is the finding.** Where an asset's Mosca deadline falls to the right of the
  density's early tail, the asset is exposed. That region is marked explicitly rather than
  left for the reader to infer from two adjacent glyphs.
"""

from __future__ import annotations

import datetime as dt
from dataclasses import dataclass
from typing import Any

#: Viridis control points, sampled from the standard map. Perceptually uniform and
#: monotone in lightness, which is what makes it safe in greyscale.
_VIRIDIS = [
    (0.0, (68, 1, 84)),
    (0.25, (59, 82, 139)),
    (0.5, (33, 145, 140)),
    (0.75, (94, 201, 98)),
    (1.0, (253, 231, 37)),
]

INK = "#16181d"
MUTED = "#5b6472"
GRID = "#dfe3e8"
PAPER = "#ffffff"
ALARM = "#b3261e"


def _viridis(t: float) -> str:
    """Interpolate the viridis ramp at ``t`` in [0, 1]."""
    t = max(0.0, min(1.0, t))
    for i in range(len(_VIRIDIS) - 1):
        t0, c0 = _VIRIDIS[i]
        t1, c1 = _VIRIDIS[i + 1]
        if t0 <= t <= t1:
            f = 0.0 if t1 == t0 else (t - t0) / (t1 - t0)
            r = round(c0[0] + (c1[0] - c0[0]) * f)
            g = round(c0[1] + (c1[1] - c0[1]) * f)
            b = round(c0[2] + (c1[2] - c0[2]) * f)
            return f"#{r:02x}{g:02x}{b:02x}"
    return "#fde725"


def _esc(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


@dataclass
class TimelineRow:
    """One asset's row."""

    label: str
    algorithm: str
    exposure_score: float
    histogram: dict[int, int]
    deadline_year: float
    not_after_year: int | None
    threat_mode: str


#: Regulatory milestones drawn as vertical rules.
MILESTONES: list[tuple[int, str]] = [
    (2027, "CNSA 2.0 procurement gate"),
    (2030, "NIST IR 8547 deprecation"),
    (2035, "NIST IR 8547 disallowance"),
]

ROW_H = 34
ROW_GAP = 6
MARGIN_L = 260
MARGIN_R = 58
MARGIN_T = 118
MARGIN_B = 80
PLOT_W = 820


def render(
    rows: list[TimelineRow],
    *,
    title: str = "Computed quantum exposure by asset",
    first_year: int | None = None,
    last_year: int | None = None,
) -> str:
    """Render the timeline as a standalone SVG document."""
    today = dt.date.today().year
    if not rows:
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="600" height="120">'
            f'<rect width="600" height="120" fill="{PAPER}"/>'
            f'<text x="24" y="64" font-family="Helvetica,Arial,sans-serif" font-size="15" '
            f'fill="{MUTED}">No assessed assets. Run an estimation first.</text></svg>'
        )

    # A single long tail must not set the scale for the whole figure. Bound the axis by
    # each asset's own 98th percentile, then take the widest of those, so a distribution
    # with a thin decade-long tail costs a clip indicator rather than half the plot width.
    def _percentile_year(hist: dict[int, int], q: float) -> int | None:
        if not hist:
            return None
        total = sum(hist.values())
        seen = 0
        for year in sorted(hist):
            seen += hist[year]
            if seen >= total * q:
                return year
        return max(hist)

    p98 = [y for r in rows if (y := _percentile_year(r.histogram, 0.98)) is not None]
    starts = [min(r.histogram) for r in rows if r.histogram]
    deadlines = [int(r.deadline_year) for r in rows]

    lo = first_year or min([today, *starts, *deadlines]) - 1
    hi = last_year or max([*p98, *deadlines]) + 2
    span = max(hi - lo, 1)

    height = MARGIN_T + len(rows) * (ROW_H + ROW_GAP) + MARGIN_B
    width = MARGIN_L + PLOT_W + MARGIN_R

    def x_of(year: float) -> float:
        return MARGIN_L + (year - lo) / span * PLOT_W

    parts: list[str] = []
    parts.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="Helvetica,Arial,sans-serif">'
    )
    parts.append(f'<rect width="{width}" height="{height}" fill="{PAPER}"/>')

    parts.append(
        f'<text x="{MARGIN_L}" y="34" font-size="17" font-weight="600" fill="{INK}">'
        f"{_esc(title)}</text>"
    )
    parts.append(
        f'<text x="{MARGIN_L}" y="55" font-size="12" fill="{MUTED}">'
        "Each band is a break-year probability density computed per asset, not a fixed "
        "Q-Day. Diamond marks the migration deadline.</text>"
    )

    # Year grid.
    step = 5 if span > 20 else 2
    start = lo + (-lo) % step
    for year in range(start, hi + 1, step):
        x = x_of(year)
        parts.append(
            f'<line x1="{x:.1f}" y1="{MARGIN_T - 14}" x2="{x:.1f}" '
            f'y2="{height - MARGIN_B + 8}" stroke="{GRID}" stroke-width="1"/>'
        )
        parts.append(
            f'<text x="{x:.1f}" y="{height - MARGIN_B + 26}" font-size="11" '
            f'fill="{MUTED}" text-anchor="middle">{year}</text>'
        )

    # Regulatory milestones. Labels are staggered across three rows and anchored to avoid
    # running off the plot, because at typical axis scales 2030 and 2035 sit close enough
    # that centred labels on one line overlap into illegibility.
    visible = [(y, lab) for y, lab in MILESTONES if lo <= y <= hi]
    for idx, (year, label) in enumerate(visible):
        x = x_of(year)
        label_y = MARGIN_T - 48 + (idx % 3) * 13
        parts.append(
            f'<line x1="{x:.1f}" y1="{label_y + 3:.1f}" x2="{x:.1f}" '
            f'y2="{height - MARGIN_B + 8}" stroke="{MUTED}" stroke-width="1" '
            f'stroke-dasharray="3 3"/>'
        )
        anchor = "middle"
        tx = x
        if x < MARGIN_L + 70:
            anchor, tx = "start", x + 4
        elif x > MARGIN_L + PLOT_W - 70:
            anchor, tx = "end", x - 4
        parts.append(
            f'<text x="{tx:.1f}" y="{label_y:.1f}" font-size="10" fill="{MUTED}" '
            f'text-anchor="{anchor}">{_esc(label)}</text>'
        )

    max_score = max((r.exposure_score for r in rows), default=1.0) or 1.0

    for i, r in enumerate(rows):
        top = MARGIN_T + i * (ROW_H + ROW_GAP)
        mid = top + ROW_H / 2
        fill = _viridis(r.exposure_score / max(max_score, 1e-9))

        parts.append(
            f'<text x="{MARGIN_L - 12}" y="{mid - 2:.1f}" font-size="12" fill="{INK}" '
            f'text-anchor="end">{_esc(r.label[:34])}</text>'
        )
        parts.append(
            f'<text x="{MARGIN_L - 12}" y="{mid + 12:.1f}" font-size="10" fill="{MUTED}" '
            f'text-anchor="end">{_esc(r.algorithm)} · {_esc(r.threat_mode)}</text>'
        )

        parts.append(
            f'<line x1="{MARGIN_L}" y1="{mid:.1f}" x2="{MARGIN_L + PLOT_W}" '
            f'y2="{mid:.1f}" stroke="{GRID}" stroke-width="0.75"/>'
        )

        if r.histogram:
            peak = max(r.histogram.values()) or 1
            half = ROW_H / 2 - 2
            ordered = [(y, c) for y, c in sorted(r.histogram.items()) if y <= hi]
            clipped = sum(c for y, c in r.histogram.items() if y > hi)
            upper = " ".join(
                f"{x_of(y):.1f},{mid - (c / peak) * half:.1f}" for y, c in ordered
            )
            lower = " ".join(
                f"{x_of(y):.1f},{mid + (c / peak) * half:.1f}"
                for y, c in reversed(ordered)
            )
            if ordered:
                parts.append(
                    f'<polygon points="{upper} {lower}" fill="{fill}" fill-opacity="0.88" '
                    f'stroke="{fill}" stroke-width="0.75"/>'
                )
            if clipped:
                # The distribution continues past the axis. Say so rather than silently
                # truncating: a reader must not mistake the edge for the end of the tail.
                ex = MARGIN_L + PLOT_W + 3
                pct = 100.0 * clipped / sum(r.histogram.values())
                parts.append(
                    f'<polygon points="{ex:.1f},{mid - 5:.1f} {ex + 7:.1f},{mid:.1f} '
                    f'{ex:.1f},{mid + 5:.1f}" fill="{fill}" fill-opacity="0.9"/>'
                    f'<title>{pct:.1f}% of samples fall beyond {hi}</title>'
                )

            # Where the deadline falls beyond the earliest credible break, the asset is
            # exposed. Mark the overlap rather than leaving it to be inferred.
            earliest = min(r.histogram)
            if r.deadline_year > earliest:
                x0 = x_of(earliest)
                x1 = min(x_of(r.deadline_year), MARGIN_L + PLOT_W)
                if x1 > x0:
                    parts.append(
                        f'<rect x="{x0:.1f}" y="{top:.1f}" width="{x1 - x0:.1f}" '
                        f'height="{ROW_H}" fill="{ALARM}" fill-opacity="0.10"/>'
                    )

        if r.not_after_year and lo <= r.not_after_year <= hi:
            x = x_of(r.not_after_year)
            parts.append(
                f'<line x1="{x:.1f}" y1="{top + 4:.1f}" x2="{x:.1f}" '
                f'y2="{top + ROW_H - 4:.1f}" stroke="{MUTED}" stroke-width="1.5"/>'
            )

        dx = x_of(r.deadline_year)
        if lo <= r.deadline_year <= hi:
            parts.append(
                f'<polygon points="{dx:.1f},{mid - 7:.1f} {dx + 6:.1f},{mid:.1f} '
                f'{dx:.1f},{mid + 7:.1f} {dx - 6:.1f},{mid:.1f}" fill="{ALARM}" '
                f'stroke="{PAPER}" stroke-width="1"/>'
            )

        parts.append(
            f'<text x="{MARGIN_L + PLOT_W + 24}" y="{mid + 4:.1f}" font-size="11" '
            f'font-weight="600" fill="{INK}">{r.exposure_score:.0f}</text>'
        )

    # Legend, drawn as real marks rather than unicode glyphs so every entry renders in
    # any font and exports cleanly to PDF.
    ly = height - 34
    x = MARGIN_L
    mid_l = ly - 4

    parts.append(
        f'<polygon points="{x},{mid_l} {x + 9},{mid_l - 5} {x + 18},{mid_l} '
        f'{x + 9},{mid_l + 5}" fill="{_viridis(0.7)}" fill-opacity="0.88"/>'
    )
    parts.append(
        f'<text x="{x + 24}" y="{ly}" font-size="10.5" fill="{MUTED}">'
        "break-year density</text>"
    )
    x += 150

    parts.append(
        f'<polygon points="{x + 6},{mid_l - 6} {x + 12},{mid_l} {x + 6},{mid_l + 6} '
        f'{x},{mid_l}" fill="{ALARM}"/>'
    )
    parts.append(
        f'<text x="{x + 18}" y="{ly}" font-size="10.5" fill="{MUTED}">'
        "migration deadline</text>"
    )
    x += 150

    parts.append(
        f'<line x1="{x + 5}" y1="{mid_l - 6}" x2="{x + 5}" y2="{mid_l + 6}" '
        f'stroke="{MUTED}" stroke-width="1.5"/>'
    )
    parts.append(
        f'<text x="{x + 14}" y="{ly}" font-size="10.5" fill="{MUTED}">'
        "certificate expiry</text>"
    )
    x += 130

    parts.append(
        f'<rect x="{x}" y="{mid_l - 6}" width="16" height="12" fill="{ALARM}" '
        f'fill-opacity="0.10"/>'
    )
    parts.append(
        f'<text x="{x + 22}" y="{ly}" font-size="10.5" fill="{MUTED}">'
        "exposed window</text>"
    )

    parts.append(
        f'<text x="{MARGIN_L}" y="{ly + 18}" font-size="10" fill="{MUTED}">'
        "Fill encodes the exposure score on a perceptually uniform ramp, so the figure is "
        "legible in greyscale. A triangle at the right edge marks a distribution that "
        "continues past the axis.</text>"
    )

    parts.append("</svg>")
    return "".join(parts)


def rows_from_api(assets: list[dict[str, Any]], exposures: list[dict[str, Any]]) -> list[TimelineRow]:
    """Build rows from API payloads."""
    by_id = {a["id"]: a for a in assets}
    out: list[TimelineRow] = []
    for e in exposures:
        a = by_id.get(e["asset_id"], {})
        label = (a.get("subject") or "").replace("CN=", "") or e["asset_id"][:12]
        out.append(
            TimelineRow(
                label=label,
                algorithm=e.get("algorithm", ""),
                exposure_score=float(e["exposure_score"]),
                histogram={int(k): int(v) for k, v in (e.get("break_year_histogram") or {}).items()},
                deadline_year=float(e["deadline_year"]),
                not_after_year=(
                    int(str(a["not_after"])[:4]) if a.get("not_after") else None
                ),
                threat_mode=a.get("threat_mode", ""),
            )
        )
    return out
