/** Keep full-page backing stores below the renderer's memory ceilings. */
export const MAX_RENDER_PIXELS = 6_000_000;
export const MAX_RENDER_DIMENSION = 16_384;

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
