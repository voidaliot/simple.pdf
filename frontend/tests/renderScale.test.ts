import assert from "node:assert/strict";
import { test } from "node:test";
import {
  boundedRenderScale,
  createRenderTileGrid,
  createTiledRenderPlan,
  DEFAULT_RENDER_TILE_SIZE,
  fallbackCssPageDimensions,
  float32Key,
  MAX_RENDER_DIMENSION,
  MAX_RENDER_PIXELS,
  MAX_RENDER_TILES,
  MAX_VIRTUAL_RENDER_DIMENSION,
  normalizedPagePointForRotation,
  renderRasterIdentity,
  renderTileRangeForBounds,
  requestedRenderDimensions,
} from "../src/lib/renderScale.ts";

function dimensions(width: number, height: number, scale: number) {
  const pixelWidth = Math.max(1, Math.round(Math.fround(width) * Math.fround(scale)));
  const pixelHeight = Math.max(1, Math.round(Math.fround(height) * Math.fround(scale)));
  return { pixelWidth, pixelHeight, pixels: pixelWidth * pixelHeight };
}

function nextFloat32(value: number) {
  const floats = new Float32Array(1);
  const bits = new Uint32Array(floats.buffer);
  floats[0] = value;
  bits[0] += 1;
  return floats[0];
}

test("backs the A4 rounding regression below the pixel budget", () => {
  const width = 595.2756;
  const height = 841.8898;
  const oldContinuousLimit = Math.sqrt(MAX_RENDER_PIXELS / (width * height));

  assert.deepEqual(dimensions(width, height, oldContinuousLimit), {
    pixelWidth: 2060,
    pixelHeight: 2913,
    pixels: 6_000_780,
  });

  const bounded = boundedRenderScale(width, height, oldContinuousLimit);
  assert.ok(dimensions(width, height, bounded).pixels <= MAX_RENDER_PIXELS);
  assert.ok(dimensions(width, height, nextFloat32(bounded)).pixels > MAX_RENDER_PIXELS);
});

test("preserves safe scales", () => {
  assert.equal(boundedRenderScale(612, 792, 1.5), 1.5);
});

test("also enforces the bitmap edge limit", () => {
  const bounded = boundedRenderScale(100_000, 100, 2);
  const result = dimensions(100_000, 100, bounded);
  assert.equal(result.pixelWidth, MAX_RENDER_DIMENSION);
  assert.ok(result.pixels <= MAX_RENDER_PIXELS);
});

test("finds a positive scale across the full float32 range", () => {
  const width = 1e13;
  const height = 100;
  const bounded = boundedRenderScale(width, height, 10.666666666666666);
  const result = dimensions(width, height, bounded);

  assert.ok(bounded > 0);
  assert.ok(result.pixelWidth <= MAX_RENDER_DIMENSION);
  assert.ok(result.pixels <= MAX_RENDER_PIXELS);
  assert.ok(dimensions(width, height, nextFloat32(bounded)).pixelWidth > MAX_RENDER_DIMENSION);
});

test("computes full requested dimensions at the actual device pixel ratio", () => {
  const cssScale = 96 / 72;
  const result = requestedRenderDimensions(612, 792, cssScale, 2.5);

  assert.deepEqual(result, {
    scale: Math.fround(cssScale * 2.5),
    pixelWidth: 2040,
    pixelHeight: 2640,
  });
});

test("tiled plans preserve high-zoom resolution instead of applying the full-frame cap", () => {
  const cssScale = 4 * 96 / 72;
  const requestedScale = Math.fround(cssScale * 2);
  const plan = createTiledRenderPlan(612, 792, cssScale, 2);

  assert.equal(plan.scale, requestedScale);
  assert.equal(plan.pixelWidth, 6528);
  assert.equal(plan.pixelHeight, 8448);
  assert.ok(plan.pixelWidth * plan.pixelHeight > MAX_RENDER_PIXELS);
  assert.ok(plan.scale > boundedRenderScale(612, 792, requestedScale));
  assert.equal(plan.tileSize, DEFAULT_RENDER_TILE_SIZE);
  assert.equal(plan.columns, 7);
  assert.equal(plan.rows, 9);
  assert.equal(plan.tiles.length, 63);
});

test("tile grids cover every pixel exactly once with bounded edge tiles", () => {
  const grid = createRenderTileGrid(2500, 2050);

  assert.equal(grid.columns, 3);
  assert.equal(grid.rows, 3);
  assert.deepEqual(grid.tiles.at(-1), {
    index: 8,
    row: 2,
    column: 2,
    x: 2048,
    y: 2048,
    width: 452,
    height: 2,
  });

  let coveredPixels = 0;
  for (const tile of grid.tiles) {
    assert.equal(tile.index, tile.row * grid.columns + tile.column);
    assert.equal(tile.x, tile.column * grid.tileSize);
    assert.equal(tile.y, tile.row * grid.tileSize);
    assert.ok(tile.width > 0 && tile.width <= grid.tileSize);
    assert.ok(tile.height > 0 && tile.height <= grid.tileSize);
    assert.ok(tile.width <= MAX_RENDER_DIMENSION);
    assert.ok(tile.height <= MAX_RENDER_DIMENSION);
    assert.ok(tile.width * tile.height <= MAX_RENDER_PIXELS);
    assert.ok(tile.x + tile.width <= 2500);
    assert.ok(tile.y + tile.height <= 2050);
    coveredPixels += tile.width * tile.height;
  }
  assert.equal(coveredPixels, 2500 * 2050);
});

