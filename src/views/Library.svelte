<script lang="ts">
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { basename, fmtDateTime } from "../lib/format";
  import { PhotoList } from "../lib/photoList.svelte";
  import Stars from "../lib/Stars.svelte";
  import { store } from "../lib/store.svelte";
  import SourceInput from "../lib/SourceInput.svelte";
  import TagInput from "../lib/TagInput.svelte";
  import { MARKS, type Mark } from "../lib/types";
  import VirtualGrid from "../lib/VirtualGrid.svelte";
  import FilterBar from "./FilterBar.svelte";
  import Viewer from "./Viewer.svelte";

  const list = new PhotoList();
  let grid: VirtualGrid | undefined = $state();
  let tagInput: TagInput | undefined = $state();
  let viewerIndex = $state<number | null>(null);
  let anchor: number | null = null;
  let showFilters = $state(false);
  /** Name being typed for a new album made from the selection; null when closed. */
  let newAlbumName = $state<string | null>(null);

  const key = $derived(JSON.stringify([store.filter, store.sort]));
  let loadedKey = "";

  $effect(() => {
    const k = key;
    void store.version;
    untrack(() => {
      if (k !== loadedKey) {
        loadedKey = k;
        anchor = null;
        list.reload(store.filter, store.sort);
        grid?.scrollToTop();
      } else {
        list.refresh();
      }
    });
  });

  // Keep the grid scrolled to the photo shown in the viewer.
  $effect(() => {
    if (viewerIndex != null) grid?.scrollToIndex(viewerIndex);
  });

  const currentImport = $derived(
    store.view.kind === "import" ? store.imports.find((i) => i.id === (store.view as { id: number }).id) : undefined,
  );

  const title = $derived.by(() => {
    const v = store.view;
    switch (v.kind) {
      case "favorites": return "Favorites";
      case "trash": return "Trash";
      case "album": return store.albums.find((a) => a.id === v.id)?.name ?? "Album";
      case "tag": return `#${store.tags.find((t) => t.id === v.id)?.name ?? ""}`;
      case "import": return `Import: ${basename(currentImport?.sourceDir ?? "")}`;
      case "mark": return store.markName(v.mark);
      default: return "All photos";
    }
  });

  const selected = $derived([...store.selection]);
  const inTrash = $derived(store.view.kind === "trash");
  const albumId = $derived(store.view.kind === "album" ? store.view.id : null);
  const markView = $derived(store.view.kind === "mark" ? store.view.mark : null);

  function select(i: number, e: MouseEvent) {
    const p = list.get(i);
    if (!p) return;
    if (e.shiftKey && anchor != null) {
      if (!(e.ctrlKey || e.metaKey)) store.selection.clear();
      const [a, b] = anchor < i ? [anchor, i] : [i, anchor];
      for (let k = a; k <= b; k++) {
        const q = list.get(k);
        if (q) store.selection.add(q.id);
      }
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      if (store.selection.has(p.id)) store.selection.delete(p.id);
      else store.selection.add(p.id);
    } else {
      store.selection.clear();
      store.selection.add(p.id);
    }
    anchor = i;
  }

  async function selectAll() {
    for (const id of await list.allIds()) store.selection.add(id);
  }

  async function emptyTrash() {
    const msg = `Remove ${store.counts.trash} photo(s) from the library? The files are moved to the system Trash, where you can still recover them.`;
    if (!(await ask(msg, { title: "Empty trash", kind: "warning" }))) return;
    store.mutate(() => api.emptyTrash(), "Moved to the system Trash");
  }

  function toggleMark(ids: number[], mark: Mark) {
    store.toggleMark(ids, mark, (id) => findLoaded(id)?.mark);
  }

  /** Suggests an album name from the selected photos' dates. */
  function suggestAlbumName(): string {
    const days = selected.map((id) => findLoaded(id)?.takenAt.slice(0, 10)).filter(Boolean).sort() as string[];
    if (!days.length) return "";
    const [first, last] = [days[0], days[days.length - 1]];
    return first === last ? first : `${first} – ${last}`;
  }

  async function createAlbumFromSelection() {
    const name = newAlbumName?.trim();
    newAlbumName = null;
    if (!name) return;
    const ids = selected;
    let id = 0;
    await store.mutate(async () => {
      id = await api.createAlbum(name);
      await api.addToAlbum(id, ids);
    }, `Album “${name}” created with ${ids.length} photo(s)`);
    if (id) store.selection.clear();
  }

  async function dayAction(action: "select" | "album", day: string, start: number, count: number) {
    const ids = await list.idsInRange(start, count);
    store.selection.clear();
    for (const id of ids) store.selection.add(id);
    anchor = start;
    if (action === "album") newAlbumName = day;
  }

  function trashOrRestore(ids: number[]) {
    store.selection.clear();
    if (inTrash) store.mutate(() => api.restorePhotos(ids), `Restored ${ids.length} photo(s)`);
    else store.mutate(() => api.trashPhotos(ids), `Moved ${ids.length} photo(s) to trash`);
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (viewerIndex != null || store.importOpen || t.closest("input, select, textarea")) return;
    const ids = selected;
    if ((e.ctrlKey || e.metaKey) && e.key === "a") selectAll();
    else if (e.key === "Escape") store.selection.clear();
    else if (e.key === "Enter" && anchor != null) viewerIndex = anchor;
    else if (!ids.length) return;
    else if (e.shiftKey && /^Digit[0-5]$/.test(e.code)) store.mutate(() => api.setRating(ids, Number(e.code.slice(5))));
    else if (e.key === "x" || /^[1-4]$/.test(e.key)) toggleMark(ids, e.key as Mark);
    else if (e.key === "f") {
      const allFav = ids.every((id) => findLoaded(id)?.favorite);
      store.mutate(() => api.setFavorite(ids, !allFav));
    } else if (e.key === "t") tagInput?.focus();
    else if (e.key === "Delete") trashOrRestore(ids);
    else return;
    e.preventDefault();
  }

  function findLoaded(id: number) {
    for (const page of Object.values(list.pages)) {
      const p = page.find((q) => q.id === id);
      if (p) return p;
    }
  }

  const focusSelect = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };

  const singleRating = $derived(selected.length === 1 ? (findLoaded(selected[0])?.rating ?? 0) : 0);
