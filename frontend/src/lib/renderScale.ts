/** Keep full-page backing stores below the renderer's memory ceilings. */
export const MAX_RENDER_PIXELS = 6_000_000;
export const MAX_RENDER_DIMENSION = 16_384;
/** Mirrors Rust's bounded, exactly-addressable virtual page edge for tiling. */
export const MAX_VIRTUAL_RENDER_DIMENSION = 262_144;

/** Default edge length for full-resolution page tiles. */
export const DEFAULT_RENDER_TILE_SIZE = 1_024;

/**
 * Prevent malformed page metadata from allocating an unbounded array of tile
 * descriptors before the backend has a chance to reject the request.
 */
export const MAX_RENDER_TILES = 65_536;

/** Full bitmap geometry at the exact float32 scale sent to the Rust renderer. */
export interface RequestedRenderDimensions {
  /** Device-pixel scale per PDF point. */
  scale: number;
  pixelWidth: number;
  pixelHeight: number;
}

export interface CssPageDimensions {
  width: number;
  height: number;
}

/** One device-pixel rectangle within the full requested page bitmap. */
export interface RenderTile {
  index: number;
  row: number;
  column: number;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A complete row-major partition of a requested bitmap. */
export interface RenderTileGrid {
  tileSize: number;
  columns: number;
  rows: number;
  tiles: RenderTile[];
}

export interface TiledRenderPlan extends RequestedRenderDimensions, RenderTileGrid {}

export interface NormalizedPagePoint {
  nx: number;
  ny: number;
}

/** Half-open device-pixel bounds within a complete virtual page raster. */
export interface RenderPixelBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Inclusive row/column range for direct row-major tile lookup. */
export interface RenderTileRange {
  minColumn: number;
  maxColumn: number;
  minRow: number;
  maxRow: number;
}

const FLOAT32_KEY_VALUE = new Float32Array(1);
const FLOAT32_KEY_BITS = new Uint32Array(FLOAT32_KEY_VALUE.buffer);

/** Return the exact IEEE-754 binary32 identity used at the Rust IPC boundary. */
export function float32Key(value: number): string {
  FLOAT32_KEY_VALUE[0] = value;
  return FLOAT32_KEY_BITS[0]!.toString(16).padStart(8, "0");
}

/** Identify one complete virtual page raster without decimal scale rounding. */
export function renderRasterIdentity(
  scale: number,
  pixelWidth: number,
  pixelHeight: number,
): string {
  return `${float32Key(scale)}:${pixelWidth}x${pixelHeight}`;
}

/** Map a point in the rotated display rectangle back into the PDF page. */
export function normalizedPagePointForRotation(
  displayX: number,
  displayY: number,
  rotation: number,
): NormalizedPagePoint {
  const point = rotation === 90
    ? { nx: displayY, ny: 1 - displayX }
    : rotation === 180
      ? { nx: 1 - displayX, ny: 1 - displayY }
      : rotation === 270
        ? { nx: 1 - displayY, ny: displayX }
        : { nx: displayX, ny: displayY };
  return {
    nx: Math.max(0, Math.min(1, point.nx)),
    ny: Math.max(0, Math.min(1, point.ny)),
  };
}

/** Resolve only the rows and columns intersecting half-open pixel bounds. */
export function renderTileRangeForBounds(
  plan: TiledRenderPlan,
  bounds: RenderPixelBounds,
): RenderTileRange | undefined {
  if (
    !Number.isFinite(bounds.left)
    || !Number.isFinite(bounds.top)
    || !Number.isFinite(bounds.right)
    || !Number.isFinite(bounds.bottom)
  ) return undefined;

  const left = Math.max(0, Math.min(plan.pixelWidth, bounds.left));
  const top = Math.max(0, Math.min(plan.pixelHeight, bounds.top));
  const right = Math.max(0, Math.min(plan.pixelWidth, bounds.right));
  const bottom = Math.max(0, Math.min(plan.pixelHeight, bounds.bottom));
  if (right <= left || bottom <= top) return undefined;

  return {
    minColumn: Math.max(0, Math.floor(left / plan.tileSize)),
    maxColumn: Math.min(plan.columns - 1, Math.ceil(right / plan.tileSize) - 1),
    minRow: Math.max(0, Math.floor(top / plan.tileSize)),
    maxRow: Math.min(plan.rows - 1, Math.ceil(bottom / plan.tileSize) - 1),
  };
}

function requirePositiveFinite(name: string, value: number): void {
  if (!Number.isFinite(value) || value <= 0) {
    throw new RangeError(`${name} must be finite and positive`);
  }
}

function requirePositiveSafeInteger(name: string, value: number): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new RangeError(`${name} must be a positive safe integer`);
  }
}

