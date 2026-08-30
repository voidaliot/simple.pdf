import type { PageSize } from "../lib/ipc";
import { clearDocumentFrames } from "../lib/pageRenderCache";
import {
  fittedZoomForSpread,
  spreadExtentForPage,
  type PageLayout,
  type Rotation,
} from "../lib/viewLayout";

export type ZoomMode = "custom" | "fit-width" | "fit-page";
export type { PageLayout, Rotation } from "../lib/viewLayout";

/** CSS pixels occupied by one PDF point at semantic 100% zoom. */
export const CSS_PIXELS_PER_POINT = 96 / 72;
const MIN_ZOOM = 0.25;
const MAX_ZOOM = 4.0;
const MIN_FIT_ZOOM = 0.05;
/** Breathing room on both sides of a fitted page (16px per side). */
const VIEWPORT_HORIZONTAL_INSET_PX = 32;
/** Vertical breathing room used by the whole-page preset. */
const VIEWPORT_VERTICAL_INSET_PX = 48;

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

function clampZoom(value: number): number {
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, value));
}

/** Fit presets must always fit, even for unusually large PDF page sizes. */
function clampFitZoom(value: number): number {
  return Math.max(MIN_FIT_ZOOM, Math.min(MAX_ZOOM, value));
}

function buildViewerStore(docId: string) {
  let zoom = $state(1.0);
  let zoomMode = $state<ZoomMode>("fit-width");
  let currentPage = $state(0);
  let pageSizes = $state<PageSize[]>([]);
  let containerWidth = $state(800);
  let containerHeight = $state(600);
  let rotation = $state<Rotation>(0);
  let pageLayout = $state<PageLayout>("single");
  let scrollLeft = $state(0);
  let scrollTop = $state(0);

  function fitWidthZoomFor(pageIndex: number): number {
    const spread = spreadExtentForPage(pageSizes, pageIndex, pageLayout, rotation);
    if (!spread) return 1.0;
    return clampFitZoom(fittedZoomForSpread(spread, {
      containerWidth,
      cssPixelsPerPoint: CSS_PIXELS_PER_POINT,
      horizontalInset: VIEWPORT_HORIZONTAL_INSET_PX,
    }));
  }

  function fitPageZoomFor(pageIndex: number): number {
    const spread = spreadExtentForPage(pageSizes, pageIndex, pageLayout, rotation);
    if (!spread) return 1.0;
    return clampFitZoom(fittedZoomForSpread(spread, {
      containerWidth,
      containerHeight,
      cssPixelsPerPoint: CSS_PIXELS_PER_POINT,
      horizontalInset: VIEWPORT_HORIZONTAL_INSET_PX,
      verticalInset: VIEWPORT_VERTICAL_INSET_PX,
    }));
  }

  /** Resolve zoom per page so mixed-size documents never reflow on page changes. */
  function zoomForPage(pageIndex: number): number {
    if (zoomMode === "fit-width") return fitWidthZoomFor(pageIndex);
    if (zoomMode === "fit-page") return fitPageZoomFor(pageIndex);
    return zoom;
  }

  const effectiveZoom = $derived(zoomForPage(currentPage));

  /** Set an exact zoom value (clamps to [0.25, 4.0]). */
  function setZoom(z: number) {
    zoom = clampZoom(z);
    zoomMode = "custom";
  }

  /** Step zoom in to the next discrete level. */
  function zoomIn() { setZoom(snapZoomUp(effectiveZoom)); }

  /** Step zoom out to the next discrete level. */
  function zoomOut() { setZoom(snapZoomDown(effectiveZoom)); }

  function setZoomMode(m: ZoomMode) { zoomMode = m; }
  function setPageLayout(layout: PageLayout) { pageLayout = layout; }
  function setCurrentPage(p: number) { currentPage = p; }
  function setPageSizes(sizes: PageSize[]) { pageSizes = sizes; }
  function setContainerSize(width: number, height: number) {
    containerWidth = Math.max(1, width);
    containerHeight = Math.max(1, height);
  }
  /** Compatibility for callers that have not yet switched to setContainerSize(). */
  function setContainerWidth(width: number) { containerWidth = Math.max(1, width); }
  function setScrollPosition(left: number, top: number) {
    scrollLeft = Math.max(0, left);
    scrollTop = Math.max(0, top);
  }
  function rotateCw() { rotation = ((rotation + 90) % 360) as Rotation; }
  function rotateCcw() { rotation = ((rotation + 270) % 360) as Rotation; }

  return {
    docId,
    get zoom() { return zoom; },
    get zoomMode() { return zoomMode; },
    get currentPage() { return currentPage; },
    get pageSizes() { return pageSizes; },
    get containerWidth() { return containerWidth; },
    get containerHeight() { return containerHeight; },
    get effectiveZoom() { return effectiveZoom; },
    get rotation() { return rotation; },
    get pageLayout() { return pageLayout; },
    get scrollLeft() { return scrollLeft; },
    get scrollTop() { return scrollTop; },
    zoomForPage,
    setZoom,
    zoomIn,
    zoomOut,
    setZoomMode,
    setPageLayout,
    setCurrentPage,
    setPageSizes,
    setContainerSize,
    setContainerWidth,
    setScrollPosition,
    rotateCw,
    rotateCcw,
  };
}

export type ViewerStore = ReturnType<typeof buildViewerStore>;

const viewerStores = new Map<string, ViewerStore>();

/** Return the stable viewer state for a document until its tab is closed. */
export function createViewerStore(docId: string): ViewerStore {
  const existing = viewerStores.get(docId);
  if (existing) return existing;
  const store = buildViewerStore(docId);
  viewerStores.set(docId, store);
  return store;
}

/** Release all retained viewer state for a closed document. */
export function disposeViewerStore(docId: string): void {
  viewerStores.delete(docId);
  clearDocumentFrames(docId);
}
