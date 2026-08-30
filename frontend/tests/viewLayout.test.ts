import assert from "node:assert/strict";
import { test } from "node:test";
import {
  fittedZoomForSpread,
  selectPageAtVerticalProbe,
  spreadExtentForPage,
  spreadStartForPage,
} from "../src/lib/viewLayout.ts";

const pages = [
  { width: 600, height: 800 },
  { width: 500, height: 700 },
  { width: 400, height: 900 },
];

test("groups dual pages into consecutive spreads", () => {
  assert.equal(spreadStartForPage(0, "dual"), 0);
  assert.equal(spreadStartForPage(1, "dual"), 0);
  assert.equal(spreadStartForPage(2, "dual"), 2);
  assert.equal(spreadStartForPage(3, "single"), 3);

  const first = spreadExtentForPage(pages, 0, "dual", 0);
  assert.deepEqual(first, { width: 1_100, height: 800, pageCount: 2 });
  assert.deepEqual(spreadExtentForPage(pages, 1, "dual", 0), first);
});

test("uses rotated page extents for the whole spread", () => {
  assert.deepEqual(spreadExtentForPage(pages, 0, "dual", 90), {
    width: 1_500,
    height: 600,
    pageCount: 2,
  });
});

test("keeps single pages and an odd dual-page tail as one-page spreads", () => {
  assert.deepEqual(spreadExtentForPage(pages, 1, "single", 0), {
    width: 500,
    height: 700,
    pageCount: 1,
  });
  assert.deepEqual(spreadExtentForPage(pages, 2, "dual", 0), {
    width: 400,
    height: 900,
    pageCount: 1,
  });
});

test("fit width accounts for the two-page gutter", () => {
  const spread = spreadExtentForPage(
    [{ width: 600, height: 800 }, { width: 600, height: 800 }],
    1,
    "dual",
    0,
  );
  assert.ok(spread);
  assert.equal(fittedZoomForSpread(spread, {
    containerWidth: 1_644,
    cssPixelsPerPoint: 96 / 72,
    horizontalInset: 32,
  }), 1);
});

test("fit page constrains a spread by its tallest page", () => {
  const spread = spreadExtentForPage(
    [{ width: 600, height: 800 }, { width: 600, height: 700 }],
    0,
    "dual",
    0,
  );
  assert.ok(spread);
  const zoom = fittedZoomForSpread(spread, {
    containerWidth: 2_044,
    containerHeight: 848,
    cssPixelsPerPoint: 96 / 72,
    horizontalInset: 32,
    verticalInset: 48,
  });
  assert.ok(Math.abs(zoom - 0.75) < Number.EPSILON * 4);
});

test("equal-distance spread pages retain the explicitly active page", () => {
  const spread = [
    { index: 0, top: 100, bottom: 900 },
    { index: 1, top: 100, bottom: 900 },
  ];

  assert.equal(selectPageAtVerticalProbe(spread, 300, 1), 1);
  assert.equal(selectPageAtVerticalProbe([...spread].reverse(), 300, 1), 1);
});

test("page selection uses a deterministic lower-index fallback and nearest row", () => {
  const candidates = [
    { index: 3, top: 1_000, bottom: 1_800 },
    { index: 2, top: 1_000, bottom: 1_800 },
    { index: 0, top: 100, bottom: 900 },
  ];

  assert.equal(selectPageAtVerticalProbe(candidates, 1_100, 0), 2);
  assert.equal(selectPageAtVerticalProbe(candidates, 850, 0), 0);
  assert.equal(selectPageAtVerticalProbe([], 300, 0), undefined);
});
