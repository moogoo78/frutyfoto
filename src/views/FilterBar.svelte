<script lang="ts">
  import { store } from "../lib/store.svelte";

  const q = store.query;
  const tagName = (id: number) => store.tags.find((t) => t.id === id)?.name ?? "?";
</script>

<div class="filters">
  <label>From <input type="date" bind:value={q.dateFrom} /></label>
  <label>To <input type="date" bind:value={q.dateTo} /></label>
  <label>
    Rating
    <select bind:value={q.minRating}>
      <option value={0}>Any</option>
      {#each [1, 2, 3, 4, 5] as n}<option value={n}>{"★".repeat(n)}+</option>{/each}
    </select>
  </label>
  {#if store.cameras.length}
    <label>
      Camera
      <select bind:value={q.camera}>
        <option value="">Any</option>
        {#each store.cameras as c}<option value={c}>{c}</option>{/each}
      </select>
    </label>
  {/if}
  <label>
    Source
    <select bind:value={q.origin}>
      <option value="">Any</option>
      <option value="camera">Camera</option>
      <option value="phone">Phone</option>
      <option value="unknown">Unknown</option>
      {#each store.sources as s}<option value={s}>{s}</option>{/each}
    </select>
  </label>
  <label>
    Tags
    <select
      onchange={(e) => {
        const id = Number(e.currentTarget.value);
        e.currentTarget.value = "";
        if (id && !q.tagIds.includes(id)) q.tagIds.push(id);
      }}>
      <option value="">Add…</option>
      {#each store.tags.filter((t) => !q.tagIds.includes(t.id)) as t (t.id)}<option value={t.id}>{t.name}</option>{/each}
    </select>
  </label>
  {#each q.tagIds as id (id)}
    <span class="chip">{tagName(id)}<button onclick={() => (q.tagIds = q.tagIds.filter((t) => t !== id))} aria-label="Remove">×</button></span>
  {/each}
  <input class="search" type="search" placeholder="File name…" bind:value={q.text} />
  {#if store.hasQuery}<button onclick={() => store.clearQuery()}>Reset</button>{/if}
</div>

<style>
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px; padding: 10px 16px;
    border-bottom: 1px solid var(--border); background: var(--panel); font-size: 13px; }
  label { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .search { width: 160px; }
</style>
