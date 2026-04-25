<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openDocument } from "../lib/ipc";
  import { tabs } from "../stores/tabs.svelte";

  let filter = $state("");
  let recents = $state<Array<{ path: string; title: string }>>([]);

  async function pickAndOpen() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof selected === "string") {
      const doc = await openDocument(selected);
      tabs.openDoc({ id: doc.id, path: doc.path, title: doc.title, pageCount: doc.page_count });
    }
  }
</script>

<section class="home">
  <header>
    <h1>simple<span class="dot">.</span>pdf</h1>
    <p class="tagline">Fast, small, modern PDF reader.</p>
  </header>

  <div class="actions">
    <button class="primary" onclick={pickAndOpen}>Open file…</button>
    <button disabled title="Coming in M2">Open folder</button>
    <button disabled title="Coming in M2">Paste URL</button>
  </div>

  <div class="filter">
    <input
      type="search"
      placeholder="Search recent files"
      bind:value={filter}
      aria-label="Search recent files"
    />
  </div>

  <section class="recents" aria-label="Recent files">
    {#if recents.length === 0}
      <div class="empty">
        <p>No recent files yet.</p>
        <p class="hint">Open a PDF to get started.</p>
      </div>
    {:else}
      <div class="grid">
        {#each recents as r}
          <article class="card">
            <div class="thumb" aria-hidden="true"></div>
            <div class="meta">
              <h3>{r.title}</h3>
              <p>{r.path}</p>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </section>
</section>

<style>
  .home {
    height: 100%;
    overflow: auto;
    padding: 48px 64px;
    max-width: 1200px;
    margin: 0 auto;
  }
  header { margin-bottom: 32px; }
  h1 {
    font-size: 48px;
    font-weight: 300;
    margin: 0;
    letter-spacing: -0.02em;
  }
  .dot { color: var(--accent); }
  .tagline {
    color: var(--fg-muted);
    margin: 8px 0 0;
    font-size: 15px;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-bottom: 24px;
  }
  .actions button {
    padding: 8px 16px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    cursor: pointer;
    transition: background 120ms ease, border-color 120ms ease;
  }
  .actions button:hover:not(:disabled) { border-color: var(--accent); }
  .actions button:disabled { opacity: 0.5; cursor: not-allowed; }
  .actions .primary {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
  .filter { margin-bottom: 24px; }
  .filter input {
    width: 100%;
    max-width: 420px;
    padding: 8px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-elev);
    color: var(--fg);
  }
  .recents .empty {
    padding: 64px 0;
    color: var(--fg-muted);
    text-align: center;
  }
  .recents .empty p { margin: 4px 0; }
  .recents .empty .hint { font-size: 13px; opacity: 0.7; }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
  }
  .card {
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    cursor: pointer;
    transition: border-color 120ms ease, transform 120ms ease;
  }
  .card:hover { border-color: var(--accent); transform: translateY(-1px); }
  .thumb {
    aspect-ratio: 3 / 4;
    background: linear-gradient(180deg, #f3f3f3, #e5e5e5);
  }
  @media (prefers-color-scheme: dark) {
    .thumb { background: linear-gradient(180deg, #2a2a2a, #1a1a1a); }
  }
  .meta { padding: 10px 12px; }
  .meta h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta p {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
