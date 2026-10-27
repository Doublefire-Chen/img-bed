// Thin client for the img-bed JSON API under /api/v1/.

export const APP_VERSION = __APP_VERSION__;

/** An error response: `{"error": {"code", "message"}}` plus the HTTP status. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

async function errorFrom(response: Response): Promise<ApiError> {
  let code = "http_error";
  let message = `${response.status} ${response.statusText}`;
  if (response.status === 413) {
    // A proxy in front of img-bed (e.g. nginx) may refuse before img-bed sees it.
    code = "too_large";
    message = "file is larger than the upload limit";
  }
  try {
    const data = (await response.json()) as { error?: { code?: string; message?: string } };
    code = data.error?.code ?? code;
    message = data.error?.message ?? message;
  } catch {
    // Not a JSON error body; keep the generic message.
  }
  return new ApiError(response.status, code, message);
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    method,
    credentials: "same-origin",
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    throw await errorFrom(response);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  return (await response.json()) as T;
}

export interface Info {
  version: string;
}

export interface SessionInfo {
  /** Unix seconds; fixed at login, never extended. */
  expires_at: number;
}

export interface PasswordChanged {
  /** How many other sessions were signed out. */
  ended_sessions: number;
}

/** Minimum password length in characters, as enforced by the server. */
export const MIN_PASSWORD_LEN = 8;

export interface ImagePath {
  path: string;
  /** Full public URL: the server's public_url + "/" + the encoded Path. */
  url: string;
  is_primary: boolean;
}

export interface Image {
  id: number;
  sha256: string;
  size: number;
  /** Detected MIME type, e.g. "image/png". */
  type: string;
  width: number | null;
  height: number | null;
  original_filename: string;
  /** Unix seconds. */
  uploaded_at: number;
  /** Unix seconds; null unless the Image is in the Trash. */
  trashed_at: number | null;
  /** When a trashed Image will be Permanently Deleted (unix seconds); null unless in the Trash. */
  permanent_delete_at: number | null;
  /** The Primary Path and its URL. */
  path: string;
  url: string;
  paths: ImagePath[];
}

export interface Stats {
  /** Images in the Library (the Trash not included). */
  image_count: number;
  /** Bytes of every stored file, the Trash included. */
  disk_usage: number;
}

/** One page of the Library, newest first. */
export interface LibraryPage {
  images: Image[];
  page: number;
  per_page: number;
  /** Images matching the search, across all pages. */
  total: number;
  pages: number;
  /** Library totals, independent of the search. */
  stats: Stats;
}

/** Query-key prefix of everything Library-related; invalidate it after a change. */
export const LIBRARY_KEY = ["library"] as const;

function libraryPage(page: number, perPage: number, search: string): Promise<LibraryPage> {
  const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
  if (search.trim()) params.set("q", search.trim());
  return request<LibraryPage>("GET", `/images?${params}`);
}

/** A stored file: a new Image, or a Duplicate Upload of an existing one. */
export interface Uploaded {
  /** The bytes matched an existing Image; nothing new was created. */
  duplicate: boolean;
  /** For a Duplicate Upload, when the Image was first uploaded (unix seconds); otherwise null. */
  original_uploaded_at: number | null;
  /** The Duplicate Upload matched an Image in the Trash, which was Restored with all its Paths. */
  restored: boolean;
  image: Image;
}

export type UploadResult =
  | ({ filename: string } & Uploaded)
  | { filename: string; error: { code: string; message: string } };

/** Uploads one file; resolves with its outcome or throws an ApiError. */
async function uploadFile(file: File): Promise<Uploaded> {
  const form = new FormData();
  form.append("file", file, file.name);
  const response = await fetch("/api/v1/images", {
    method: "POST",
    credentials: "same-origin",
    body: form,
  });
  if (!response.ok) {
    throw await errorFrom(response);
  }
  const data = (await response.json()) as { results: UploadResult[] };
  const result = data.results[0];
  if (!result) throw new ApiError(500, "no_result", "the server returned no result");
  if ("error" in result) throw new ApiError(400, result.error.code, result.error.message);
  return {
    duplicate: result.duplicate,
    original_uploaded_at: result.original_uploaded_at,
    restored: result.restored,
    image: result.image,
  };
}