/**
 * Return bounded layout geometry for a page whose render plan was rejected.
 * This keeps one malformed or exceptionally large page from tearing down the
 * document or creating an effectively unscrollable CSS box while its local
 * error message remains visible.
 */
export function fallbackCssPageDimensions(
  pageWidth: number,
  pageHeight: number,
  cssScale: number,
  devicePixelRatio: number,
): CssPageDimensions {
  const safeDpr = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0
    ? devicePixelRatio
    : 1;
  const rawWidth = pageWidth * cssScale;
  const rawHeight = pageHeight * cssScale;
  const width = Number.isFinite(rawWidth) && rawWidth > 0 ? Math.max(1, rawWidth) : 1;
  const height = Number.isFinite(rawHeight) && rawHeight > 0 ? Math.max(1, rawHeight) : 1;
  const maxCssEdge = MAX_VIRTUAL_RENDER_DIMENSION / safeDpr;
  const reduction = Math.min(1, maxCssEdge / Math.max(width, height));
  return {
    width: Math.max(1, width * reduction),
    height: Math.max(1, height * reduction),
  };
}

/**
 * Resolve the full device-pixel dimensions requested for a page without
 * applying the full-frame memory cap.
 *
 * `cssScale` is the number of CSS pixels per PDF point (semantic zoom times
 * 96/72). `devicePixelRatio` must be the actual browser DPR; it is deliberately
 * not capped here. The result mirrors Rust's f32 IPC conversion and dimension
 * rounding so every tile uses one consistent virtual page geometry.
 */
export function requestedRenderDimensions(
  pageWidth: number,
  pageHeight: number,
  cssScale: number,
  devicePixelRatio: number,
): RequestedRenderDimensions {
  requirePositiveFinite("pageWidth", pageWidth);
  requirePositiveFinite("pageHeight", pageHeight);
  requirePositiveFinite("cssScale", cssScale);
  requirePositiveFinite("devicePixelRatio", devicePixelRatio);

  const width = Math.fround(pageWidth);
  const height = Math.fround(pageHeight);
  const scale = Math.fround(cssScale * devicePixelRatio);

  if (!Number.isFinite(width) || width <= 0) {
    throw new RangeError("pageWidth must remain finite and positive as float32");
  }
  if (!Number.isFinite(height) || height <= 0) {
    throw new RangeError("pageHeight must remain finite and positive as float32");
  }
  if (!Number.isFinite(scale) || scale <= 0) {
    throw new RangeError("cssScale * devicePixelRatio must remain finite and positive as float32");
  }

  const pixelWidth = Math.max(1, Math.round(width * scale));
  const pixelHeight = Math.max(1, Math.round(height * scale));
  requirePositiveSafeInteger("pixelWidth", pixelWidth);
  requirePositiveSafeInteger("pixelHeight", pixelHeight);
  if (
    pixelWidth > MAX_VIRTUAL_RENDER_DIMENSION
    || pixelHeight > MAX_VIRTUAL_RENDER_DIMENSION
  ) {
    throw new RangeError(
      `virtual render dimensions must not exceed ${MAX_VIRTUAL_RENDER_DIMENSION}px per edge`,
    );
  }

  return { scale, pixelWidth, pixelHeight };
}

