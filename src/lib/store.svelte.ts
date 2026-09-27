import { SvelteSet } from "svelte/reactivity";
import { api } from "./api";
import { errorText } from "./format";
import type { Album, Counts, Filter, ImportProgress, Sort, Tag, View } from "./types";

interface Toast {
  id: number;
  text: string;
  error: boolean;
}

class Store {
  ready = $state(false);
  root = $state<string | null>(null);
  view = $state<View>({ kind: "library" });

  /** Filter-bar fields; combined with the current view in `filter`. */
  query = $state({ dateFrom: "", dateTo: "", minRating: 0, camera: "", text: "", tagIds: [] as number[] });
  sort = $state<Sort>("takenDesc");
  thumbSize = $state(180);

  tags = $state.raw<Tag[]>([]);
  albums = $state.raw<Album[]>([]);
  cameras = $state.raw<string[]>([]);
  counts = $state.raw<Counts>({ all: 0, favorites: 0, trash: 0 });

  selection = new SvelteSet<number>();
  /** Bumped whenever photo data changes, so lists re-fetch. */
  version = $state(0);

  importOpen = $state(false);
  progress = $state.raw<ImportProgress | null>(null);
  toasts = $state<Toast[]>([]);

  #toastId = 0;
  #lastImportRefresh = 0;

  get filter(): Filter {
    const q = this.query;
    const v = this.view;
    const tagIds = [...q.tagIds];
    if (v.kind === "tag" && !tagIds.includes(v.id)) tagIds.push(v.id);
    return {
      dateFrom: q.dateFrom || undefined,
      dateTo: q.dateTo || undefined,
      minRating: q.minRating || undefined,
      camera: q.camera || undefined,
      text: q.text || undefined,
      tagIds,
      albumId: v.kind === "album" ? v.id : undefined,
      favorite: v.kind === "favorites" ? true : undefined,
      trashed: v.kind === "trash",
    };
  }

  get hasQuery(): boolean {
    const q = this.query;
    return !!(q.dateFrom || q.dateTo || q.minRating || q.camera || q.text || q.tagIds.length);
  }

  async init() {
    await api.onImportProgress((p) => this.#onProgress(p));
    try {
      const lib = await api.currentLibrary();
      this.root = lib?.root ?? null;
      if (this.root) await this.refreshMeta();
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.ready = true;
  }

  async openLibrary(path: string, create: boolean) {
    try {
      const lib = await api.openLibrary(path, create);
      this.root = lib.root;
      this.view = { kind: "library" };
      this.clearQuery();
      this.selection.clear();
      await this.refreshMeta();
      this.version++;
    } catch (e) {
      this.toast(errorText(e), true);
    }
  }

  setView(view: View) {
    this.view = view;
    this.selection.clear();
  }

  clearQuery() {
    // Mutate in place: components hold a reference to `query`.
    Object.assign(this.query, { dateFrom: "", dateTo: "", minRating: 0, camera: "", text: "", tagIds: [] });
  }

  async refreshMeta() {
    const [tags, albums, cameras, counts] = await Promise.all([
      api.listTags(),
      api.listAlbums(),
      api.listCameras(),
      api.counts(),
    ]);
    this.tags = tags;
    this.albums = albums;
    this.cameras = cameras;
    this.counts = counts;
    // Leave views that no longer exist.
    const v = this.view;
    if ((v.kind === "album" && !albums.some((a) => a.id === v.id)) || (v.kind === "tag" && !tags.some((t) => t.id === v.id))) {
      this.setView({ kind: "library" });
    }
  }

  /** Runs a mutation, reports errors, then refreshes lists and sidebar data. */
  async mutate(fn: () => Promise<unknown>, okText?: string) {
    try {
      await fn();
      if (okText) this.toast(okText);
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.version++;
    await this.refreshMeta().catch(() => {});
  }

  toast(text: string, error = false) {
    const id = ++this.#toastId;
    this.toasts.push({ id, text, error });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), error ? 6000 : 2500);
  }

  #onProgress(p: ImportProgress) {
    this.progress = p;
    const now = Date.now();
    if (p.finished || (p.added > 0 && now - this.#lastImportRefresh > 2000)) {
      this.#lastImportRefresh = now;
      this.version++;
      this.refreshMeta().catch(() => {});
    }
  }
}

export const store = new Store();
