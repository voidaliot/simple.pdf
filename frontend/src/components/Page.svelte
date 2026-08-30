<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { type TextSpan, type Annotation, type AnnRect, type FormField, type LinkTarget } from "../lib/ipc";
  import { requestPageFrame, type PageFrame, type PageFrameRequest } from "../lib/pageRenderCache";
  import {
    boundedRenderScale,
    createTiledRenderPlan,
    fallbackCssPageDimensions,
    MAX_RENDER_DIMENSION,
    MAX_RENDER_PIXELS,
    normalizedPagePointForRotation,
    renderRasterIdentity,
    renderTileRangeForBounds,
    type RenderPixelBounds,
    type RenderTile,
    type TiledRenderPlan,
  } from "../lib/renderScale";
  import { CSS_PIXELS_PER_POINT } from "../stores/viewer.svelte";
  import PageTile from "./PageTile.svelte";

  export interface Highlight {
    left: number;
    top: number;
    width: number;
    height: number;
    active?: boolean;
  }

  // ── Named constants ──────────────────────────────────────────────────────────
  /** Short delay lets a visible page start before its prefetch neighbours. */
  const PREFETCH_RENDER_DELAY_MS = 40;
  /** Visible/cache-hit work should paint in the current event-loop turn. */
  const VISIBLE_RENDER_DELAY_MS = 0;
  type AnnotTool = "none" | "highlight" | "underline" | "strikeout" | "text" | "ink";

  interface Props {
    /** Document ID. */
    docId: string;
    /** Zero-based page index. */
    pageIndex: number;
    /** Page width in PDF points. */
    width: number;
    /** Page height in PDF points. */
    height: number;
    /** Semantic zoom factor where 1.0 means 100% (96 CSS dpi). */
    zoom: number;
    /** Current browser device-pixel ratio; changes when moving between monitors. */
    devicePixelRatio?: number;
    /** Invalidates tile visibility geometry when the page row layout changes. */
    pageLayout?: "single" | "dual";
    /** Whether this page is currently in the viewport or prefetch zone. */
    visible: boolean;
    /** True when the page intersects the real viewport (not only the prefetch zone). */
    priority?: boolean;
    /** Visual rotation in degrees (0 | 90 | 180 | 270). */
    rotation?: number;
    textSpans?: TextSpan[];
    highlights?: Highlight[];
    /** Index of the active find match within this page's highlights. */
    activeHighlight?: number;
    annotations?: Annotation[];
    /** Bump whenever annotations change to force a re-render (PDFium bakes annotations). */
    annotationsVersion?: number;
    formFields?: FormField[];
    xfaReadOnly?: boolean;
    activeTool?: AnnotTool;
    inkColor?: [number, number, number];
    inkWidth?: number;
    onPageClick?: (left: number, top: number) => void;
    onTextSelected?: (rects: AnnRect[]) => void;
    onInkStroke?: (paths: [number, number][][]) => void;
    onDeleteAnnotation?: (annotIndex: number) => void;
    onLinkActivate?: (target: LinkTarget) => void;
    onFieldText?: (annotIndex: number, value: string) => void;
    onFieldChecked?: (annotIndex: number, checked: boolean) => void;
    onPushButton?: (annotIndex: number) => void;
  }

  let {
    docId, pageIndex, width, height, zoom,
    devicePixelRatio = window.devicePixelRatio || 1,
    pageLayout = "single",
    visible, priority = false,
    rotation = 0,
    textSpans, highlights, activeHighlight = -1,
    annotations,
    annotationsVersion = 0,
    formFields,
    xfaReadOnly = false,
    activeTool = "none",
    inkColor = [255, 0, 0],
    inkWidth = 2,
    onPageClick, onTextSelected, onInkStroke, onDeleteAnnotation,
    onLinkActivate,
    onFieldText, onFieldChecked, onPushButton,
  }: Props = $props();

  interface RenderPlanResult {
    plan?: TiledRenderPlan;
    error: string;
  }

  interface ActiveRenderTile {
    tile: RenderTile;
    priority: boolean;
  }

  function errorMessage(error: unknown): string {
    return error instanceof Error ? error.message : String(error);
  }

  function safeCssDimension(points: number, scale: number): number {
    const value = points * scale;
    return Number.isFinite(value) && value > 0 ? Math.max(1, value) : 1;
  }

  const dpr = $derived(
    Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1,
  );
  const cssScale = $derived(zoom * CSS_PIXELS_PER_POINT);
  const renderPlanResult = $derived.by((): RenderPlanResult => {
    try {
      return { plan: createTiledRenderPlan(width, height, cssScale, dpr), error: "" };
    } catch (error) {
      return { error: errorMessage(error) };
    }
  });
  const renderPlan = $derived(renderPlanResult.plan);
  const planError = $derived(renderPlanResult.error);
  const fallbackCssSize = $derived(
    fallbackCssPageDimensions(width, height, cssScale, dpr),
  );
  const tiledRender = $derived(
    renderPlan !== undefined && (
      renderPlan.pixelWidth > MAX_RENDER_DIMENSION
        || renderPlan.pixelHeight > MAX_RENDER_DIMENSION
        || renderPlan.pixelWidth * renderPlan.pixelHeight > MAX_RENDER_PIXELS
    ),
  );

  // Derive CSS edges from the exact backing dimensions. This gives every
  // canvas exactly `dpr` backing pixels per CSS pixel and avoids a one-pixel
  // browser resample caused by rounding the two sizes independently.
  const cssW = $derived(
    renderPlan ? renderPlan.pixelWidth / dpr : fallbackCssSize.width,
  );
  const cssH = $derived(
    renderPlan ? renderPlan.pixelHeight / dpr : fallbackCssSize.height,
  );
  const rasterIdentity = $derived(
    renderPlan
      ? renderRasterIdentity(renderPlan.scale, renderPlan.pixelWidth, renderPlan.pixelHeight)
      : "invalid",
  );
  const inkRenderScale = $derived(
    (() => {
      const value = boundedRenderScale(width, height, cssScale * Math.min(dpr, 2));
      return Number.isFinite(value) && value > 0 ? value : 1;
    })(),
  );
  const inkPixelWidth = $derived(
    Math.max(1, Math.round(safeCssDimension(Math.fround(width), inkRenderScale))),
  );
  const inkPixelHeight = $derived(
    Math.max(1, Math.round(safeCssDimension(Math.fround(height), inkRenderScale))),
  );

  // Post-rotation layout dimensions
  const isRotated = $derived(rotation === 90 || rotation === 270);
  const displayW = $derived(isRotated ? cssH : cssW);
  const displayH = $derived(isRotated ? cssW : cssH);
  const innerTop = $derived((displayH - cssH) / 2);
  const innerLeft = $derived((displayW - cssW) / 2);

  // ── Canvas render state ──────────────────────────────────────────────────────
  /** The page canvas — always in the DOM so bind:this is valid in the effect. */
  let canvasEl = $state<HTMLCanvasElement | undefined>();
  let pageWrapperEl = $state<HTMLDivElement | undefined>();
  let pageInnerEl = $state<HTMLDivElement | undefined>();
  /** True once the canvas has been painted at least once. */
  let hasContent = $state(false);
  /** Non-empty when the last render failed. */
  let renderError = $state("");
  /** True while a network render is in-flight (not a cache hit). */
  let rendering = $state(false);
  /** Incremented by the retry action to re-run the render effect. */
  let retryVersion = $state(0);
  let activeTiles = $state<ActiveRenderTile[]>([]);
  let tileErrors = $state<Map<string, string>>(new Map());
  let renderGeneration = 0;
  let lastPaintedKey = "";
  let virtualizerFrame: number | undefined;

  const tileRenderError = $derived(tileErrors.values().next().value ?? "");
  const displayedRenderError = $derived(planError || renderError || tileRenderError);

  function handleTileRenderError(tileIdentity: string, message: string) {
    const next = new Map(tileErrors);
    if (message) next.set(tileIdentity, message);
    else next.delete(tileIdentity);
    tileErrors = next;
  }

  function normalizedPagePoint(
    clientX: number,
    clientY: number,
    pageRect: DOMRectReadOnly,
  ) {
    const displayX = (clientX - pageRect.left) / Math.max(1, pageRect.width);
    const displayY = (clientY - pageRect.top) / Math.max(1, pageRect.height);
    return normalizedPagePointForRotation(displayX, displayY, rotation);
  }

  function rasterBounds(
    pageRect: DOMRectReadOnly,
    viewportRect: DOMRectReadOnly,
    plan: TiledRenderPlan,
    margin: number,
  ): RenderPixelBounds | undefined {
    const clippedLeft = Math.max(pageRect.left, viewportRect.left - margin);
    const clippedTop = Math.max(pageRect.top, viewportRect.top - margin);
    const clippedRight = Math.min(pageRect.right, viewportRect.right + margin);
    const clippedBottom = Math.min(pageRect.bottom, viewportRect.bottom + margin);
    if (clippedRight <= clippedLeft || clippedBottom <= clippedTop) return undefined;

    const corners = [
      normalizedPagePoint(clippedLeft, clippedTop, pageRect),
      normalizedPagePoint(clippedRight, clippedTop, pageRect),
      normalizedPagePoint(clippedRight, clippedBottom, pageRect),
      normalizedPagePoint(clippedLeft, clippedBottom, pageRect),
    ];
    const left = Math.max(0, Math.floor(Math.min(...corners.map((point) => point.nx)) * plan.pixelWidth));
    const top = Math.max(0, Math.floor(Math.min(...corners.map((point) => point.ny)) * plan.pixelHeight));
    const right = Math.min(plan.pixelWidth, Math.ceil(Math.max(...corners.map((point) => point.nx)) * plan.pixelWidth));
    const bottom = Math.min(plan.pixelHeight, Math.ceil(Math.max(...corners.map((point) => point.ny)) * plan.pixelHeight));
    return right > left && bottom > top ? { left, top, right, bottom } : undefined;
  }

  function intersects(tile: RenderTile, bounds: RenderPixelBounds | undefined): boolean {
    return bounds !== undefined
      && tile.x < bounds.right
      && tile.x + tile.width > bounds.left
      && tile.y < bounds.bottom
      && tile.y + tile.height > bounds.top;
  }

  function updateActiveTiles() {
    const wrapper = pageWrapperEl;
    const inner = pageInnerEl;
    const plan = renderPlan;
    const viewport = wrapper?.closest<HTMLElement>(".pages-area");
    if (!visible || !tiledRender || !wrapper || !inner || !plan || !viewport) {
      if (activeTiles.length > 0) activeTiles = [];
      return;
    }

    const pageRect = inner.getBoundingClientRect();
    const viewportRect = viewport.getBoundingClientRect();
    const visibleBounds = rasterBounds(pageRect, viewportRect, plan, 0);
    // Keep one tile beyond every visible edge. This bounds live
    // canvases by viewport area while still rendering the next scroll step.
    const requestedBounds = rasterBounds(
      pageRect,
      viewportRect,
      plan,
      plan.tileSize / dpr,
    );
    if (!requestedBounds) {
      if (activeTiles.length > 0) activeTiles = [];
      return;
    }

    const range = renderTileRangeForBounds(plan, requestedBounds);
    if (!range) {
      if (activeTiles.length > 0) activeTiles = [];
      return;
    }
    const next: ActiveRenderTile[] = [];
    for (let row = range.minRow; row <= range.maxRow; row += 1) {
      for (let column = range.minColumn; column <= range.maxColumn; column += 1) {
        const tile = plan.tiles[row * plan.columns + column];
        if (tile) next.push({ tile, priority: intersects(tile, visibleBounds) });
      }
    }

    const unchanged = next.length === activeTiles.length && next.every((item, index) => {
      const current = activeTiles[index];
      return current?.tile.index === item.tile.index && current.priority === item.priority;
    });
    if (!unchanged) activeTiles = next;
  }

  function scheduleTileWindowUpdate() {
    if (virtualizerFrame !== undefined) return;
    virtualizerFrame = requestAnimationFrame(() => {
      virtualizerFrame = undefined;
      updateActiveTiles();
    });
  }

  onMount(() => {
    const wrapper = pageWrapperEl;
    const viewport = wrapper?.closest<HTMLElement>(".pages-area");
    if (!wrapper || !viewport) return;
    const observer = typeof ResizeObserver === "undefined"
      ? undefined
      : new ResizeObserver(scheduleTileWindowUpdate);
    observer?.observe(viewport);
    observer?.observe(wrapper);
    viewport.addEventListener("scroll", scheduleTileWindowUpdate, { passive: true });
    window.addEventListener("resize", scheduleTileWindowUpdate, { passive: true });
    scheduleTileWindowUpdate();
    return () => {
      observer?.disconnect();
      viewport.removeEventListener("scroll", scheduleTileWindowUpdate);
      window.removeEventListener("resize", scheduleTileWindowUpdate);
      if (virtualizerFrame !== undefined) cancelAnimationFrame(virtualizerFrame);
      virtualizerFrame = undefined;
    };
  });

  $effect(() => {
    // Establish all geometry inputs as dependencies, then defer layout reads to
    // one animation frame so transforms and CSS dimensions have been applied.
    renderPlan;
    tiledRender;
    rotation;
    pageLayout;
    dpr;
    visible;
    pageWrapperEl;
    pageInnerEl;
    scheduleTileWindowUpdate();
  });

  $effect(() => {
    // Errors belong to one exact raster, annotation, and retry generation.
    rasterIdentity;
    annotationsVersion;
    retryVersion;
    renderError = "";
    tileErrors = new Map();
  });

  /** Paint a cached or freshly-rendered frame onto the canvas. */
  function paint(frame: PageFrame) {
    const canvas = canvasEl;
    if (!canvas) return;
    // Setting canvas.width clears the buffer; putImageData follows in the
    // same microtask so the browser never paints a blank frame.
    canvas.width = frame.width;
    canvas.height = frame.height;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      rendering = false;
      renderError = "Canvas rendering is unavailable";
      return;
    }
    ctx.putImageData(new ImageData(frame.data, frame.width, frame.height), 0, 0);
    hasContent = true;
    renderError = "";
    rendering = false;
  }

  function releaseCanvas() {
    const canvas = canvasEl;
    if (!canvas) return;
    canvas.width = 1;
    canvas.height = 1;
    hasContent = false;
    rendering = false;
    lastPaintedKey = "";
  }

  // Explicitly relinquish WebView2's backing store instead of waiting for a
  // detached canvas to be discovered by garbage collection.
  onDestroy(releaseCanvas);

  function retryRender() {
    renderError = "";
    tileErrors = new Map();
    retryVersion += 1;
  }

  $effect(() => {
    const canvas = canvasEl;
    if (!canvas) return;

    if (!visible) {
      // Release large backing stores once a page is safely outside the prefetch
      // range. A short grace period prevents churn at observer boundaries.
      const releaseTimer = setTimeout(releaseCanvas, 1_500);
      return () => clearTimeout(releaseTimer);
    }

    const plan = renderPlan;
    if (!plan) {
      releaseCanvas();
      return;
    }

    // Large pages are rendered by the bounded tile window. A full-frame backing
    // store from an older raster must not remain attached at the new geometry.
    if (tiledRender) {
      releaseCanvas();
      return;
    }

    const id = docId;
    const idx = pageIndex;
    const scale = plan.scale;
    const frameKey = `${id}:${idx}:${rasterIdentity}:${annotationsVersion}:${retryVersion}`;
    if (lastPaintedKey === frameKey) {
      rendering = false;
      return;
    }

    // Reading priority here lets an entering page promote a queued prefetch
    // request. The shared scheduler deduplicates the identical frame key.
    const isPriority = priority;
    const delay = isPriority
      ? VISIBLE_RENDER_DELAY_MS
      : PREFETCH_RENDER_DELAY_MS;
    const generation = ++renderGeneration;
    let cancelled = false;
    let request: PageFrameRequest | undefined;
    rendering = true;

    // Keep the previous frame visible while the replacement is in flight.
    const renderTimer = setTimeout(() => {
      if (cancelled) return;

      request = requestPageFrame({
        docId: id,
        pageIndex: idx,
        scale,
        fullWidth: plan.pixelWidth,
        fullHeight: plan.pixelHeight,
        version: annotationsVersion,
        retryVersion,
        priority: isPriority ? 0 : 1,
      });
      request.promise
        .then((frame) => {
          if (cancelled || generation !== renderGeneration) return;
          lastPaintedKey = frameKey;
          paint(frame);
        })
        .catch((e: unknown) => {
          if (cancelled || generation !== renderGeneration) return;
          renderError = errorMessage(e);
          rendering = false;
        });
    }, delay);

    return () => {
      cancelled = true;
      clearTimeout(renderTimer);
      request?.cancel();
    };
  });

  // ── Ink drawing ──────────────────────────────────────────────────────────────
  let inkDrawing = $state(false);
  let inkCurrentPath = $state<[number, number][]>([]);
  let inkAllPaths = $state<[number, number][][]>([]);
  let inkCanvas: HTMLCanvasElement | undefined = $state();

  function inkStart(e: PointerEvent) {
    if (activeTool !== "ink" || !inkCanvas) return;
    inkDrawing = true;
    inkCanvas.setPointerCapture(e.pointerId);
    const { nx, ny } = normPos(e, inkCanvas);
    inkCurrentPath = [[nx, ny]];
    drawInk();
  }

  function inkMove(e: PointerEvent) {
    if (!inkDrawing || !inkCanvas) return;
    const { nx, ny } = normPos(e, inkCanvas);
    inkCurrentPath = [...inkCurrentPath, [nx, ny]];
    drawInk();
  }

  function inkEnd() {
    if (!inkDrawing) return;
    inkDrawing = false;
    if (inkCurrentPath.length > 1) inkAllPaths = [...inkAllPaths, inkCurrentPath];
    inkCurrentPath = [];
    drawInk();
  }

  function inkCommit() {
    if (inkAllPaths.length === 0) return;
    onInkStroke?.(inkAllPaths);
    inkAllPaths = [];
    inkCurrentPath = [];
    drawInk();
  }

  function drawInk() {
    if (!inkCanvas) return;
    const ctx = inkCanvas.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, inkCanvas.width, inkCanvas.height);
    const drawPaths = (paths: [number, number][][], color: string, lw: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lw;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      for (const path of paths) {
        if (path.length < 2) continue;
        ctx.beginPath();
        ctx.moveTo(path[0]![0] * cssW, path[0]![1] * cssH);
        for (let i = 1; i < path.length; i++) ctx.lineTo(path[i]![0] * cssW, path[i]![1] * cssH);
        ctx.stroke();
      }
    };
    const colorStr = `rgb(${inkColor.join(",")})`;
    drawPaths(inkAllPaths, colorStr, inkWidth);
    if (inkCurrentPath.length > 1) drawPaths([inkCurrentPath], colorStr, inkWidth);
  }

  $effect(() => {
    if (!inkCanvas) return;
    inkCanvas.width = inkPixelWidth;
    inkCanvas.height = inkPixelHeight;
    const ctx = inkCanvas.getContext("2d");
    if (ctx) ctx.scale(inkPixelWidth / cssW, inkPixelHeight / cssH);
    drawInk();
  });

  // ── Text selection → annotation rects ────────────────────────────────────────
  function clientToPage(clientX: number, clientY: number, pageEl: HTMLElement) {
    return normalizedPagePoint(clientX, clientY, pageEl.getBoundingClientRect());
  }

  function onPointerUp(e: PointerEvent) {
    if (!["highlight", "underline", "strikeout"].includes(activeTool)) return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) return;
    const range = sel.getRangeAt(0);
    const rects: AnnRect[] = [];
    const pageEl = (e.currentTarget as HTMLElement).parentElement;
    if (!pageEl) return;
    const pageRect = pageEl.getBoundingClientRect();
    for (const r of range.getClientRects()) {
      const clippedLeft = Math.max(r.left, pageRect.left);
      const clippedTop = Math.max(r.top, pageRect.top);
      const clippedRight = Math.min(r.right, pageRect.right);
      const clippedBottom = Math.min(r.bottom, pageRect.bottom);
      if (clippedRight <= clippedLeft || clippedBottom <= clippedTop) continue;
      const corners = [
        clientToPage(clippedLeft, clippedTop, pageEl),
        clientToPage(clippedRight, clippedTop, pageEl),
        clientToPage(clippedRight, clippedBottom, pageEl),
        clientToPage(clippedLeft, clippedBottom, pageEl),
      ];
      const left = Math.min(...corners.map((point) => point.nx));
      const top = Math.min(...corners.map((point) => point.ny));
      const right = Math.max(...corners.map((point) => point.nx));
      const bottom = Math.max(...corners.map((point) => point.ny));
      rects.push({
        left,
        top,
        width: right - left,
        height: bottom - top,
      });
    }
    if (rects.length > 0) onTextSelected?.(rects);
  }

  function normPos(e: PointerEvent, el: HTMLElement) {
    return clientToPage(e.clientX, e.clientY, el);
  }

  function capturePageClick(e: MouseEvent) {
    const pageEl = (e.currentTarget as HTMLElement).parentElement;
    if (!pageEl) return;
    const { nx, ny } = clientToPage(e.clientX, e.clientY, pageEl);
    onPageClick?.(nx, ny);
  }

