<script lang="ts">
  import { renderPageB64, type TextSpan, type Annotation, type AnnRect, type FormField } from "../lib/ipc";

  export interface Highlight {
    left: number;
    top: number;
    width: number;
    height: number;
  }

  type AnnotTool = "none" | "highlight" | "underline" | "strikeout" | "text" | "ink";

  interface Props {
    docId: string;
    pageIndex: number;
    width: number;
    height: number;
    zoom: number;
    visible: boolean;
    rotation?: number;
    textSpans?: TextSpan[];
    highlights?: Highlight[];
    activeHighlight?: number;
    annotations?: Annotation[];
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
  }

  let {
    docId, pageIndex, width, height, zoom, visible,
    rotation = 0,
    textSpans, highlights, activeHighlight = -1,
    annotations,
    formFields,
    xfaReadOnly = false,
    activeTool = "none",
    inkColor = [255, 0, 0],
    inkWidth = 2,
    onPageClick, onTextSelected, onInkStroke, onDeleteAnnotation,
    onFieldText, onFieldChecked,
  }: Props = $props();

  const dpr = window.devicePixelRatio ?? 1;
  const renderScale = $derived(zoom * dpr);

  const cssW = $derived(Math.round(width * zoom));
  const cssH = $derived(Math.round(height * zoom));

  const isRotated = $derived(rotation === 90 || rotation === 270);
  const displayW = $derived(isRotated ? cssH : cssW);
  const displayH = $derived(isRotated ? cssW : cssH);
  const innerTop = $derived(Math.round((displayH - cssH) / 2));
  const innerLeft = $derived(Math.round((displayW - cssW) / 2));

  // IPC-based image loading — avoids pdf:// custom scheme which WebView2
  // blocks when the frontend is served from the HTTP dev server.
  type ImgState = "idle" | "loading" | "loaded" | "error";
  let imgState = $state<ImgState>("idle");
  let imgSrc = $state("");
  let imgError = $state("");

  $effect(() => {
    if (!visible) {
      imgState = "idle";
      imgSrc = "";
      imgError = "";
      return;
    }

    // Capture reactive inputs before the async gap.
    const id = docId;
    const idx = pageIndex;
    const scale = renderScale;

    imgState = "loading";
    imgError = "";
    let cancelled = false;

    renderPageB64(id, idx, scale)
      .then((dataUrl) => {
        if (!cancelled) {
          imgSrc = dataUrl;
          imgState = "loaded";
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          imgError = String(e);
          imgState = "error";
        }
      });

    return () => { cancelled = true; };
  });

  // ── Ink drawing ─────────────────────────────────────────────────────────────
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
    if (inkCurrentPath.length > 1) {
      inkAllPaths = [...inkAllPaths, inkCurrentPath];
    }
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
    const draw = (paths: [number, number][][], color: string, lw: number) => {
      ctx.strokeStyle = color;
      ctx.lineWidth = lw;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      for (const path of paths) {
        if (path.length < 2) continue;
        ctx.beginPath();
        ctx.moveTo(path[0]![0] * inkCanvas!.width, path[0]![1] * inkCanvas!.height);
        for (let i = 1; i < path.length; i++) {
          ctx.lineTo(path[i]![0] * inkCanvas!.width, path[i]![1] * inkCanvas!.height);
        }
        ctx.stroke();
      }
    };
    draw(inkAllPaths, `rgb(${inkColor.join(",")})`, inkWidth);
    if (inkCurrentPath.length > 1)
      draw([inkCurrentPath], `rgb(${inkColor.join(",")})`, inkWidth);
  }

  $effect(() => {
    // resize canvas when cssW/cssH change
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
      // r is in viewport coords; convert to normalized page coords
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
    <!-- Page image -->
    {#if imgState === "loaded"}
      <img src={imgSrc} alt="Page {pageIndex + 1}" width={cssW} height={cssH}
        draggable="false" />
    {:else if imgState === "error"}
      <div class="error-overlay" title={imgError}>
        <span>⚠ render failed</span>
        {#if imgError}
          <small class="error-detail">{imgError.slice(0, 120)}</small>
        {/if}
      </div>
    {:else}
      <div class="skeleton" aria-hidden="true"></div>
    {/if}

    <!-- Existing annotation overlays (read from PDF) -->
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
                {#if ann.contents}
                  <div class="sticky-popup">{ann.contents}</div>
                {/if}
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

    <!-- Transparent text layer -->
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
          <div class="highlight-rect" class:active-match={i === activeHighlight}
            style:left="{hl.left * cssW}px" style:top="{hl.top * cssH}px"
            style:width="{hl.width * cssW}px" style:height="{hl.height * cssH}px">
          </div>
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
                style:left="{field.rect.left * cssW}px"
                style:top="{field.rect.top * cssH}px"
                style:width="{field.rect.width * cssW}px"
                style:height="{field.rect.height * cssH}px"
                style:font-size="{Math.max(8, field.rect.height * cssH * 0.6)}px"
                value={field.value}
                disabled={xfaReadOnly}
                aria-label={field.name || "Text field"}
                onchange={(e) => onFieldText?.(field.index, (e.target as HTMLTextAreaElement).value)}
              ></textarea>
            {:else}
              <input
                type="text"
                class="form-field form-text"
                style:left="{field.rect.left * cssW}px"
                style:top="{field.rect.top * cssH}px"
                style:width="{field.rect.width * cssW}px"
                style:height="{field.rect.height * cssH}px"
                style:font-size="{Math.max(8, field.rect.height * cssH * 0.75)}px"
                value={field.value}
                disabled={xfaReadOnly}
                aria-label={field.name || "Text field"}
                onchange={(e) => onFieldText?.(field.index, (e.target as HTMLInputElement).value)}
              />
            {/if}
          {:else if field.kind === "checkbox"}
            <input
              type="checkbox"
              class="form-field form-check"
              style:left="{field.rect.left * cssW + (field.rect.width * cssW) / 2 - 8}px"
              style:top="{field.rect.top * cssH + (field.rect.height * cssH) / 2 - 8}px"
              checked={field.checked}
              disabled={xfaReadOnly}
              aria-label={field.name || "Checkbox"}
              onchange={(e) => onFieldChecked?.(field.index, (e.target as HTMLInputElement).checked)}
            />
          {:else if field.kind === "radio"}
            <input
              type="radio"
              class="form-field form-check"
              style:left="{field.rect.left * cssW + (field.rect.width * cssW) / 2 - 8}px"
              style:top="{field.rect.top * cssH + (field.rect.height * cssH) / 2 - 8}px"
              checked={field.checked}
              disabled={xfaReadOnly}
              name={field.name}
              aria-label={field.name || "Radio button"}
              onchange={(e) => { if ((e.target as HTMLInputElement).checked) onFieldChecked?.(field.index, true); }}
            />
          {:else if field.kind === "combo"}
            <select
              class="form-field form-select"
              style:left="{field.rect.left * cssW}px"
              style:top="{field.rect.top * cssH}px"
              style:width="{field.rect.width * cssW}px"
              style:height="{field.rect.height * cssH}px"
              style:font-size="{Math.max(8, field.rect.height * cssH * 0.65)}px"
              disabled={xfaReadOnly}
              aria-label={field.name || "Dropdown"}
              onchange={(e) => onFieldText?.(field.index, (e.target as HTMLSelectElement).value)}
            >
              {#each field.options as opt}
                <option value={opt} selected={opt === field.value}>{opt}</option>
              {/each}
            </select>
          {:else if field.kind === "list"}
            <select
              class="form-field form-select"
              multiple
              style:left="{field.rect.left * cssW}px"
              style:top="{field.rect.top * cssH}px"
              style:width="{field.rect.width * cssW}px"
              style:height="{field.rect.height * cssH}px"
              style:font-size="{Math.max(8, field.rect.height * cssH * 0.5)}px"
              disabled={xfaReadOnly}
              aria-label={field.name || "List"}
            >
              {#each field.options as opt}
                <option value={opt} selected={opt === field.value}>{opt}</option>
              {/each}
            </select>
          {/if}
        {/each}
      </div>
    {/if}

    <!-- Ink canvas (for drawing) -->
    {#if activeTool === "ink"}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <canvas
        class="ink-canvas"
        bind:this={inkCanvas}
        width={cssW * dpr}
        height={cssH * dpr}
        style:width="{cssW}px"
        style:height="{cssH}px"
        onpointerdown={inkStart}
        onpointermove={inkMove}
        onpointerup={inkEnd}
        onpointerleave={inkEnd}
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
    position: relative; overflow: hidden; background: white;
    box-shadow: var(--shadow); border-radius: 2px; flex-shrink: 0;
  }
  .page-inner { position: absolute; transform-origin: center center; overflow: hidden; }
  img { display: block; width: 100%; height: 100%; object-fit: fill; user-select: none; position: absolute; inset: 0; }

  .skeleton {
    position: absolute; inset: 0;
    background: linear-gradient(90deg, var(--bg-elev) 25%, var(--border) 50%, var(--bg-elev) 75%);
    background-size: 200% 100%;
    animation: shimmer 1.4s ease infinite;
  }
  @keyframes shimmer {
    0%   { background-position: 200% center; }
    100% { background-position: -200% center; }
  }
  .error-overlay {
    position: absolute; inset: 0; display: flex; flex-direction: column;
    align-items: center; justify-content: center; gap: 6px;
    background: var(--bg-elev); color: var(--danger); font-size: 13px;
  }
  .error-detail {
    font-size: 10px; color: var(--fg-muted); max-width: 90%;
    word-break: break-all; text-align: center; opacity: 0.8;
  }

  /* Annotation overlays */
  .annot-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .annot-rect {
    position: absolute;
    border-radius: 2px;
    pointer-events: auto;
    cursor: default;
  }
  .annot-highlight { mix-blend-mode: multiply; }
  .annot-underline, .annot-squiggly { background: transparent !important; }
  .annot-strikeout { background: transparent !important; }
  .underline-line { position: absolute; bottom: 1px; left: 0; right: 0; height: 2px; }
  .strikeout-line { position: absolute; top: 50%; left: 0; right: 0; height: 2px; transform: translateY(-50%); }
  .sticky-icon { font-size: 16px; line-height: 1; display: block; }
  .sticky-popup {
    position: absolute; left: 20px; top: 0; min-width: 160px; max-width: 240px;
    background: var(--bg-elev); border: 1px solid var(--border); border-radius: var(--radius);
    box-shadow: var(--shadow); padding: 8px 10px; font-size: 12px; color: var(--fg);
    z-index: 10; white-space: pre-wrap; display: none;
  }
  .annot-rect:focus .sticky-popup,
  .annot-rect:hover .sticky-popup { display: block; }

  /* Text layer */
  .text-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .text-span {
    position: absolute; color: transparent; white-space: pre;
    cursor: text; user-select: text; pointer-events: auto; line-height: 1; padding: 0; margin: 0;
    transform-origin: top left;
  }

  /* Find highlights */
  .highlight-layer { position: absolute; inset: 0; pointer-events: none; }
  .highlight-rect {
    position: absolute; background: rgba(255,210,0,0.35);
    border: 1px solid rgba(200,160,0,0.6); border-radius: 1px;
  }
  .highlight-rect.active-match { background: rgba(255,120,0,0.5); border-color: rgba(200,80,0,0.9); }

  /* Form fields overlay */
  .form-layer { position: absolute; inset: 0; overflow: hidden; pointer-events: none; }
  .form-field {
    position: absolute;
    pointer-events: auto;
    box-sizing: border-box;
    border: 1px solid rgba(0, 100, 255, 0.4);
    background: rgba(255, 255, 255, 0.85);
    color: #111;
    padding: 1px 3px;
    font-family: inherit;
    outline: none;
  }
  .form-field:focus { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(0,100,255,0.2); }
  .form-field:disabled { background: rgba(220,220,220,0.6); cursor: default; }
  .form-text { resize: none; overflow: hidden; }
  .form-check { width: 16px; height: 16px; padding: 0; border: none; background: none; cursor: pointer; }
  .form-select { padding: 1px 2px; }

  /* Ink canvas */
  .ink-canvas {
    position: absolute; inset: 0; cursor: crosshair;
    touch-action: none;
  }
  .ink-commit {
    position: absolute; bottom: 8px; right: 8px;
    background: var(--accent); color: var(--accent-fg);
    border: none; border-radius: var(--radius); padding: 4px 10px;
    cursor: pointer; font-size: 12px; z-index: 10;
  }

  /* Sticky / text tool capture */
  .click-capture {
    position: absolute; inset: 0; cursor: crosshair; z-index: 5;
  }
</style>
