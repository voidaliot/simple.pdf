<script lang="ts">
  import { onDestroy } from "svelte";
  import {
    requestPageFrame,
    type PageFrame,
    type PageFrameRequest,
  } from "../lib/pageRenderCache";
  import { renderRasterIdentity, type RenderTile } from "../lib/renderScale";

  interface Props {
    docId: string;
    pageIndex: number;
    scale: number;
    tile: RenderTile;
    fullWidth: number;
    fullHeight: number;
    cssWidth: number;
    cssHeight: number;
    annotationsVersion: number;
    retryVersion: number;
    priority: boolean;
    onRenderError?: (tileIdentity: string, message: string) => void;
  }

  let {
    docId,
    pageIndex,
    scale,
    tile,
    fullWidth,
    fullHeight,
    cssWidth,
    cssHeight,
    annotationsVersion,
    retryVersion,
    priority,
    onRenderError,
  }: Props = $props();

  const left = $derived(tile.x / fullWidth * cssWidth);
  const top = $derived(tile.y / fullHeight * cssHeight);
  const right = $derived((tile.x + tile.width) / fullWidth * cssWidth);
  const bottom = $derived((tile.y + tile.height) / fullHeight * cssHeight);
  const displayWidth = $derived(right - left);
  const displayHeight = $derived(bottom - top);

  let canvasEl = $state<HTMLCanvasElement | undefined>();
  let hasContent = $state(false);
  let generation = 0;
  let lastPaintedKey = "";
  const rasterIdentity = $derived(renderRasterIdentity(scale, fullWidth, fullHeight));
  const tileIdentity = $derived(
    `${rasterIdentity}:${tile.x},${tile.y},${tile.width},${tile.height}`,
  );

  function releaseCanvas() {
    if (!canvasEl) return;
    canvasEl.width = 1;
    canvasEl.height = 1;
    hasContent = false;
    lastPaintedKey = "";
  }

  function paint(frame: PageFrame) {
    const canvas = canvasEl;
    if (!canvas) return;
    canvas.width = frame.width;
    canvas.height = frame.height;
    const context = canvas.getContext("2d", { alpha: false });
    if (!context) {
      onRenderError?.(tileIdentity, "Canvas rendering is unavailable");
      return;
    }
    context.putImageData(new ImageData(frame.data, frame.width, frame.height), 0, 0);
    hasContent = true;
    onRenderError?.(tileIdentity, "");
  }

  onDestroy(() => {
    onRenderError?.(tileIdentity, "");
    releaseCanvas();
  });

  $effect(() => {
    const canvas = canvasEl;
    if (!canvas) return;

    const region = tile;
    const key = [
      docId,
      pageIndex,
      tileIdentity,
      annotationsVersion,
      retryVersion,
    ].join(":");
    if (lastPaintedKey === key) return;
    // The parent keys this component by the complete raster and tile geometry,
    // but clear defensively before every replacement request as well. Do not
    // read `hasContent` here: doing so would make paint state an effect input.
    releaseCanvas();
    onRenderError?.(tileIdentity, "");

    const requestPriority = priority ? 0 : 1;
    const currentGeneration = ++generation;
    let cancelled = false;
    let request: PageFrameRequest | undefined;
    const timer = setTimeout(() => {
      if (cancelled) return;
      request = requestPageFrame({
        docId,
        pageIndex,
        scale,
        fullWidth,
        fullHeight,
        version: annotationsVersion,
        retryVersion,
        tile: {
          x: region.x,
          y: region.y,
          width: region.width,
          height: region.height,
        },
        priority: requestPriority,
      });
      request.promise
        .then((frame) => {
          if (cancelled || currentGeneration !== generation) return;
          lastPaintedKey = key;
          paint(frame);
        })
        .catch((error: unknown) => {
          if (cancelled || currentGeneration !== generation) return;
          onRenderError?.(
            tileIdentity,
            error instanceof Error ? error.message : String(error),
          );
        });
    }, requestPriority === 0 ? 0 : 40);

    return () => {
      cancelled = true;
      clearTimeout(timer);
      request?.cancel();
    };
  });
</script>

<div
  class="page-tile"
  style:left="{left}px"
  style:top="{top}px"
  style:width="{displayWidth}px"
  style:height="{displayHeight}px"
  data-tile-index={tile.index}
>
  <canvas
    bind:this={canvasEl}
    class:visible={hasContent}
    style:width="{displayWidth}px"
    style:height="{displayHeight}px"
    aria-hidden="true"
  ></canvas>
</div>

<style>
  .page-tile {
    position: absolute;
    overflow: hidden;
    pointer-events: none;
  }

  canvas {
    position: absolute;
    inset: 0;
    display: block;
    opacity: 0;
  }

  canvas.visible { opacity: 1; }
</style>
