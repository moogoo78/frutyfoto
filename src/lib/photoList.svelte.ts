import { api } from "./api";
import type { Bucket, Filter, Photo, Sort } from "./types";

const PAGE = 200;

/**
 * A lazily paged view of the photos matching a filter. Day buckets give the full
 * layout up front; photo rows are fetched page by page as they scroll into view.
 */
export class PhotoList {
  buckets = $state.raw<Bucket[]>([]);
  total = $state(0);
  /** False until the first reload finishes. */
  loaded = $state(false);
  pages = $state.raw<Record<number, Photo[]>>({});

  #filter: Filter = { tagIds: [], trashed: false };
  #sort: Sort = "takenDesc";
  #gen = 0;
  #loading = new Map<number, Promise<void>>();

  /** Switches to a new filter/sort, dropping all cached pages. */
  async reload(filter: Filter, sort: Sort) {
    const gen = ++this.#gen;
    this.#filter = filter;
    this.#sort = sort;
    this.#loading.clear();
    const buckets = await api.dateBuckets(filter, sort);
    if (gen !== this.#gen) return;
    this.#setBuckets(buckets);
    this.pages = {};
    this.loaded = true;
  }

  /** Re-fetches the current filter while keeping loaded pages on screen until replaced. */
  async refresh() {
    const gen = ++this.#gen;
    this.#loading.clear();
    const loaded = Object.keys(this.pages).map(Number);
    const [buckets, ...pages] = await Promise.all([
      api.dateBuckets(this.#filter, this.#sort),
      ...loaded.map((p) => api.listPhotos(this.#filter, this.#sort, p * PAGE, PAGE)),
    ]);
    if (gen !== this.#gen) return;
    this.#setBuckets(buckets as Bucket[]);
    const next: Record<number, Photo[]> = {};
    loaded.forEach((p, i) => (next[p] = pages[i] as Photo[]));
    this.pages = next;
  }

  get(i: number): Photo | undefined {
    return this.pages[Math.floor(i / PAGE)]?.[i % PAGE];
  }

  /** Makes sure indexes `from..=to` are loaded (or loading). */
  ensure(from: number, to: number) {
    for (let p = Math.floor(Math.max(0, from) / PAGE); p <= Math.floor(to / PAGE); p++) this.#load(p);
  }

  async fetch(i: number): Promise<Photo | undefined> {
    if (i < 0 || i >= this.total) return undefined;
    await this.#load(Math.floor(i / PAGE));
    return this.get(i);
  }

  /** All ids matching the current filter (for select-all). */
  async allIds(): Promise<number[]> {
    const photos = await api.listPhotos(this.#filter, this.#sort, 0, this.total);
    return photos.map((p) => p.id);
  }

  /** Ids at indexes `start..start+count` (e.g. one day's photos). */
  async idsInRange(start: number, count: number): Promise<number[]> {
    const photos = await api.listPhotos(this.#filter, this.#sort, start, count);
    return photos.map((p) => p.id);
  }

  #setBuckets(buckets: Bucket[]) {
    this.buckets = buckets;
    this.total = buckets.reduce((n, b) => n + b.count, 0);
  }

  #load(page: number): Promise<void> {
    if (this.pages[page]) return Promise.resolve();
    const pending = this.#loading.get(page);
    if (pending) return pending;
    const gen = this.#gen;
    const promise = api.listPhotos(this.#filter, this.#sort, page * PAGE, PAGE).then(
      (photos) => {
        if (gen !== this.#gen) return;
        this.pages = { ...this.pages, [page]: photos };
      },
      () => {
        this.#loading.delete(page);
      },
    );
    this.#loading.set(page, promise);
    return promise;
  }
}
