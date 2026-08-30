import assert from "node:assert/strict";
import { test } from "node:test";
import {
  boundedRenderScale,
  MAX_RENDER_DIMENSION,
  MAX_RENDER_PIXELS,
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