test("tile grids handle pages smaller than one tile", () => {
  assert.deepEqual(createRenderTileGrid(17, 23), {
    tileSize: DEFAULT_RENDER_TILE_SIZE,
    columns: 1,
    rows: 1,
    tiles: [{ index: 0, row: 0, column: 0, x: 0, y: 0, width: 17, height: 23 }],
  });
});

test("requested dimensions reject invalid or unrepresentable inputs", () => {
  const invalidInputs: Array<[number, number, number, number]> = [
    [0, 792, 1, 1],
    [612, -1, 1, 1],
    [612, 792, Number.NaN, 1],
    [612, 792, 1, Number.POSITIVE_INFINITY],
    [Number.MIN_VALUE, 792, 1, 1],
    [612, 792, Number.MIN_VALUE, 1],
    [Number.MAX_VALUE, 792, 1, 1],
    [1e13, 792, 1_000, 1],
  ];

  for (const args of invalidInputs) {
    assert.throws(() => requestedRenderDimensions(...args), RangeError);
  }

  assert.equal(
    requestedRenderDimensions(MAX_VIRTUAL_RENDER_DIMENSION, 1, 1, 1).pixelWidth,
    MAX_VIRTUAL_RENDER_DIMENSION,
  );
  assert.throws(
    () => requestedRenderDimensions(MAX_VIRTUAL_RENDER_DIMENSION + 1, 1, 1, 1),
    RangeError,
  );
});

test("fallback CSS geometry remains bounded when a render plan is rejected", () => {
  assert.deepEqual(fallbackCssPageDimensions(1_000_000, 500_000, 1, 2), {
    width: MAX_VIRTUAL_RENDER_DIMENSION / 2,
    height: MAX_VIRTUAL_RENDER_DIMENSION / 4,
  });
  assert.deepEqual(fallbackCssPageDimensions(Number.NaN, Number.POSITIVE_INFINITY, 1, 2), {
    width: 1,
    height: 1,
  });
});

test("tile grids reject invalid tile geometry and unbounded descriptor counts", () => {
  for (const tileSize of [0, -1, 1.5, Number.NaN, Math.floor(Math.sqrt(MAX_RENDER_PIXELS)) + 1]) {
    assert.throws(() => createRenderTileGrid(100, 100, tileSize), RangeError);
  }
  assert.throws(() => createRenderTileGrid(0, 100), RangeError);
  assert.throws(() => createRenderTileGrid(100, Number.POSITIVE_INFINITY), RangeError);
  assert.throws(
    () => createRenderTileGrid((MAX_RENDER_TILES + 1) * DEFAULT_RENDER_TILE_SIZE, 1),
    RangeError,
  );
});

test("float32 render keys distinguish adjacent scales hidden by decimal formatting", () => {
  const scale = Math.fround(1);
  const adjacentScale = nextFloat32(scale);

  assert.equal(scale.toFixed(5), adjacentScale.toFixed(5));
  assert.notEqual(float32Key(scale), float32Key(adjacentScale));
  assert.notEqual(
    renderRasterIdentity(scale, 612, 792),
    renderRasterIdentity(adjacentScale, 612, 792),
  );
});

test("render raster identity includes both full raster dimensions", () => {
  const scale = Math.fround(2.5);
  const identity = renderRasterIdentity(scale, 1530, 1980);

  assert.notEqual(identity, renderRasterIdentity(scale, 1531, 1980));
  assert.notEqual(identity, renderRasterIdentity(scale, 1530, 1981));
});

test("maps rotated display points back to normalized page coordinates", () => {
  assert.deepEqual(normalizedPagePointForRotation(0.75, 0.25, 0), { nx: 0.75, ny: 0.25 });
  assert.deepEqual(normalizedPagePointForRotation(0.75, 0.25, 90), { nx: 0.25, ny: 0.25 });
  assert.deepEqual(normalizedPagePointForRotation(0.75, 0.25, 180), { nx: 0.25, ny: 0.75 });
  assert.deepEqual(normalizedPagePointForRotation(0.75, 0.25, 270), { nx: 0.75, ny: 0.75 });
});

test("resolves half-open viewport bounds to a direct bounded tile range", () => {
  const plan = {
    scale: 1,
    pixelWidth: 3000,
    pixelHeight: 2500,
    ...createRenderTileGrid(3000, 2500),
  };

  assert.deepEqual(
    renderTileRangeForBounds(plan, { left: 1024, top: 100, right: 2048, bottom: 2048 }),
    { minColumn: 1, maxColumn: 1, minRow: 0, maxRow: 1 },
  );
  assert.deepEqual(
    renderTileRangeForBounds(plan, { left: -500, top: 2050, right: 500, bottom: 3000 }),
    { minColumn: 0, maxColumn: 0, minRow: 2, maxRow: 2 },
  );
  assert.equal(
    renderTileRangeForBounds(plan, { left: 100, top: 100, right: 100, bottom: 200 }),
    undefined,
  );
});
