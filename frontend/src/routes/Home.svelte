<script lang="ts">
  import { recents, type RecentEntry } from "../stores/recents.svelte";
  import { pickAndOpen, openPath } from "../lib/open";

  let filter = $state("");

  const sorted = $derived([
    ...recents.entries.filter((e) => e.pinned),
    ...recents.entries.filter((e) => !e.pinned),
  ]);

  const filtered = $derived(
    filter.trim()
      ? sorted.filter(
          (e) =>
            e.title.toLowerCase().includes(filter.toLowerCase()) ||
            e.path.toLowerCase().includes(filter.toLowerCase())
        )
      : sorted
  );

  let contextMenu = $state<{ x: number; y: number; entry: RecentEntry } | null>(
    null
  );

  function onContextMenu(e: MouseEvent, entry: RecentEntry) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, entry };
  }

  function closeContext() {
    contextMenu = null;
  }
</script>

<svelte:window onclick={closeContext} />

<section class="home">
  <header>
    <h1>simple<span class="dot">.</span>pdf</h1>
    <p class="tagline">Fast, small, modern PDF reader.</p>
  </header>

  <div class="actions">
    <button class="primary" onclick={pickAndOpen}>Open file…</button>
    <button disabled title="Coming soon">Open folder</button>
    <button disabled title="Coming soon">Paste URL</button>
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
    {#if recents.entries.length === 0}
      <div class="empty">
        <p>No recent files yet.</p>
        <p class="hint">Open a PDF to get started.</p>
      </div>
    {:else if filtered.length === 0}
      <div class="empty">
        <p>No matches for "{filter}".</p>
      </div>
    {:else}
      <div class="grid">
        {#each filtered as entry (entry.path)}
          <div
            class="card"
            class:pinned={entry.pinned}
            role="button"
            tabindex="0"
            title={entry.path}
            onclick={() => openPath(entry.path).catch(console.error)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ")
                openPath(entry.path).catch(console.error);
            }}
            oncontextmenu={(e) => onContextMenu(e, entry)}
          >
            <div class="thumb" aria-hidden="true">
              {#if entry.pinned}<span class="pin-badge">pinned</span>{/if}
            </div>
            <div class="meta">
              <h3>{entry.title}</h3>
              <p>{entry.path}</p>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</section>

{#if contextMenu}
  <div
    class="context-menu"
    style:left="{contextMenu.x}px"
    style:top="{contextMenu.y}px"
    role="menu"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <button
      role="menuitem"
      onclick={() => {
        recents.togglePin(contextMenu!.entry.path);
        closeContext();
      }}
    >
      {contextMenu.entry.pinned ? "Unpin" : "Pin to top"}
    </button>
    <button
      role="menuitem"
      onclick={() => {
        recents.remove(contextMenu!.entry.path);
        closeContext();
      }}
    >
      Remove from recents
    </button>
  </div>
{/if}

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
    user-select: none;
  }
  .card:hover { border-color: var(--accent); transform: translateY(-1px); }
  .card.pinned { border-color: var(--accent); }
  .thumb {
    position: relative;
    aspect-ratio: 3 / 4;
    background: linear-gradient(180deg, #f3f3f3, #e5e5e5);
  }
  @media (prefers-color-scheme: dark) {
    .thumb { background: linear-gradient(180deg, #2a2a2a, #1a1a1a); }
  }
  .pin-badge {
    position: absolute;
    top: 6px;
    right: 6px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--accent);
    background: var(--bg-elev);
    border: 1px solid var(--accent);
    border-radius: 3px;
    padding: 2px 5px;
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
  .context-menu {
    position: fixed;
    z-index: 200;
    background: var(--bg-elev);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 4px 0;
    min-width: 180px;
  }
  .context-menu button {
    display: block;
    width: 100%;
    padding: 8px 16px;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    font: inherit;
    font-size: 13px;
    color: var(--fg);
  }
  .context-menu button:hover { background: var(--bg-chrome); }
</style>
