<script lang="ts">
  import { renderPagePixels, type TextSpan, type Annotation, type AnnRect, type FormField } from "../lib/ipc";

  export interface Highlight {
    left: number;
    top: number;
    width: number;
    height: number;
  }

  // ── Named constants ──────────────────────────────────────────────────────────
  /** Debounce ms before re-rendering after a zoom change. */
  const ZOOM_DEBOUNCE_MS = 250;
  /** Max cached render entries across all pages (LRU eviction). */
  const MAX_CACHE_ENTRIES = 40;

  // ── Module-level render cache (survives Svelte component re-creation) ────────
  //
  // Key: `${docId}|${pageIndex}|${scaleTo3dp}|${dprTo2dp}`
  // Stores the last rendered RGBA pixels so zoom changes that return to a
  // previously-seen scale are instant.  Evicted LRU-style when the cache
  // grows beyond MAX_CACHE_ENTRIES.
  interface CachedFrame {
    width: number;
    height: number;
    data: Uint8ClampedArray<ArrayBuffer>;
  }
  const _cache = new Map<string, CachedFrame>();

  function cacheKey(docId: string, idx: number, scale: number, dpr: number): string {
    return `${docId}|${idx}|${scale.toFixed(3)}|${dpr.toFixed(2)}`;
  }

  function cacheGet(key: string): CachedFrame | undefined {
    const v = _cache.get(key);
    if (v) {
      // Refresh LRU order
      _cache.delete(key);
      _cache.set(key, v);
    }
    return v;
  }

  function cachePut(key: string, frame: CachedFrame): void {
    _cache.set(key, frame);
    // Evict oldest entries
    while (_cache.size > MAX_CACHE_ENTRIES) {
      const first = _cache.keys().next().value;
      if (first !== undefined) _cache.delete(first);
    }
  }

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
    /** Current zoom factor (CSS pixels per PDF point). */
    zoom: number;
    /** Whether this page is currently in the viewport or prefetch zone. */
    visible: boolean;
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
    onPageClick?: (e: MouseEvent, el: HTMLElement, cssW: number, cssH: number) => void;
    onTextSelected?: (rects: AnnRect[]) => void;
    onInkStroke?: (paths: [number, number][][]) => void;
    onDeleteAnnotation?: (annotIndex: number) => void;
    onFieldText?: (annotIndex: number, value: string) => void;
    onFieldChecked?: (annotIndex: number, checked: boolean) => void;
    onPushButton?: (annotIndex: number) => void;
  }

  let {
    docId, pageIndex, width, height, zoom, visible,
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
    onFieldText, onFieldChecked, onPushButton,
  }: Props = $props();

  const dpr = window.devicePixelRatio ?? 1;
  const renderScale = $derived(zoom * dpr);

  // Pre-rotation CSS dimensions
  const cssW = $derived(Math.round(width * zoom));
  const cssH = $derived(Math.round(height * zoom));

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
  /** Timer for zoom-change debounce. */
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  /** Paint a cached or freshly-rendered frame onto the canvas. */
  function paint(frame: CachedFrame) {
    const canvas = canvasEl;
    if (!canvas) return;
    // Setting canvas.width clears the buffer; putImageData follows in the
    // same microtask so the browser never paints a blank frame.
    canvas.width = frame.width;
    canvas.height = frame.height;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;
    ctx.putImageData(new ImageData(frame.data, frame.width, frame.height), 0, 0);
    hasContent = true;
    renderError = "";
    rendering = false;
  }

  $effect(() => {
    if (!visible) return;

    const id = docId;
    const idx = pageIndex;
    const scale = renderScale;
    void annotationsVersion; // Re-render when annotations change
    let cancelled = false;

    const key = cacheKey(id, idx, scale, dpr);

    // Cache hit → paint immediately, no IPC call
    const cached = cacheGet(key);
    if (cached) {
      paint(cached);
      return;
    }

    // Cache miss → debounce then render
    // While the debounce is waiting, keep the existing canvas content visible
    // (old-zoom pixels stay on screen, just rescaled by CSS — better than blank).
    if (debounceTimer !== null) clearTimeout(debounceTimer);
    rendering = true;

    debounceTimer = setTimeout(() => {
      if (cancelled) return;

      renderPagePixels(id, idx, scale)
        .then((frame) => {
          if (cancelled) return;
          cachePut(key, frame);
          paint(frame);
        })
        .catch((e: unknown) => {
          if (cancelled) return;
          renderError = String(e);
          rendering = false;
        });
    }, ZOOM_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      if (debounceTimer !== null) { clearTimeout(debounceTimer); debounceTimer = null; }
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
  function onPointerUp() {
    if (!["highlight", "underline", "strikeout"].includes(activeTool)) return;
    const sel = window.getSelection();
    if (!sel || sel.isCollapsed) return;
    const range = sel.getRangeAt(0);
    const rects: AnnRect[] = [];
    for (const r of range.getClientRects()) {
      const pageEl = inkCanvas?.parentElement;
      if (!pageEl) continue;
      const pr = pageEl.getBoundingClientRect();
      rects.push({
        left: (r.left - pr.left) / cssW,
        top: (r.top - pr.top) / cssH,
        width: r.width / cssW,
        height: r.height / cssH,
      });
    }
    if (rects.length > 0) onTextSelected?.(rects);
  }

  function normPos(e: PointerEvent, el: HTMLElement) {
    const r = el.getBoundingClientRect();
    return { nx: (e.clientX - r.left) / el.clientWidth, ny: (e.clientY - r.top) / el.clientHeight };
  }

  function annColor(c: [number, number, number, number]) {
    return `rgba(${c[0]},${c[1]},${c[2]},${c[3] / 255})`;
  }
</script>

<div
  class="page-wrapper"
  style:width="{displayW}px"
  style:height="{displayH}px"
  aria-label="Page {pageIndex + 1}"
  role="img"
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
    <canvas
      bind:this={canvasEl}
      class="page-canvas"
      class:page-canvas-visible={hasContent}
      style:width="{cssW}px"
      style:height="{cssH}px"
      aria-hidden="true"
    ></canvas>

    <!-- Skeleton: shown while loading for the first time -->
    {#if !hasContent && !renderError}
      <div class="skeleton" aria-hidden="true">
        {#if rendering}
          <div class="spinner" aria-hidden="true"></div>
        {/if}
      </div>
    {/if}

    <!-- Error overlay -->
    {#if renderError}
      <div class="error-overlay" title={renderError}>
        <span>⚠ render failed</span>
        <small class="error-detail">{renderError.slice(0, 120)}</small>
      </div>
    {/if}

    <!-- Existing annotations (read from PDF) -->
    {#if annotations && annotations.length > 0}
      <div class="annot-layer" aria-hidden="true">
        {#each annotations as ann}
          {#if ann.kind !== "widget"}
            <div
              class="annot-rect annot-{ann.kind}"
              style:left="{ann.rect.left * cssW}px"
              style:top="{ann.rect.top * cssH}px"
              style:width="{ann.rect.width * cssW}px"
              style:height="{ann.rect.height * cssH}px"
              style:background={ann.kind === "text" ? "none" : annColor(ann.color)}
              title={ann.contents ?? ann.kind}
              role="button"
              tabindex="0"
              aria-label="Annotation: {ann.kind}{ann.contents ? ` — ${ann.contents}` : ''}"
              ondblclick={() => onDeleteAnnotation?.(ann.index)}
              onkeydown={(e) => { if (e.key === "Delete") onDeleteAnnotation?.(ann.index); }}
            >
              {#if ann.kind === "text"}
                <span class="sticky-icon" style:color={annColor(ann.color)}>📌</span>
                {#if ann.contents}<div class="sticky-popup">{ann.contents}</div>{/if}
              {:else if ann.kind === "strikeout"}
                <div class="strikeout-line" style:background={annColor(ann.color)}></div>
              {:else if ann.kind === "underline"}
                <div class="underline-line" style:background={annColor(ann.color)}></div>
              {/if}
            </div>
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
          >{span.text}</span>
        {/each}
      </div>
    {/if}

    <!-- Find highlights -->
    {#if highlights && highlights.length > 0}
      <div class="highlight-layer" aria-hidden="true">
        {#each highlights as hl, i}
          <div
            class="highlight-rect"
            class:active-match={i === activeHighlight}
            style:left="{hl.left * cssW}px"
            style:top="{hl.top * cssH}px"
            style:width="{hl.width * cssW}px"
            style:height="{hl.height * cssH}px"
          ></div>
        {/each}
      </div>
    {/if}

    <!-- AcroForm field overlay -->
    {#if formFields && formFields.length > 0}
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
                onchange={(e) => onFieldText?.(field.index, (e.target as HTMLTextAreaElement).value)}
              ></textarea>
            {:else}
              <input type="text" class="form-field form-text"
                style:left="{field.rect.left * cssW}px" style:top="{field.rect.top * cssH}px"
                style:width="{field.rect.width * cssW}px" style:height="{field.rect.height * cssH}px"
                style:font-size="{Math.max(8, field.rect.height * cssH * 0.75)}px"
                value={field.value} disabled={xfaReadOnly} aria-label={field.name || "Text field"}
                onchange={(e) => onFieldText?.(field.index, (e.target as HTMLInputElement).value)}
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
        onclick={(e) => onPageClick?.(e, e.currentTarget.parentElement as HTMLElement, cssW, cssH)}>
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
  }

  .page-inner {
    position: absolute;
    transform-origin: center center;
    overflow: hidden;
    /* White background prevents any transparent-pixel bleed-through. */
    background: white;
  }

  /* Page canvas */
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
    background: linear-gradient(
      90deg,
      var(--bg-elev) 25%,
      var(--border) 50%,
      var(--bg-elev) 75%
    );
    background-size: 200% 100%;
    animation: shimmer 1.4s ease infinite;
    display: flex;
    align-items: center;
    justify-content: center;
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
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    gap: 6px; background: var(--bg-elev);
    color: var(--danger); font-size: 13px;
  }

  .error-detail {
    font-size: 10px; color: var(--fg-muted); max-width: 90%;
    word-break: break-all; text-align: center; opacity: 0.8;
  }

  /* Annotations */
  .annot-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .annot-rect {
    position: absolute; border-radius: 2px;
    pointer-events: auto; cursor: default;
  }
  .annot-highlight { mix-blend-mode: multiply; }
  .annot-underline, .annot-squiggly, .annot-strikeout { background: transparent !important; }
  .underline-line { position: absolute; bottom: 1px; left: 0; right: 0; height: 2px; }
  .strikeout-line { position: absolute; top: 50%; left: 0; right: 0; height: 2px; transform: translateY(-50%); }
  .sticky-icon { font-size: 16px; line-height: 1; display: block; }
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
  .text-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
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
  .highlight-layer { position: absolute; inset: 0; pointer-events: none; }
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
  .form-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
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
  .ink-canvas { position: absolute; inset: 0; cursor: crosshair; touch-action: none; }
  .ink-commit {
    position: absolute; bottom: 8px; right: 8px;
    background: var(--accent); color: var(--accent-fg);
    border: none; border-radius: var(--radius); padding: 4px 10px;
    cursor: pointer; font-size: 12px; z-index: 10;
  }

  /* Text/sticky tool click capture */
  .click-capture { position: absolute; inset: 0; cursor: crosshair; z-index: 5; }
</style>
