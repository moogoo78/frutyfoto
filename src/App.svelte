<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "./lib/store.svelte";
  import Duplicates from "./views/Duplicates.svelte";
  import ImportDialog from "./views/ImportDialog.svelte";
  import Library from "./views/Library.svelte";
  import Sidebar from "./views/Sidebar.svelte";
  import Welcome from "./views/Welcome.svelte";

  onMount(() => {
    store.init();
  });
</script>

{#if store.ready}
  {#if !store.root}
    <Welcome />
  {:else}
    <div class="app">
      <Sidebar />
      <main>
        {#key store.root}
          {#if store.view.kind === "duplicates"}
            <Duplicates />
          {:else}
            <Library />
          {/if}
        {/key}
      </main>
    </div>
    <ImportDialog />
  {/if}
{/if}

<div class="toasts">
  {#each store.toasts as t (t.id)}
    <div class="toast" class:error={t.error}>{t.text}</div>
  {/each}
</div>

<style>
  .app { display: flex; height: 100vh; }
  main { flex: 1; min-width: 0; }
  .toasts { position: fixed; bottom: 16px; right: 16px; z-index: 100; display: flex; flex-direction: column; gap: 8px; }
  .toast { padding: 10px 14px; border-radius: 8px; background: var(--panel); border: 1px solid var(--border);
    box-shadow: 0 6px 20px rgb(0 0 0 / 0.35); font-size: 13px; max-width: 420px; }
  .toast.error { border-color: var(--danger); color: var(--danger); }
</style>
