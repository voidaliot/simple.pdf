<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { theme, type ThemeChoice } from "../stores/theme.svelte";
  import { configurePdfAssociation, getPdfAssociation } from "../lib/ipc";

  const themeOptions: { value: ThemeChoice; label: string }[] = [
    { value: "system", label: "System (auto)" },
    { value: "light",  label: "Light" },
    { value: "dark",   label: "Dark" },
  ];

  let assocEnabled = $state(false);
  let assocBusy = $state(false);
  let assocError = $state("");

  async function refreshAssociation() {
    try {
      assocEnabled = await getPdfAssociation();
      assocError = "";
    } catch (error) {
      assocError = error instanceof Error ? error.message : String(error);
    }
  }

  onMount(() => {
    let unlistenFocus: (() => void) | undefined;
    let disposed = false;
    void refreshAssociation();
    void getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void refreshAssociation();
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenFocus = unlisten;
    });
    return () => {
      disposed = true;
      unlistenFocus?.();
    };
  });

  async function configureAssoc() {
    assocBusy = true;
    assocError = "";
    try {
      await configurePdfAssociation();
    } catch (error) {
      assocError = error instanceof Error ? error.message : String(error);
    } finally {
      assocBusy = false;
    }
  }
</script>

<section class="settings">
  <h1>Settings</h1>

  <div class="group">
    <h2>Appearance</h2>
    <label class="row">
      <span>Theme</span>
      <select value={theme.choice} onchange={(e) => theme.set((e.target as HTMLSelectElement).value as ThemeChoice)}>
        {#each themeOptions as opt}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="group">
    <h2>File Associations</h2>
    <div class="row">
      <span>
        Default PDF app
        <small class="note">
          {assocEnabled
            ? "simple.pdf is currently the Windows default."
            : "Choose simple.pdf from Windows Default Apps. No admin access is required."}
        </small>
      </span>
      <button
        type="button"
        class="association-button"
        disabled={assocBusy}
        onclick={configureAssoc}
      >{assocBusy ? "Opening…" : assocEnabled ? "Change…" : "Choose…"}</button>
    </div>
    {#if assocError}<p class="association-error" role="alert">{assocError}</p>{/if}
  </div>

  <div class="group">
    <h2>About</h2>
    <p class="muted">simple.pdf — fast, small, modern PDF reader.</p>
    <p class="muted">Built with Tauri 2, PDFium, Svelte 5.</p>
  </div>
</section>

<style>
  .settings {
    height: 100%; overflow: auto; padding: 48px 64px;
    max-width: 640px; margin: 0 auto;
  }
  h1 { font-size: 28px; font-weight: 300; margin: 0 0 32px; letter-spacing: -0.01em; }
  h2 { font-size: 14px; font-weight: 600; text-transform: uppercase;
       letter-spacing: 0.06em; color: var(--fg-muted); margin: 0 0 12px; }
  .group { margin-bottom: 32px; }
  .row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 0; border-bottom: 1px solid var(--border); font-size: 14px;
    gap: 12px;
  }
  .row span { color: var(--fg); display: flex; flex-direction: column; gap: 2px; }
  .note { font-size: 11px; color: var(--fg-muted); font-weight: 400; }
  select {
    padding: 5px 10px; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg-elev); color: var(--fg); font: inherit; font-size: 13px; cursor: pointer;
  }
  .association-button {
    padding: 6px 12px; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--bg-elev); color: var(--fg); font: inherit; font-size: 13px;
    cursor: pointer; flex-shrink: 0;
  }
  .association-button:hover:not(:disabled) { border-color: var(--accent); }
  .association-button:disabled { opacity: 0.5; cursor: not-allowed; }
  .association-error { margin: 8px 0 0; color: var(--danger); font-size: 12px; }
  .muted { color: var(--fg-muted); font-size: 13px; margin: 4px 0; }
</style>