</script>

<svelte:window {onkeydown} />

<section class="library">
  <header>
    {#if currentImport}
      <button class="link" onclick={() => store.setView({ kind: "imports" })} title="Back to import history">‹ Imports</button>
    {/if}
    {#if markView}
      <button class="link" onclick={() => store.setView({ kind: "marks" })} title="Back to marks">‹ Marks</button>
      <span class="mark mark-{markView}">{markView}</span>
    {/if}
    <h1 title={currentImport?.sourceDir}>{title}</h1>
    <span class="count">
      {list.total.toLocaleString()} photos{#if currentImport} · imported {fmtDateTime(currentImport.startedAt)}{/if}
    </span>
    <span class="grow"></span>
    {#if markView && list.total > 0}
      <button class:danger={markView === "x"} class:primary={markView !== "x"}
        disabled={!!store.runningMark || !store.markSlot(markView)?.kind}
        title={store.markSlot(markView)?.kind ? "" : "Set up this mark in the Marks view first"}
        onclick={() => store.runMark(markView)}>
        {store.runningMark === markView ? "Running…" : markView === "x" ? `Move ${list.total} to trash` : `Run on ${list.total}`}
      </button>
    {/if}
    {#if inTrash && store.counts.trash > 0}
      <button class="danger" onclick={emptyTrash}>Empty trash</button>
    {/if}
    <button class:active={showFilters || store.hasQuery} onclick={() => (showFilters = !showFilters)}>
      Filter{store.hasQuery ? " •" : ""}
    </button>
    <select bind:value={store.sort} title="Sort">
      <option value="takenDesc">Newest first</option>
      <option value="takenAsc">Oldest first</option>
      <option value="importedDesc">Recently imported</option>
      <option value="ratingDesc">Highest rated</option>
    </select>
    <input type="range" min="100" max="360" step="20" bind:value={store.thumbSize} title="Thumbnail size" />
  </header>

  {#if showFilters}<FilterBar />{/if}

  {#if selected.length}
    <div class="selbar">
      <strong>{selected.length} selected</strong>
      {#if !inTrash}
        <span class="marks" role="group" aria-label="Mark">
          {#each MARKS as m}
            <button class="mark mark-{m}" title="{store.markName(m)} ({m})" onclick={() => toggleMark(selected, m)}>{m}</button>
          {/each}
          <button class="unmark" title="Clear mark" onclick={() => store.mutate(() => api.setMark(selected, null))}>∅</button>
        </span>
        <Stars value={singleRating} onchange={(n) => store.mutate(() => api.setRating(selected, n))} />
        <button onclick={() => store.mutate(() => api.setFavorite(selected, true))}>♥ Favorite</button>
        <button onclick={() => store.mutate(() => api.setFavorite(selected, false))}>♡ Unfavorite</button>
        <TagInput bind:this={tagInput} onadd={(names) => store.mutate(() => api.addTags(selected, names), "Tagged")} />
        <SourceInput onset={(source) => store.mutate(() => api.setSource(selected, source), source ? `Source set to ${source}` : "Source cleared")} />
        {#if newAlbumName != null}
          <input
            class="albumname"
            placeholder="New album name"
            bind:value={newAlbumName}
            use:focusSelect
            onblur={() => (newAlbumName = null)}
            onkeydown={(e) => {
              if (e.key === "Enter") createAlbumFromSelection();
              if (e.key === "Escape") newAlbumName = null;
              e.stopPropagation();
            }} />
        {:else}
          <button onclick={() => (newAlbumName = suggestAlbumName())}>＋ New album</button>
        {/if}
        {#if store.albums.length}
          <select
            onchange={(e) => {
              const id = Number(e.currentTarget.value);
              e.currentTarget.value = "";
              if (id) store.mutate(() => api.addToAlbum(id, selected), "Added to album");
            }}>
            <option value="">Add to album…</option>
            {#each store.albums as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
          </select>
        {/if}
        {#if albumId != null}
          <button onclick={() => store.mutate(() => api.removeFromAlbum(albumId, selected))}>Remove from album</button>
          {#if selected.length === 1}
            <button onclick={() => store.mutate(() => api.setAlbumCover(albumId, selected[0]), "Cover set")}>Set as cover</button>
          {/if}
        {/if}
      {/if}
      <span class="grow"></span>
      <button class:danger={!inTrash} onclick={() => trashOrRestore(selected)}>{inTrash ? "Restore" : "Trash"}</button>
      <button onclick={() => store.selection.clear()}>Clear</button>
    </div>
  {/if}

  <div class="body">
    {#if !list.loaded}
      <div></div>
    {:else if list.total === 0}
      <div class="empty">
        {#if store.hasQuery}
          <p>No photos match the current filters.</p>
          <button onclick={() => store.clearQuery()}>Clear filters</button>
        {:else if store.view.kind === "library"}
          <p>Your library is empty.</p>
          <button class="primary" onclick={() => store.openImport()}>Import photos</button>
        {:else if inTrash}
          <p>Trash is empty.</p>
        {:else}
          <p>Nothing here yet.</p>
        {/if}
      </div>
    {:else}
      <VirtualGrid bind:this={grid} {list} onselect={select} onopen={(i) => (viewerIndex = i)}
        ondayaction={inTrash ? undefined : dayAction} showOrigin={albumId != null} />
    {/if}
  </div>
</section>

{#if viewerIndex != null}
  <Viewer {list} bind:index={viewerIndex} onclose={() => (viewerIndex = null)} />
{/if}

<style>
  .library { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .grow { flex: 1; }
  input[type="range"] { width: 100px; }
  .selbar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 16px;
    background: var(--accent-bg); border-bottom: 1px solid var(--border); }
  .body { flex: 1; min-height: 0; }
  .marks { display: inline-flex; gap: 3px; }
  .marks button { padding: 0 7px; height: 26px; line-height: 26px; border: none; }
  .marks .unmark { background: var(--cell); color: var(--muted); }
  .albumname { width: 200px; }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: var(--muted); }
</style>
