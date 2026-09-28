export interface Photo {
  id: number;
  hash: string;
  relPath: string;
  origName: string;
  sourcePath: string | null;
  takenAt: string;
  importedAt: string;
  width: number | null;
  height: number | null;
  fileSize: number;
  mime: string | null;
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  iso: number | null;
  fNumber: number | null;
  exposure: string | null;
  focalLen: number | null;
  gpsLat: number | null;
  gpsLon: number | null;
  rating: number;
  favorite: boolean;
  trashedAt: string | null;
  mark: Mark | null;
  /** Where the photo came from, set by the user (e.g. "LINE"). */
  source: string | null;
  /** Guessed from EXIF make/model. */
  device: "phone" | "camera" | "unknown";
  path: string;
  /** What the viewer shows: the original, or a JPEG preview for HEIC. */
  display: string;
  thumb: string;
}

export interface Tag {
  id: number;
  name: string;
  count: number;
}

export interface Album {
  id: number;
  name: string;
  createdAt: string;
  count: number;
  cover: string | null;
}

export interface PhotoDetail {
  photo: Photo;
  tags: Tag[];
  albums: Album[];
}

export interface Bucket {
  day: string;
  count: number;
}

export interface Counts {
  all: number;
  favorites: number;
  trash: number;
  marked: number;
}

export interface Filter {
  dateFrom?: string;
  dateTo?: string;
  tagIds: number[];
  albumId?: number;
  importId?: number;
  minRating?: number;
  favorite?: boolean;
  camera?: string;
  text?: string;
  /** A mark, or "any" for every marked photo. */
  mark?: string;
  /** The user-set source, else "phone" / "camera" / "unknown". */
  origin?: string;
  trashed: boolean;
}

export type Sort = "takenDesc" | "takenAsc" | "importedDesc" | "ratingDesc";

export interface ImportRecord {
  id: number;
  sourceDir: string;
  startedAt: string;
  /** null if the app quit before the import finished. */
  finishedAt: string | null;
  added: number;
  skippedDupes: number;
  failed: number;
  cancelled: boolean;
  /** Photos from this import still in the library. */
  photoCount: number;
  cover: string | null;
}

export interface ImportProgress {
  importId: number;
  total: number;
  done: number;
  added: number;
  skipped: number;
  failed: number;
  current: string;
  finished: boolean;
  cancelled: boolean;
  errors: { path: string; error: string }[];
}

/** What the main area is showing. */
export type View =
  | { kind: "library" }
  | { kind: "favorites" }
  | { kind: "album"; id: number }
  | { kind: "tag"; id: number }
  | { kind: "imports" }
  | { kind: "import"; id: number }
  | { kind: "marks" }
  | { kind: "mark"; mark: Mark }
  | { kind: "duplicates" }
  | { kind: "trash" };

/** "x" marks a photo for deletion; "1".."4" are custom actions. */
export type Mark = "x" | "1" | "2" | "3" | "4";
export const MARKS: Mark[] = ["x", "1", "2", "3", "4"];

export type MarkActionKind = "trash" | "copy" | "album" | "tag";

export interface MarkSlot {
  mark: Mark;
  label: string;
  /** null for a custom mark that hasn't been set up. */
  kind: MarkActionKind | null;
  /** Folder (copy), album id (album) or tag name (tag). */
  target: string;
  count: number;
}

export interface RunResult {
  done: number;
  skipped: number;
  errors: string[];
}