</script>

<div
  class="page-wrapper"
  bind:this={pageWrapperEl}
  style:width="{displayW}px"
  style:height="{displayH}px"
  aria-label="Page {pageIndex + 1}"
  role="group"
>
  <div
    class="page-inner"
    bind:this={pageInnerEl}
    style:width="{cssW}px"
    style:height="{cssH}px"
    style:top="{innerTop}px"
    style:left="{innerLeft}px"
    style:transform="rotate({rotation}deg)"
  >
    <!--
      Page canvas — always mounted (not inside {#if}) so bind:this is always valid.
      CSS size matches logical dimensions; buffer size (canvas.width/height) is set
      by the render effect to match the device-pixel render.
      background: white prevents any transparent bleed-through.
    -->
    <div class="page-raster" role="img" aria-label="Rendered page {pageIndex + 1}">
      <canvas
        bind:this={canvasEl}
        class="page-canvas"
        class:page-canvas-visible={hasContent}
        style:width="{cssW}px"
        style:height="{cssH}px"
        aria-hidden="true"
      ></canvas>
      {#if tiledRender && renderPlan}
        <div class="page-tile-raster" aria-hidden="true">
          {#each activeTiles as active (`${rasterIdentity}:${active.tile.x},${active.tile.y},${active.tile.width},${active.tile.height}`)}
            <PageTile
              {docId}
              {pageIndex}
              scale={renderPlan.scale}
              tile={active.tile}
              fullWidth={renderPlan.pixelWidth}
              fullHeight={renderPlan.pixelHeight}
              cssWidth={cssW}
              cssHeight={cssH}
              {annotationsVersion}
              {retryVersion}
              priority={active.priority}
              onRenderError={handleTileRenderError}
            />
          {/each}
        </div>
      {/if}
    </div>

    <!-- Skeleton: shown while loading for the first time -->
    {#if renderPlan && !tiledRender && !hasContent && !displayedRenderError}
      <div class="skeleton" class:skeleton-active={visible && rendering} aria-hidden="true">
        {#if rendering}
          <div class="spinner" aria-hidden="true"></div>
        {/if}
      </div>
    {/if}

    <!-- Error overlay -->
    {#if displayedRenderError}
      <div
        class="error-overlay"
        title={displayedRenderError}
        role="alert"
        aria-live="polite"
      >
        <strong>Page could not be rendered</strong>
        <small class="error-detail">{displayedRenderError.slice(0, 120)}</small>
        {#if !planError}
          <button type="button" onclick={retryRender}>Retry</button>
        {/if}
      </div>
    {/if}

    <!--
      PDFium is the sole visual renderer for existing annotations. These are
      transparent hit targets only; painting their coarse bounding rectangles
      here would double-render highlights and turn ink/link/stamp bounds into
      solid blocks.
    -->
    {#if visible && annotations && annotations.length > 0}
      <div class="annot-layer">
        {#each annotations as ann}
          {#if ann.kind !== "widget" && ann.kind !== "other"}
            <button
              type="button"
              class="annot-rect"
              class:annot-link={ann.kind === "link" && ann.link_target !== null}
              style:left="{ann.rect.left * cssW}px"
              style:top="{ann.rect.top * cssH}px"
              style:width="{ann.rect.width * cssW}px"
              style:height="{ann.rect.height * cssH}px"
              title={ann.contents ?? (ann.link_target?.kind === "uri" ? ann.link_target.uri : ann.link_target?.kind === "page" ? `Page ${ann.link_target.page_index + 1}` : ann.kind)}
              role={ann.kind === "link" ? "link" : "button"}
              tabindex="0"
              aria-label="Annotation: {ann.kind}{ann.contents ? ` — ${ann.contents}` : ''}"
              onclick={() => {
                if (ann.kind === "link" && ann.link_target) onLinkActivate?.(ann.link_target);
              }}
              ondblclick={() => {
                if (ann.kind !== "link") onDeleteAnnotation?.(ann.index);
              }}
              onkeydown={(e) => {
                if (ann.kind !== "link" && e.key === "Delete") {
                  onDeleteAnnotation?.(ann.index);
                }
              }}
            >
              {#if ann.contents}<div class="sticky-popup">{ann.contents}</div>{/if}
            </button>
          {/if}
        {/each}
      </div>
    {/if}

    <!-- Transparent text layer for selection and find highlights -->
    {#if visible && textSpans && textSpans.length > 0}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="text-layer" onpointerup={onPointerUp}>
        {#each textSpans as span}
          <span
            class="text-span"
            style:left="{span.left * cssW}px"
            style:top="{span.top * cssH}px"
            style:width="{span.width * cssW}px"
            style:height="{span.height * cssH}px"
            style:font-size="{Math.max(1, span.height * cssH)}px"
          >{span.text}{" "}</span>
        {/each}
      </div>
    {/if}

    <!-- Find highlights -->
    {#if visible && highlights && highlights.length > 0}
      <div class="highlight-layer" aria-hidden="true">
        {#each highlights as hl, i}
          <div
            class="highlight-rect"
            class:active-match={hl.active || i === activeHighlight}
            style:left="{hl.left * cssW}px"
            style:top="{hl.top * cssH}px"
            style:width="{hl.width * cssW}px"
            style:height="{hl.height * cssH}px"
          ></div>
        {/each}
      </div>
    {/if}

    <!-- AcroForm field overlay -->
    {#if visible && formFields && formFields.length > 0}
      <div class="form-layer" aria-label="Form fields">
        {#each formFields as field}
          {#if field.kind === "text"}
            {#if field.multiline}
              <!-- svelte-ignore a11y_autofocus -->
              <textarea
                class="form-field form-text"
                style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
                style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
                style:font-size="{Math.max(8, field.rect.height * cssH * 0.6)}px"
                value={field.value} disabled={xfaReadOnly} aria-label={field.name || "Text field"}
                oninput={(e) => onFieldText?.(field.index, (e.target as HTMLTextAreaElement).value)}
              ></textarea>
            {:else}
              <input type="text" class="form-field form-text"
                style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
                style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
                style:font-size="{Math.max(8, field.rect.height * cssH * 0.75)}px"
                value={field.value} disabled={xfaReadOnly} aria-label={field.name || "Text field"}
                oninput={(e) => onFieldText?.(field.index, (e.target as HTMLInputElement).value)}
              />
            {/if}
          {:else if field.kind === "checkbox"}
            <input type="checkbox" class="form-field form-check"
              style:left="{field.rect.left * cssW + (field.rect.width * cssW) / 2 - 8}px"
              style:top="{field.rect.top * cssH + (field.rect.height * cssH) / 2 - 8}px"
              checked={field.checked} disabled={xfaReadOnly} aria-label={field.name || "Checkbox"}
              onchange={(e) => onFieldChecked?.(field.index, (e.target as HTMLInputElement).checked)}
            />
          {:else if field.kind === "radio"}
            <input type="radio" class="form-field form-check"
              style:left="{field.rect.left * cssW + (field.rect.width * cssW) / 2 - 8}px"
              style:top="{field.rect.top * cssH + (field.rect.height * cssH) / 2 - 8}px"
              checked={field.checked} disabled={xfaReadOnly} name={field.name}
              aria-label={field.name || "Radio button"}
              onchange={(e) => { if ((e.target as HTMLInputElement).checked) onFieldChecked?.(field.index, true); }}
            />
          {:else if field.kind === "combo"}
            <select class="form-field form-select"
              style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
              style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
              style:font-size="{Math.max(8, field.rect.height * cssH * 0.65)}px"
              disabled={xfaReadOnly} aria-label={field.name || "Dropdown"}
              onchange={(e) => onFieldText?.(field.index, (e.target as HTMLSelectElement).value)}
            >
              {#each field.options as opt}
                <option value={opt} selected={opt === field.value}>{opt}</option>
              {/each}
            </select>
          {:else if field.kind === "list"}
            <select class="form-field form-select" multiple
              style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
              style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
              style:font-size="{Math.max(8, field.rect.height * cssH * 0.5)}px"
              disabled={xfaReadOnly} aria-label={field.name || "List"}
              onchange={(e) => onFieldText?.(
                field.index,
                Array.from((e.target as HTMLSelectElement).selectedOptions).map((option) => option.value).join(","),
              )}
            >
              {#each field.options as opt}
                <option value={opt} selected={opt === field.value}>{opt}</option>
              {/each}
            </select>
          {:else if field.kind === "push"}
            <button class="form-field form-push"
              style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
              style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
              style:font-size="{Math.max(8, field.rect.height * cssH * 0.6)}px"
              disabled={xfaReadOnly} aria-label={field.name || "Button"}
              onclick={() => onPushButton?.(field.index)}
            >{field.name || "Reset"}</button>
          {/if}
        {/each}
      </div>
    {/if}

    <!-- Ink drawing canvas -->
    {#if activeTool === "ink"}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <canvas class="ink-canvas" bind:this={inkCanvas}
        width={inkPixelWidth} height={inkPixelHeight}
        style:width="{cssW}px" style:height="{cssH}px"
        onpointerdown={inkStart} onpointermove={inkMove}
        onpointerup={inkEnd} onpointerleave={inkEnd}
      ></canvas>
      {#if inkAllPaths.length > 0}
        <button class="ink-commit" onclick={inkCommit}>Add ink</button>
      {/if}
    {/if}

    <!-- Text/sticky click capture -->
    {#if activeTool === "text"}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="click-capture" role="presentation"
        onclick={capturePageClick}>
      </div>
    {/if}
  </div>
</div>

<style>
  .page-wrapper {
    position: relative;
    overflow: hidden;
    background: white;
    box-shadow: var(--shadow);
    border-radius: 2px;
    flex-shrink: 0;
    contain: layout paint style;
    content-visibility: auto;
  }

  .page-inner {
    position: absolute;
    transform-origin: center center;
    overflow: hidden;
    /* White background prevents any transparent-pixel bleed-through. */
    background: white;
  }

  /* Page canvas */
  .page-raster {
    position: absolute;
    inset: 0;
  }
  .page-tile-raster {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .page-canvas {
    display: block;
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    background: white;
    /* Hidden until first successful render; fade in on reveal. */
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .page-canvas-visible {
    opacity: 1;
  }

  /* Loading skeleton with shimmer animation */
  .skeleton {
    position: absolute;
    inset: 0;
    background: white;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .skeleton-active {
    background: linear-gradient(
      90deg,
      var(--bg-elev) 25%,
      var(--border) 50%,
      var(--bg-elev) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease infinite;
  }

  @keyframes shimmer {
    0%   { background-position: 200% center; }
    100% { background-position: -200% center; }
  }

  /* Spinner shown inside skeleton while rendering */
  .spinner {
    width: 20px;
    height: 20px;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
    opacity: 0.6;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .error-overlay {
    position: absolute; inset: 0;
    z-index: 20;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 6px; background: var(--bg-elev);
    color: var(--danger); font-size: 13px;
  }

  .error-detail {
    font-size: 10px; color: var(--fg-muted); max-width: 90%;
    word-break: break-all; text-align: center; opacity: 0.8;
  }

  .error-overlay button {
    margin-top: 4px; padding: 5px 12px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg-elev); color: var(--fg); cursor: pointer;
  }
  .error-overlay button:hover { border-color: var(--accent); }

  /* Transparent annotation interaction layer; PDFium paints appearances. */
  .annot-layer { position: absolute; z-index: 2; inset: 0; overflow: hidden; pointer-events: none; }
  .annot-rect {
    position: absolute; border-radius: 2px;
    pointer-events: auto; cursor: default; background: transparent;
    border: 0; padding: 0; color: inherit; font: inherit; text-align: left;
  }
  .annot-link { cursor: pointer; }
  .annot-link:hover { background: rgba(65, 135, 245, 0.08); }
  .annot-rect:focus-visible { outline: 2px solid color-mix(in srgb, var(--accent) 65%, transparent); }
  .sticky-popup {
    position: absolute; left: 20px; top: 0;
    min-width: 160px; max-width: 240px;
    background: var(--bg-elev); border: 1px solid var(--border);
    border-radius: var(--radius); box-shadow: var(--shadow);
    padding: 8px 10px; font-size: 12px; color: var(--fg);
    z-index: 10; white-space: pre-wrap; display: none;
  }
  .annot-rect:focus .sticky-popup,
  .annot-rect:hover .sticky-popup { display: block; }

  /* Text layer — invisible but selectable */
  .text-layer { position: absolute; z-index: 1; inset: 0; overflow: hidden; pointer-events: none; }
  .text-span {
    position: absolute;
    color: transparent;
    white-space: pre;
    cursor: text;
    user-select: text;
    pointer-events: auto;
    line-height: 1;
    padding: 0; margin: 0;
    transform-origin: top left;
  }
  .text-span::selection { background: rgba(59, 130, 246, 0.28); }

  /* Find highlights */
  .highlight-layer { position: absolute; z-index: 1; inset: 0; pointer-events: none; }
  .highlight-rect {
    position: absolute;
    background: rgba(255, 210, 0, 0.38);
    border: 1px solid rgba(200, 160, 0, 0.55);
    border-radius: 2px;
    transition: background 80ms;
  }
  .highlight-rect.active-match {
    background: rgba(255, 120, 0, 0.52);
    border-color: rgba(200, 80, 0, 0.85);
  }

  /* Form fields */
  .form-layer { position: absolute; z-index: 3; inset: 0; overflow: hidden; pointer-events: none; }
  .form-field {
    position: absolute; pointer-events: auto; box-sizing: border-box;
    border: 1px solid rgba(0, 100, 255, 0.4);
    background: rgba(255, 255, 255, 0.88);
    color: #111; padding: 1px 3px; font-family: inherit; outline: none;
  }
  .form-field:focus { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(0,100,255,0.18); }
  .form-field:disabled { background: rgba(220, 220, 220, 0.65); cursor: default; }
  .form-text { resize: none; overflow: hidden; }
  .form-check { width: 16px; height: 16px; padding: 0; border: none; background: none; cursor: pointer; }
  .form-select { padding: 1px 2px; }
  .form-push {
    cursor: pointer; background: var(--bg-elev); border-radius: var(--radius);
    overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
  }

  /* Ink canvas */
  .ink-canvas { position: absolute; z-index: 4; inset: 0; cursor: crosshair; touch-action: none; }
  .ink-commit {
    position: absolute; bottom: 8px; right: 8px;
    background: var(--accent); color: var(--accent-fg);
    border: none; border-radius: var(--radius); padding: 4px 10px;
    cursor: pointer; font-size: 12px; z-index: 10;
  }

  /* Text/sticky tool click capture */
  .click-capture { position: absolute; inset: 0; cursor: crosshair; z-index: 5; }
</style>
