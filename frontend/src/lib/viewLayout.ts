export type PageLayout = "single" | "dual";
export type Rotation = 0 | 90 | 180 | 270;

export interface LayoutPageSize {
  width: number;
  height: number;
}

export interface SpreadExtent {
  /** Combined page width in PDF points, before the CSS gutter is added. */
  width: number;
  /** Height of the tallest page in PDF points. */
  height: number;
  pageCount: number;
}

export interface PageVerticalBounds {
  index: number;
  top: number;
  bottom: number;
}

/** Must match the gap used by Viewer.svelte for pages within one spread. */
export const DUAL_PAGE_GAP_PX = 12;

export function spreadStartForPage(pageIndex: number, layout: PageLayout): number {
  return layout === "dual" ? Math.floor(pageIndex / 2) * 2 : pageIndex;
}

/**
 * Measure the single page or consecutive two-page spread containing pageIndex.
 * Both pages in a dual spread therefore resolve to the same fitted zoom.
 */
export function spreadExtentForPage(
  pageSizes: readonly LayoutPageSize[],
  pageIndex: number,
  layout: PageLayout,
  rotation: Rotation,
): SpreadExtent | undefined {
  if (pageSizes.length === 0) return undefined;

  const safeIndex = pageIndex >= 0 && pageIndex < pageSizes.length ? pageIndex : 0;
  const start = spreadStartForPage(safeIndex, layout);
  const end = Math.min(pageSizes.length, start + (layout === "dual" ? 2 : 1));
  const rotated = rotation === 90 || rotation === 270;
  let width = 0;
  let height = 0;

  for (let index = start; index < end; index += 1) {
    const page = pageSizes[index]!;
    const pageWidth = rotated ? page.height : page.width;
    const pageHeight = rotated ? page.width : page.height;
    width += pageWidth;
    height = Math.max(height, pageHeight);
  }

  return { width, height, pageCount: end - start };
}

interface FitSpreadOptions {
  containerWidth: number;
  containerHeight?: number;
  cssPixelsPerPoint: number;
  horizontalInset: number;
  verticalInset?: number;
}

/** Return the zoom that fits an entire spread within the supplied viewport. */
export function fittedZoomForSpread(
  spread: SpreadExtent,
  options: FitSpreadOptions,
): number {
  const gutter = spread.pageCount > 1 ? DUAL_PAGE_GAP_PX : 0;
  const availableWidth = Math.max(1, options.containerWidth - options.horizontalInset - gutter);
  const widthZoom = availableWidth / Math.max(1, spread.width * options.cssPixelsPerPoint);
  if (options.containerHeight === undefined) return widthZoom;

  const availableHeight = Math.max(
    1,
    options.containerHeight - (options.verticalInset ?? 0),
  );
  const heightZoom = availableHeight / Math.max(1, spread.height * options.cssPixelsPerPoint);
  return Math.min(widthZoom, heightZoom);
}

/**
 * Select the page nearest a vertical viewport probe. Equal-distance pages in a
 * spread retain the explicitly active page; otherwise the lower index wins so
 * the result never depends on IntersectionObserver callback order.
 */
export function selectPageAtVerticalProbe(
  candidates: readonly PageVerticalBounds[],
  probeY: number,
  activePage: number,
): number | undefined {
  let bestIndex: number | undefined;
  let bestDistance = Number.POSITIVE_INFINITY;

  for (const candidate of candidates) {
    const distance = probeY < candidate.top
      ? candidate.top - probeY
      : probeY > candidate.bottom
        ? probeY - candidate.bottom
        : 0;
    if (
      distance < bestDistance
      || (
        distance === bestDistance
        && (
          candidate.index === activePage
          || (bestIndex !== activePage && (bestIndex === undefined || candidate.index < bestIndex))
        )
      )
    ) {
      bestDistance = distance;
      bestIndex = candidate.index;
    }
  }

  return bestIndex;
}
