import type { PageSize } from "../lib/ipc";

export type ZoomMode = "custom" | "fit-width" | "fit-page";

export interface ViewerState {
  zoom: number;
  zoomMode: ZoomMode;
  currentPage: number;
  pageSizes: PageSize[];
  containerWidth: number;
}

function createViewerStore(docId: string) {
  let zoom = $state(1.0);
  let zoomMode = $state<ZoomMode>("fit-width");
  let currentPage = $state(0);
  let pageSizes = $state<PageSize[]>([]);
  let containerWidth = $state(800);

  const fitWidthZoom = $derived(
    pageSizes.length > 0 && pageSizes[0] !== undefined
      ? (containerWidth - 48) / pageSizes[0].width
      : 1.0
  );
  const fitPageZoom = $derived(
    pageSizes[currentPage] !== undefined
      ? Math.min(
          (containerWidth - 48) / pageSizes[currentPage]!.width,
          1.0
        )
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

  return {
    docId,
    get zoom() { return zoom; },
    get zoomMode() { return zoomMode; },
    get currentPage() { return currentPage; },
    get pageSizes() { return pageSizes; },
    get containerWidth() { return containerWidth; },
    get effectiveZoom() { return effectiveZoom; },
    setZoom,
    setZoomMode,
    setCurrentPage,
    setPageSizes,
    setContainerWidth,
  };
}

export type ViewerStore = ReturnType<typeof createViewerStore>;
export { createViewerStore };
