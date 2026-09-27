<script lang="ts">
  import { src } from "./api";
  import { fmtDay } from "./format";
  import type { PhotoList } from "./photoList.svelte";
  import { store } from "./store.svelte";

  let {
    list,
    onopen,
    onselect,
  }: {
    list: PhotoList;
    onopen: (index: number) => void;
    onselect: (index: number, e: MouseEvent) => void;
  } = $props();

  const HEADER = 44;
  const GAP = 6;
  const PAD = 16;
  const OVERSCAN = 800;

  let el: HTMLDivElement;
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  const cols = $derived(Math.max(1, Math.floor((width - PAD * 2 + GAP) / (store.thumbSize + GAP))));
  const cell = $derived(Math.max(40, (width - PAD * 2 - GAP * (cols - 1)) / cols));

  type Row =
    | { kind: "header"; top: number; day: string; count: number }
    | { kind: "photos"; top: number; start: number; n: number };

  const layout = $derived.by(() => {
    const rows: Row[] = [];
    let top = PAD;
    let index = 0;
    for (const b of list.buckets) {
      if (b.day) {
        rows.push({ kind: "header", top, day: b.day, count: b.count });
        top += HEADER;
      }
      for (let i = 0; i < b.count; i += cols) {
        rows.push({ kind: "photos", top, start: index + i, n: Math.min(cols, b.count - i) });
        top += cell + GAP;
      }
      index += b.count;
    }
    return { rows, height: top + PAD };
  });

  const visible = $derived.by(() => {
    const rows = layout.rows;
    const lo = scrollTop - OVERSCAN;
    const hi = scrollTop + height + OVERSCAN;
    let a = 0;
    let b = rows.length;
    while (a < b) {
      const m = (a + b) >> 1;
      if (rows[m].top + cell < lo) a = m + 1;
      else b = m;
    }
    const out: Row[] = [];
    for (let i = a; i < rows.length && rows[i].top < hi; i++) out.push(rows[i]);
    return out;
  });

  const cells = $derived(
    visible.flatMap((r) =>
      r.kind === "photos" ? Array.from({ length: r.n }, (_, k) => ({ i: r.start + k, top: r.top, col: k })) : [],
    ),
  );

  /** Day label of the section at the top of the viewport. */
  const currentDay = $derived.by(() => {
    let day = "";
    for (const r of layout.rows) {
      if (r.top > scrollTop + 8) break;
      if (r.kind === "header") day = r.day;
    }
    return scrollTop > HEADER ? day : "";
  });

  $effect(() => {
    if (cells.length) list.ensure(cells[0].i, cells[cells.length - 1].i);
  });

  export function scrollToTop() {
    if (el) el.scrollTop = 0;
    scrollTop = 0;
  }

  export function scrollToIndex(index: number) {
    const row = layout.rows.find((r) => r.kind === "photos" && index >= r.start && index < r.start + r.n);
    if (!row || !el) return;
    if (row.top < el.scrollTop || row.top + cell > el.scrollTop + height) {
      el.scrollTop = row.top - height / 2 + cell / 2;
    }
  }

  function dragStart(e: DragEvent, id: number) {
    const ids = store.selection.has(id) ? [...store.selection] : [id];
    e.dataTransfer?.setData("application/x-foto-ids", JSON.stringify(ids));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
  }
</script>

<div class="wrap">
<div class="grid" bind:this={el} bind:clientWidth={width} bind:clientHeight={height} onscroll={() => (scrollTop = el.scrollTop)}>
  <div class="spacer" style:height="{layout.height}px">
    {#each visible as row (row.top)}
      {#if row.kind === "header"}
        <h3 class="day" style:top="{row.top}px">{fmtDay(row.day)} <span>{row.count}</span></h3>
      {/if}
    {/each}
    {#each cells as c (c.i)}
      {@const p = list.get(c.i)}
      <button
        class="cell"
        class:selected={p && store.selection.has(p.id)}
        style:top="{c.top}px"
        style:left="{PAD + c.col * (cell + GAP)}px"
        style:width="{cell}px"
        style:height="{cell}px"
        draggable={!!p}
        ondragstart={(e) => p && dragStart(e, p.id)}
        onclick={(e) => onselect(c.i, e)}
        ondblclick={() => onopen(c.i)}>
        {#if p}
          <img src={src(p.thumb)} alt={p.origName} loading="lazy" decoding="async" draggable="false" />
          {#if p.favorite || p.rating}
            <span class="badges">
              {#if p.favorite}<span class="fav">♥</span>{/if}
              {#if p.rating}<span class="rating">{"★".repeat(p.rating)}</span>{/if}
            </span>
          {/if}
        {/if}
      </button>
    {/each}
  </div>
</div>
{#if currentDay}<div class="pill">{fmtDay(currentDay)}</div>{/if}
</div>

<style>
  .wrap { position: relative; height: 100%; }
  .grid { position: relative; overflow-y: auto; height: 100%; }
  .spacer { position: relative; }
  .day {
    position: absolute; left: 16px; right: 16px; height: 44px; margin: 0;
    display: flex; align-items: center; gap: 8px; font-size: 15px; font-weight: 600;
  }
  .day span { color: var(--muted); font-weight: 400; font-size: 13px; }
  .cell {
    position: absolute; padding: 0; border: 2px solid transparent; border-radius: 6px;
    background: var(--cell); overflow: hidden; cursor: default;
  }
  .cell img { width: 100%; height: 100%; object-fit: cover; display: block; }
  .cell.selected { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent); }
  .cell.selected img { opacity: 0.85; }
  .badges {
    position: absolute; left: 4px; bottom: 4px; display: flex; gap: 4px; padding: 1px 5px;
    border-radius: 4px; background: rgb(0 0 0 / 0.55); font-size: 11px;
  }
  .fav { color: #ff6b81; }
  .rating { color: var(--star); letter-spacing: -1px; }
  .pill {
    position: absolute; top: 8px; left: 16px;
    padding: 4px 10px; border-radius: 999px; background: var(--panel); border: 1px solid var(--border);
    font-size: 12px; box-shadow: 0 2px 8px rgb(0 0 0 / 0.3); pointer-events: none;
  }
</style>
