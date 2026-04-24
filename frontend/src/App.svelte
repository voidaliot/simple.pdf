<script lang="ts">
  import TabBar from "./components/TabBar.svelte";
  import Home from "./routes/Home.svelte";
  import Viewer from "./routes/Viewer.svelte";
  import { tabs } from "./stores/tabs.svelte";
</script>

<div class="shell">
  <TabBar />
  <main class="content">
    {#if tabs.active}
      {#if tabs.active.kind === "home"}
        <Home />
      {:else}
        <Viewer tab={tabs.active} />
      {/if}
    {/if}
  </main>
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
  }
  .content {
    flex: 1;
    overflow: hidden;
    background: var(--bg);
  }
</style>
