import { SvelteSet } from "svelte/reactivity";
import { api } from "./api";
import { errorText } from "./format";
import type { Album, Counts, Filter, ImportProgress, ImportRecord, Mark, MarkSlot, Sort, Tag, View } from "./types";

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
  query = $state({ dateFrom: "", dateTo: "", minRating: 0, camera: "", origin: "", text: "", tagIds: [] as number[] });
  sort = $state<Sort>("takenDesc");
  thumbSize = $state(180);

  tags = $state.raw<Tag[]>([]);
  albums = $state.raw<Album[]>([]);
  cameras = $state.raw<string[]>([]);
  counts = $state.raw<Counts>({ all: 0, favorites: 0, trash: 0, marked: 0 });
  marks = $state.raw<MarkSlot[]>([]);
  sources = $state.raw<string[]>([]);
  imports = $state.raw<ImportRecord[]>([]);

  selection = new SvelteSet<number>();
  /** Bumped whenever photo data changes, so lists re-fetch. */
  version = $state(0);

  importOpen = $state(false);
  /** Source folder to prefill when the import dialog opens. */
  importPreset = $state("");
  progress = $state.raw<ImportProgress | null>(null);
  toasts = $state<Toast[]>([]);
  /** Mark whose batch is running, with copy progress. */
  runningMark = $state<Mark | null>(null);
  markProgress = $state<{ done: number; total: number } | null>(null);

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
      origin: q.origin || undefined,
      text: q.text || undefined,
      mark: v.kind === "mark" ? v.mark : undefined,
      tagIds,
      albumId: v.kind === "album" ? v.id : undefined,
      importId: v.kind === "import" ? v.id : undefined,
      favorite: v.kind === "favorites" ? true : undefined,
      trashed: v.kind === "trash",
    };
  }

  get hasQuery(): boolean {
    const q = this.query;
    return !!(q.dateFrom || q.dateTo || q.minRating || q.camera || q.origin || q.text || q.tagIds.length);
  }

  async init() {
    await api.onImportProgress((p) => this.#onProgress(p));
    await api.onMarkProgress((p) => (this.markProgress = { done: p.done, total: p.total }));
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

  openImport(source = "") {
    this.importPreset = source;
    this.importOpen = true;
  }

  setView(view: View) {
    this.view = view;
    this.selection.clear();
  }

  clearQuery() {
    // Mutate in place: components hold a reference to `query`.
    Object.assign(this.query, { dateFrom: "", dateTo: "", minRating: 0, camera: "", origin: "", text: "", tagIds: [] });
  }

  async refreshMeta() {
    const [tags, albums, cameras, counts, imports, marks, sources] = await Promise.all([
      api.listTags(),
      api.listAlbums(),
      api.listCameras(),
      api.counts(),
      api.listImports(),
      api.listMarks(),
      api.listSources(),
    ]);
    this.marks = marks;
    this.sources = sources;
    this.imports = imports;
    this.tags = tags;
    this.albums = albums;
    this.cameras = cameras;
    this.counts = counts;
    // Leave views that no longer exist.
    const v = this.view;
    if (
      (v.kind === "album" && !albums.some((a) => a.id === v.id)) ||
      (v.kind === "tag" && !tags.some((t) => t.id === v.id)) ||
      (v.kind === "import" && !imports.some((i) => i.id === v.id))
    ) {
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

  markSlot(mark: Mark): MarkSlot | undefined {
    return this.marks.find((s) => s.mark === mark);
  }

  /** "Delete", the label given to a custom mark, or "Mark 2". */
  markName(mark: Mark): string {
    return this.markSlot(mark)?.label || (mark === "x" ? "Delete" : `Mark ${mark}`);
  }

  /**
   * Marks photos with `mark`, or clears it if they all carry it already.
   * `current` gives the known marks of the photos.
   */
  toggleMark(ids: number[], mark: Mark, current: (id: number) => Mark | null | undefined) {
    const clear = ids.every((id) => current(id) === mark);
    return this.mutate(() => api.setMark(ids, clear ? null : mark));
  }

  /** Runs the batch action of `mark` on every photo carrying it. */
  async runMark(mark: Mark) {
    if (this.runningMark) return;
    this.runningMark = mark;
    this.markProgress = null;
    try {
      const r = await api.runMark(mark);
      const parts = [`${this.markName(mark)}: ${r.done} photo(s) done`];
      if (r.skipped) parts.push(`${r.skipped} already there`);
      if (r.errors.length) parts.push(`${r.errors.length} failed — ${r.errors.slice(0, 3).join("; ")}`);
      this.toast(parts.join(", "), r.errors.length > 0);
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.runningMark = null;
    this.markProgress = null;
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
