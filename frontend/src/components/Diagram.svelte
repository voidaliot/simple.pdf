<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import { writeTextFile } from "@tauri-apps/plugin-fs";
  import { renderDiagram } from "../lib/diagrams";
  import type { DiagramSource } from "../lib/diagramSource";

  let { diagram, index, title }: { diagram: DiagramSource; index: number; title: string } = $props();
  let root: HTMLElement;
  let visible = $state(false);
  let revision = $state(0);
  let error = $state("");
  let actionMessage = $state("");
  let rendered = $state<{ svg: string; url: string; width: number; height: number } | null>(null);
  let zoom = $state(1);
  let fit = $state(true);
  const label = $derived(diagram.format === "mermaid" ? "Mermaid" : "PlantUML");

  onMount(() => {
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        visible = true;
        observer.disconnect();
      }
    }, { rootMargin: "300px" });
    observer.observe(root);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!visible) return;
    const current = diagram;
    void revision;
    const controller = new AbortController();
    let url: string | undefined;
    rendered = null;
    error = "";
    void renderDiagram(current.format, current.source, controller.signal).then((result) => {
      if (controller.signal.aborted) return;
      url = URL.createObjectURL(new Blob([result.svg], { type: "image/svg+xml" }));
      rendered = { ...result, url };
    }).catch((reason: unknown) => {
      if (!controller.signal.aborted) error = reason instanceof Error ? reason.message : String(reason);
    });
    return () => { controller.abort(); if (url) URL.revokeObjectURL(url); };
  });

  async function copySource() {
    try { await navigator.clipboard.writeText(diagram.source); actionMessage = "Source copied"; }
    catch { actionMessage = "Could not copy the source"; }
  }

  function changeZoom(delta: number) {
    if (!rendered) return;
    const current = fit ? (root.querySelector("img")?.getBoundingClientRect().width ?? rendered.width) / rendered.width : zoom;
    if (delta < 0 && current <= 0.25) return;
    zoom = Math.max(0.25, Math.min(4, current + delta));
    fit = false;
  }

  async function exportSvg() {
    if (!rendered) return;
    const result = rendered;
    const filename = `${title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_")}-${index + 1}.svg`;
    try {
      if (isTauri()) {
        const path = await save({ defaultPath: filename, filters: [{ name: "SVG diagram", extensions: ["svg"] }] });
        if (!path) return;
        await writeTextFile(path, result.svg);
      } else {
        const link = document.createElement("a");
        link.href = result.url;
        link.download = filename;
        link.click();
      }
      actionMessage = "SVG exported";
    } catch (reason) { actionMessage = `Could not export: ${String(reason)}`; }
  }
</script>

<section class="diagram" bind:this={root} aria-label={`${label} diagram ${index + 1}`}>
  <div class="diagram-toolbar">
    <strong>{label}</strong>
    <div class="controls">
      <button onclick={() => changeZoom(-0.25)} disabled={!rendered || (!fit && zoom <= 0.25)} aria-label="Zoom out">−</button>
      <span class="zoom-label">{fit ? "Fit" : `${Math.round(zoom * 100)}%`}</span>
      <button onclick={() => changeZoom(0.25)} disabled={!rendered || (!fit && zoom >= 4)} aria-label="Zoom in">+</button>
      <button onclick={() => { fit = true; zoom = 1; }} disabled={!rendered}>Fit</button>
      <button onclick={exportSvg} disabled={!rendered}>Export SVG</button>
    </div>
  </div>
  {#if error}
    <div class="diagram-error" role="alert">
      <strong>Could not render this diagram</strong>
      <pre>{error}</pre>
      <button onclick={() => revision++}>Retry</button>
    </div>
  {:else if rendered}
    <!-- Keyboard focus lets users scroll diagrams wider than the viewport. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="diagram-viewport" tabindex="0" role="region" aria-label={`${label} preview`}>
      <img src={rendered.url} alt={`${label} diagram ${index + 1}; text available under Source`}
        style:width={fit ? `min(100%, ${rendered.width}px)` : `${rendered.width * zoom}px`}
        onerror={() => { error = "The diagram image could not be displayed."; }} />
    </div>
  {:else}
    <div class="diagram-loading" role="status">{visible ? `Rendering ${label}…` : "Diagram preview loads when visible"}</div>
  {/if}
  <details open={!!error}>
    <summary>Source</summary>
    <button class="copy-source" onclick={copySource}>Copy source</button>
    <pre class="diagram-source"><code>{diagram.source}</code></pre>
  </details>
  {#if actionMessage}<p class="action-message" role="status">{actionMessage}</p>{/if}
</section>

<style>
  .diagram { margin: 20px 0; border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; background: var(--bg-elev); color: var(--fg); font-size: 13px; }
  .diagram-toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; padding: 9px 12px; border-bottom: 1px solid var(--border-subtle); background: var(--bg-chrome); }
  .controls { display: flex; align-items: center; gap: 5px; flex-wrap: wrap; }
  button { border: 1px solid var(--border); background: var(--bg-elev); border-radius: var(--radius); padding: 5px 9px; cursor: pointer; }
  button:hover { background: var(--control-hover); }
  button:disabled { opacity: 0.45; cursor: default; }
  .zoom-label { min-width: 40px; text-align: center; font-variant-numeric: tabular-nums; }
  .diagram-viewport { max-height: 75vh; min-height: 100px; overflow: auto; padding: 24px; background: white; color-scheme: light; }
  img { display: block; height: auto; max-width: none; margin: 0 auto; }
  .diagram-loading { padding: 40px 24px; color: var(--fg-muted); }
  .diagram-error { padding: 16px; color: var(--danger); }
  .diagram-error pre { white-space: pre-wrap; overflow-wrap: anywhere; font: inherit; }
  details { padding: 10px 12px; border-top: 1px solid var(--border-subtle); }
  summary { cursor: pointer; width: fit-content; color: var(--fg-muted); }
  .diagram-source { overflow: auto; max-height: 420px; padding: 12px; background: var(--bg); tab-size: 2; font-size: 12px; }
  .copy-source { margin-top: 10px; }
  .action-message { margin: 0; padding: 8px 12px; color: var(--fg-muted); }
</style>
