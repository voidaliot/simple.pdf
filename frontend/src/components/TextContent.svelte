<script lang="ts">
  import { onMount } from "svelte";
  import { parseMarkdown } from "../lib/markdown";
  import { sanitizeMarkdown } from "../lib/markdownSanitize";
  import { openExternalUri, type TextDocument } from "../lib/ipc";
  import { notifications } from "../stores/notifications.svelte";

  let { document }: { document: TextDocument } = $props();
  let root: HTMLElement;
  // The parent keys this component by the immutable source snapshot.
  const content = $derived.by(() => {
    try {
      return { html: sanitizeMarkdown(parseMarkdown(document.source)), error: "" };
    } catch (error) {
      return { html: "", error: error instanceof Error ? error.message : String(error) };
    }
  });

  onMount(() => {
    const onClick = (event: MouseEvent) => {
      const anchor = (event.target as Element).closest("a");
      if (!anchor) return;
      event.preventDefault();
      const href = anchor.getAttribute("href") ?? "";
      if (/^(https?:|mailto:)/i.test(href)) void openExternalUri(href).catch(notifications.error);
      else notifications.error("Only HTTP, HTTPS, and email links can be opened from Markdown.");
    };
    root.addEventListener("click", onClick);
    return () => {
      root.removeEventListener("click", onClick);
    };
  });
</script>

<article class="text-content" bind:this={root}>
  {#if content.error}<p role="alert">{content.error}</p>
  {:else if !document.source.trim()}<p class="empty">This document is empty.</p>
  {:else}{@html content.html}{/if}
</article>

<style>
  .text-content { max-width: 1120px; padding: 28px 36px 80px; margin: 0 auto; line-height: 1.7; overflow-wrap: anywhere; }
  .empty { color: var(--fg-muted); }
  .text-content :global(h1), .text-content :global(h2), .text-content :global(h3) { line-height: 1.3; margin: 1.5em 0 0.6em; }
  .text-content :global(h1:first-child) { margin-top: 0; }
  .text-content :global(pre) { padding: 16px; overflow: auto; background: var(--bg-elev); border-radius: var(--radius); }
  .text-content :global(code) { font-family: Consolas, monospace; }
  .text-content :global(a) { color: var(--accent); }
  .text-content :global(blockquote) { margin-left: 0; padding-left: 18px; border-left: 3px solid var(--border); color: var(--fg-muted); }
  .text-content :global(table) { border-collapse: collapse; display: block; overflow: auto; }
  .text-content :global(th), .text-content :global(td) { border: 1px solid var(--border); padding: 8px 12px; }
  .text-content :global(hr) { border: 0; border-top: 1px solid var(--border); margin: 24px 0; }
  @media (max-width: 800px) { .text-content { padding: 20px 16px 48px; } }
</style>