/** Partition an integer pixel rectangle into a complete, gap-free tile grid. */
export function createRenderTileGrid(
  pixelWidth: number,
  pixelHeight: number,
  tileSize = DEFAULT_RENDER_TILE_SIZE,
): RenderTileGrid {
  requirePositiveSafeInteger("pixelWidth", pixelWidth);
  requirePositiveSafeInteger("pixelHeight", pixelHeight);
  requirePositiveSafeInteger("tileSize", tileSize);

  if (tileSize > MAX_RENDER_DIMENSION || tileSize * tileSize > MAX_RENDER_PIXELS) {
    throw new RangeError(
      `tileSize must fit the ${MAX_RENDER_DIMENSION}px edge and ${MAX_RENDER_PIXELS}-pixel limits`,
    );
  }

  const columns = Math.ceil(pixelWidth / tileSize);
  const rows = Math.ceil(pixelHeight / tileSize);
  const tileCount = columns * rows;
  if (!Number.isSafeInteger(tileCount) || tileCount > MAX_RENDER_TILES) {
    throw new RangeError(`render plan exceeds the ${MAX_RENDER_TILES}-tile limit`);
  }

  const tiles: RenderTile[] = [];
  for (let row = 0; row < rows; row += 1) {
    const y = row * tileSize;
    const height = Math.min(tileSize, pixelHeight - y);
    for (let column = 0; column < columns; column += 1) {
      const x = column * tileSize;
      const width = Math.min(tileSize, pixelWidth - x);
      tiles.push({
        index: tiles.length,
        row,
        column,
        x,
        y,
        width,
        height,
      });
    }
  }

  return { tileSize, columns, rows, tiles };
}

/** Build full-resolution virtual page geometry and its bounded tile grid. */
export function createTiledRenderPlan(
  pageWidth: number,
  pageHeight: number,
  cssScale: number,
  devicePixelRatio: number,
  tileSize = DEFAULT_RENDER_TILE_SIZE,
): TiledRenderPlan {
  const dimensions = requestedRenderDimensions(
    pageWidth,
    pageHeight,
    cssScale,
    devicePixelRatio,
  );
  return {
    ...dimensions,
    ...createRenderTileGrid(dimensions.pixelWidth, dimensions.pixelHeight, tileSize),
  };
}

/**
 * Return the largest float32 scale whose rounded bitmap dimensions fit.
 *
 * Rust receives the scale and page dimensions as f32, promotes them to f64,
 * rounds each edge, then checks their product. Mirroring those steps avoids a
 * continuous-area cap rounding a page a few pixels beyond the hard limit.
 */
export function boundedRenderScale(
  pageWidth: number,
  pageHeight: number,
  requestedScale: number,
): number {
  const width = Math.fround(pageWidth);
  const height = Math.fround(pageHeight);
  const requested = Math.fround(requestedScale);

  if (
    !Number.isFinite(width) || width <= 0 ||
    !Number.isFinite(height) || height <= 0 ||
    !Number.isFinite(requested) || requested <= 0
  ) {
    return requestedScale;
  }

  const fits = (candidate: number) => {
    const scale = Math.fround(candidate);
    const pixelWidth = Math.max(1, Math.round(width * scale));
    const pixelHeight = Math.max(1, Math.round(height * scale));
    return pixelWidth <= MAX_RENDER_DIMENSION &&
      pixelHeight <= MAX_RENDER_DIMENSION &&
      pixelWidth * pixelHeight <= MAX_RENDER_PIXELS;
  };

  if (fits(requested)) return requested;

  // Positive IEEE-754 float bit patterns have the same ordering as their
  // numeric values. Searching that finite domain finds the exact largest f32
  // even when the fitting scale is many orders of magnitude below the request.
  const values = new Float32Array(1);
  const bits = new Uint32Array(values.buffer);
  const toBits = (value: number) => {
    values[0] = value;
    return bits[0];
  };
  const fromBits = (value: number) => {
    bits[0] = value;
    return values[0];
  };

  let lowBits = 1; // Smallest positive f32 always rounds each edge to 1 px.
  let highBits = toBits(requested);
  while (lowBits < highBits) {
    const midpoint = lowBits + Math.floor((highBits - lowBits + 1) / 2);
    if (fits(fromBits(midpoint))) lowBits = midpoint;
    else highBits = midpoint - 1;
  }
  return fromBits(lowBits);
}
