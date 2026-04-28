import type { PageSize } from "../lib/ipc";

export type ZoomMode = "custom" | "fit-width" | "fit-page";
export type Rotation = 0 | 90 | 180 | 270;

/** Discrete zoom levels that the +/- buttons and Ctrl+Wheel snap to. */
export const ZOOM_LEVELS = [0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0] as const;

/** Step to the next discrete zoom level above the current value. */
export function snapZoomUp(current: number): number {
  return ZOOM_LEVELS.find((z) => z > current + 0.01) ?? 4.0;
}

/** Step to the next discrete zoom level below the current value. */
export function snapZoomDown(current: number): number {
  return [...ZOOM_LEVELS].reverse().find((z) => z < current - 0.01) ?? 0.25;
}

function createViewerStore(docId: string) {
  let zoom = $state(1.0);
  let zoomMode = $state<ZoomMode>("fit-width");
  let currentPage = $state(0);
  let pageSizes = $state<PageSize[]>([]);
  let containerWidth = $state(800);
  let rotation = $state<Rotation>(0);

  const fitWidthZoom = $derived(
    pageSizes.length > 0 && pageSizes[0] !== undefined
      ? (rotation === 90 || rotation === 270)
          ? (containerWidth - 48) / pageSizes[0].height
          : (containerWidth - 48) / pageSizes[0].width
      : 1.0
  );

  const fitPageZoom = $derived(
    pageSizes[currentPage] !== undefined
      ? (rotation === 90 || rotation === 270)
          ? Math.min((containerWidth - 48) / pageSizes[currentPage]!.height, 1.0)
          : Math.min((containerWidth - 48) / pageSizes[currentPage]!.width, 1.0)
      : 1.0
  );

  const effectiveZoom = $derived(
    zoomMode === "fit-width" ? fitWidthZoom
    : zoomMode === "fit-page" ? fitPageZoom
    : zoom
  );

  /** Set an exact zoom value (clamps to [0.25, 4.0]). */
  function setZoom(z: number) {
    zoom = Math.max(0.25, Math.min(4.0, z));
    zoomMode = "custom";
  }

  /** Step zoom in to the next discrete level. */
  function zoomIn() { setZoom(snapZoomUp(effectiveZoom)); }

  /** Step zoom out to the next discrete level. */
  function zoomOut() { setZoom(snapZoomDown(effectiveZoom)); }

  function setZoomMode(m: ZoomMode) { zoomMode = m; }
  function setCurrentPage(p: number) { currentPage = p; }
  function setPageSizes(sizes: PageSize[]) { pageSizes = sizes; }
  function setContainerWidth(w: number) { containerWidth = w; }
  function rotateCw() { rotation = ((rotation + 90) % 360) as Rotation; }
  function rotateCcw() { rotation = ((rotation + 270) % 360) as Rotation; }

  return {
    docId,
    get zoom() { return zoom; },
    get zoomMode() { return zoomMode; },
    get currentPage() { return currentPage; },
    get pageSizes() { return pageSizes; },
    get containerWidth() { return containerWidth; },
    get effectiveZoom() { return effectiveZoom; },
    get rotation() { return rotation; },
    setZoom,
    zoomIn,
    zoomOut,
    setZoomMode,
    setCurrentPage,
    setPageSizes,
    setContainerWidth,
    rotateCw,
    rotateCcw,
  };
}

export type ViewerStore = ReturnType<typeof createViewerStore>;
export { createViewerStore };
