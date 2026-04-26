<script lang="ts">
  interface Props {
    onClose: () => void;
    onPlace: (paths: [number, number][][]) => Promise<void>;
  }

  let { onClose, onPlace }: Props = $props();

  let canvas: HTMLCanvasElement | undefined = $state();
  let drawing = $state(false);
  let currentPath = $state<[number, number][]>([]);
  let allPaths = $state<[number, number][][]>([]);
  let placing = $state(false);

  function getCtx() {
    return canvas?.getContext("2d") ?? null;
  }

  function normPos(e: PointerEvent) {
    if (!canvas) return { nx: 0, ny: 0 };
    const r = canvas.getBoundingClientRect();
    return {
      nx: (e.clientX - r.left) / r.width,
      ny: (e.clientY - r.top) / r.height,
    };
  }

  function redraw() {
    const ctx = getCtx();
    if (!ctx || !canvas) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.strokeStyle = "#1a1a1a";
    ctx.lineWidth = 2;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    for (const path of [...allPaths, currentPath]) {
      if (path.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(path[0]![0] * canvas.width, path[0]![1] * canvas.height);
      for (let i = 1; i < path.length; i++) {
        ctx.lineTo(path[i]![0] * canvas.width, path[i]![1] * canvas.height);
      }
      ctx.stroke();
    }
  }

  function onPointerDown(e: PointerEvent) {
    if (!canvas) return;
    drawing = true;
    canvas.setPointerCapture(e.pointerId);
    const { nx, ny } = normPos(e);
    currentPath = [[nx, ny]];
  }

  function onPointerMove(e: PointerEvent) {
    if (!drawing) return;
    const { nx, ny } = normPos(e);
    currentPath = [...currentPath, [nx, ny]];
    redraw();
  }

  function onPointerUp() {
    if (!drawing) return;
    drawing = false;
    if (currentPath.length > 1) allPaths = [...allPaths, currentPath];
    currentPath = [];
    redraw();
  }

  function clear() {
    allPaths = [];
    currentPath = [];
    redraw();
  }

  async function place() {
    if (allPaths.length === 0) return;
    placing = true;
    try { await onPlace(allPaths); }
    finally { placing = false; }
  }

  function onKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="backdrop" role="dialog" aria-modal="true" aria-label="Signature capture" tabindex="-1"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
  onkeydown={() => {}}>
  <div class="modal">
    <div class="modal-header">
      <h2>Draw signature</h2>
      <button onclick={onClose} aria-label="Close">✕</button>
    </div>
    <p class="hint">Draw your signature below. Double-click to clear.</p>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <canvas
      bind:this={canvas}
      width={480}
      height={200}
      class="sig-canvas"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointerleave={onPointerUp}
      ondblclick={clear}
    ></canvas>
    <div class="modal-footer">
      <button onclick={clear}>Clear</button>
      <button class="primary" onclick={place} disabled={allPaths.length === 0 || placing}>
        {placing ? "Placing…" : "Place on page"}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; background: rgba(0,0,0,0.5);
    display: flex; align-items: center; justify-content: center; z-index: 500;
  }
  .modal {
    background: var(--bg-elev); border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 24px; width: 520px;
    box-shadow: var(--shadow);
  }
  .modal-header {
    display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px;
  }
  .modal-header h2 { margin: 0; font-size: 18px; font-weight: 500; }
  .modal-header button {
    background: none; border: none; cursor: pointer; font-size: 18px; color: var(--fg-muted); padding: 2px;
  }
  .modal-header button:hover { color: var(--fg); }
  .hint { font-size: 12px; color: var(--fg-muted); margin: 0 0 12px; }
  .sig-canvas {
    display: block; width: 100%; border: 2px dashed var(--border);
    border-radius: var(--radius); background: white; cursor: crosshair;
    touch-action: none;
  }
  .modal-footer { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
  .modal-footer button {
    padding: 8px 16px; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--bg-elev);
    cursor: pointer; font: inherit; font-size: 13px;
  }
  .modal-footer button:disabled { opacity: 0.5; cursor: not-allowed; }
  .modal-footer .primary { background: var(--accent); color: var(--accent-fg); border-color: var(--accent); }
</style>
