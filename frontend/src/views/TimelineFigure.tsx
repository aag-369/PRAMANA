import { api } from "../api";

/**
 * The hero view.
 *
 * The figure is rendered server-side as SVG and embedded rather than redrawn in the
 * browser. That is deliberate: the artefact a reader sees here is byte-identical to the
 * one that goes into the paper, so the figure cannot drift from the published one.
 */
export function TimelineFigure({ inventoryId }: { inventoryId: string }) {
  const url = api.timelineSvgUrl(inventoryId);
  return (
    <>
      <h2>Exposure timeline</h2>
      <p className="sub">
        Each band is a break-year probability density computed for that specific key, under
        a distribution over fault-tolerant architectures and fitted hardware trajectories.
        The diamond is the migration deadline: where it falls to the right of the density's
        early tail, the asset is already exposed.
      </p>
      <div className="figure">
        <img src={url} alt="Computed quantum exposure by asset" />
      </div>
      <p className="sub">
        <a href={url} download="pramana-exposure-timeline.svg">
          Download SVG
        </a>{" "}
        — vector, publication resolution, legible in greyscale.
      </p>
    </>
  );
}
