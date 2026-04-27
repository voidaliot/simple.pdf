<script lang="ts">
  import { onMount } from "svelte";
  import { theme, type ThemeChoice } from "../stores/theme.svelte";
  import { getPdfAssociation, setPdfAssociation } from "../lib/ipc";

  const themeOptions: { value: ThemeChoice; label: string }[] = [
    { value: "system", label: "System (auto)" },
    { value: "light",  label: "Light" },
    { value: "dark",   label: "Dark" },
  ];

  let assocEnabled = $state(false);
  let assocBusy = $state(false);

  onMount(async () => {
    try { assocEnabled = await getPdfAssociation(); } catch { /**/ }
  });

  async function toggleAssoc(e: Event) {
    const checked = (e.target as HTMLInputElement).checked;
    assocBusy = true;
    try {
      await setPdfAssociation(checked);
      assocEnabled = checked;
    } catch (err) {
      console.error("Association toggle failed:", err);
      // revert
      assocEnabled = !checked;
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
    <label class="row">
      <span>
        Open .pdf files with simple.pdf
        <small class="note">Sets HKCU association — no admin required.</small>
      </span>
      <input
        type="checkbox"
        class="toggle"
        checked={assocEnabled}
        disabled={assocBusy}
        onchange={toggleAssoc}
        aria-label="Associate .pdf files with simple.pdf"
      />
    </label>
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
  .toggle { width: 18px; height: 18px; cursor: pointer; flex-shrink: 0; }
  .toggle:disabled { opacity: 0.5; cursor: not-allowed; }
  .muted { color: var(--fg-muted); font-size: 13px; margin: 4px 0; }
</style>
