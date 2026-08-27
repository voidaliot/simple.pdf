<script lang="ts">
  import { onDestroy } from "svelte";
  import { type TextSpan, type Annotation, type AnnRect, type FormField, type LinkTarget } from "../lib/ipc";
  import { requestPageFrame, type PageFrame, type PageFrameRequest } from "../lib/pageRenderCache";
  import { CSS_PIXELS_PER_POINT } from "../stores/viewer.svelte";

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
  /** Full-page bitmaps need a hard ceiling until the renderer is tiled. */
  const MAX_RENDER_PIXELS = 6_000_000;

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
    docId, pageIndex, width, height, zoom, visible, priority = false,
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

  // Cap DPR to keep full-page backing stores within a predictable memory budget.
  // The canvas is still laid out at the native CSS size and the browser smooths
  // the rare high-zoom case where the render budget is reached.
  const dpr = Math.min(2, Math.max(1, window.devicePixelRatio || 1));
  const cssScale = $derived(zoom * CSS_PIXELS_PER_POINT);
  const maxRenderScale = $derived(
    Math.sqrt(MAX_RENDER_PIXELS / Math.max(1, width * height)),
  );
  const renderScale = $derived(Math.min(cssScale * dpr, maxRenderScale));

  // Pre-rotation CSS dimensions
  const cssW = $derived(Math.max(1, Math.round(width * cssScale)));
  const cssH = $derived(Math.max(1, Math.round(height * cssScale)));

  // Post-rotation layout dimensions
  const isRotated = $derived(rotation === 90 || rotation === 270);
  const displayW = $derived(isRotated ? cssH : cssW);
  const displayH = $derived(isRotated ? cssW : cssH);
  const innerTop = $derived(Math.round((displayH - cssH) / 2));
  const innerLeft = $derived(Math.round((displayW - cssW) / 2));

  // ── Canvas render state ──────────────────────────────────────────────────────
  /** The page canvas — always in the DOM so bind:this is valid in the effect. */
  let canvasEl = $state<HTMLCanvasElement | undefined>();
  /** True once the canvas has been painted at least once. */
  let hasContent = $state(false);
  /** Non-empty when the last render failed. */
  let renderError = $state("");
  /** True while a network render is in-flight (not a cache hit). */
  let rendering = $state(false);
  /** Incremented by the retry action to re-run the render effect. */
  let retryVersion = $state(0);
  let renderGeneration = 0;
  let lastPaintedKey = "";

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

    const id = docId;
    const idx = pageIndex;
    const scale = renderScale;
    const frameKey = `${id}:${idx}:${scale.toFixed(5)}:${annotationsVersion}:${retryVersion}`;
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
        version: annotationsVersion,
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
          renderError = String(e);
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
    inkCanvas.width = cssW * dpr;
    inkCanvas.height = cssH * dpr;
    const ctx = inkCanvas.getContext("2d");
    if (ctx) ctx.scale(dpr, dpr);
    drawInk();
  });

  // ── Text selection → annotation rects ────────────────────────────────────────
  function clientToPage(clientX: number, clientY: number, pageEl: HTMLElement) {
    const rect = pageEl.getBoundingClientRect();
    const displayX = (clientX - rect.left) / Math.max(1, rect.width);
    const displayY = (clientY - rect.top) / Math.max(1, rect.height);

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
  style:width="{displayW}px"
  style:height="{displayH}px"
  aria-label="Page {pageIndex + 1}"
  role="group"
>
  <div
    class="page-inner"
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
    </div>

    <!-- Skeleton: shown while loading for the first time -->
    {#if !hasContent && !renderError}
      <div class="skeleton" class:skeleton-active={visible && rendering} aria-hidden="true">
        {#if rendering}
          <div class="spinner" aria-hidden="true"></div>
        {/if}
      </div>
    {/if}

    <!-- Error overlay -->
    {#if renderError}
      <div class="error-overlay" title={renderError}>
        <strong>Page could not be rendered</strong>
        <small class="error-detail">{renderError.slice(0, 120)}</small>
        <button type="button" onclick={retryRender}>Retry</button>
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
        width={cssW * dpr} height={cssH * dpr}
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
