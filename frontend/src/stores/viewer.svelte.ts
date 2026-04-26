import type { PageSize } from "../lib/ipc";

export type ZoomMode = "custom" | "fit-width" | "fit-page";
export type Rotation = 0 | 90 | 180 | 270;

function createViewerStore(docId: string) {
  let zoom = $state(1.0);
  let zoomMode = $state<ZoomMode>("fit-width");
  let currentPage = $state(0);
  let pageSizes = $state<PageSize[]>([]);
  let containerWidth = $state(800);
  let rotation = $state<Rotation>(0);

  // When rotated 90/270° the page's height becomes the display width.
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

  function setZoom(z: number) {
    zoom = Math.max(0.25, Math.min(4.0, z));
    zoomMode = "custom";
  }
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