/** One page of the Trash, most recently trashed first. */
export interface TrashPage {
  images: Image[];
  page: number;
  per_page: number;
  /** Images in the Trash. */
  total: number;
  pages: number;
  /** Days an Image stays in the Trash before it is Permanently Deleted. */
  trash_days: number;
}

/**
 * Query-key prefix of the Trash. It lives under the Library key, so changes
 * that invalidate the Library refresh the Trash too.
 */
export const TRASH_KEY = [...LIBRARY_KEY, "trash"] as const;

function trashPage(page: number, perPage: number): Promise<TrashPage> {
  const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
  return request<TrashPage>("GET", `/trash?${params}`);
}

/** A ready-made Path Template. */
export interface Preset {
  id: string;
  name: string;
  template: string;
  /** An example Path it makes for an upload of "holiday photo.jpg" now. */
  example: string;
}

export interface Settings {
  /** The Path Template new uploads get their first Path from. */
  path_template: string;
  /** The Preset the template is, or null for a Custom template. */
  preset: string | null;
  example: string;
  presets: Preset[];
}

export interface TemplatePreview {
  path_template: string;
  preset: string | null;
  example: string;
}

export const SETTINGS_KEY = ["settings"] as const;

/** An API Token as listed; the secret is never included. */
export interface ApiToken {
  id: number;
  name: string;
  /** Unix seconds. */
  created_at: number;
  /** Unix seconds; null until first used. Updated at most once a minute. */
  last_used_at: number | null;
  /** Unix seconds; null while the token is active. */
  revoked_at: number | null;
}

/** A newly created API Token with its secret, which is shown only this once. */
export interface CreatedApiToken extends ApiToken {
  secret: string;
}

export const TOKENS_KEY = ["tokens"] as const;

/** The upload endpoint, the only one an API Token may call. */
export const UPLOAD_ENDPOINT = "/api/v1/images";
/** The multipart field the upload endpoint reads files from. */
export const UPLOAD_FIELD = "file";

export const api = {
  info: () => request<Info>("GET", "/info"),
  session: () => request<SessionInfo>("GET", "/session"),
  login: (password: string, rememberMe: boolean) =>
    request<SessionInfo>("POST", "/session", { password, remember_me: rememberMe }),
  logout: () => request<void>("DELETE", "/session"),
  /** Changes the Owner's password; every other session ends, this one continues. */
  changePassword: (currentPassword: string, newPassword: string) =>
    request<PasswordChanged>("PUT", "/session/password", {
      current_password: currentPassword,
      new_password: newPassword,
    }),
  uploadFile,
  libraryPage,
  image: (id: number) => request<Image>("GET", `/images/${id}`),
  addPath: (id: number, path: string) => request<Image>("POST", `/images/${id}/paths`, { path }),
  removePath: (id: number, path: string) =>
    request<Image>("DELETE", `/images/${id}/paths/${encodePath(path)}`),
  setPrimary: (id: number, path: string) => request<Image>("PUT", `/images/${id}/primary`, { path }),
  /** Moves Images to the Trash. */
  trash: (ids: number[]) => request<{ images: Image[] }>("POST", "/trash", { ids }),
  trashPage,
  restore: (ids: number[]) => request<{ images: Image[] }>("POST", "/trash/restore", { ids }),
  /** Permanently Deletes Images from the Trash. */
  deletePermanently: (ids: number[]) =>
    request<{ deleted: number }>("POST", "/trash/delete", { ids }),
  settings: () => request<Settings>("GET", "/settings"),
  savePathTemplate: (template: string) =>
    request<Settings>("PUT", "/settings", { path_template: template }),
  previewPathTemplate: (template: string) =>
    request<TemplatePreview>("POST", "/settings/preview", { path_template: template }),
  tokens: () => request<{ tokens: ApiToken[] }>("GET", "/tokens"),
  createToken: (name: string) => request<CreatedApiToken>("POST", "/tokens", { name }),
  revokeToken: (id: number) => request<ApiToken>("DELETE", `/tokens/${id}`),
};

/** The Image's file for previews, also while it is in the Trash (Owner session). */
export function imageFileSrc(id: number): string {
  return `/api/v1/images/${id}/file`;
}

/** Percent-encodes each segment of a Path, keeping the `/` between them. */
function encodePath(path: string): string {
  return path.split("/").map(encodeURIComponent).join("/");
}

export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}
