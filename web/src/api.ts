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

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/v1${path}`, {
    method,
    credentials: "same-origin",
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) {
    let code = "http_error";
    let message = `${response.status} ${response.statusText}`;
    try {
      const data = (await response.json()) as { error?: { code?: string; message?: string } };
      code = data.error?.code ?? code;
      message = data.error?.message ?? message;
    } catch {
      // Not a JSON error body; keep the generic message.
    }
    throw new ApiError(response.status, code, message);
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

export const api = {
  info: () => request<Info>("GET", "/info"),
  session: () => request<SessionInfo>("GET", "/session"),
  login: (password: string, rememberMe: boolean) =>
    request<SessionInfo>("POST", "/session", { password, remember_me: rememberMe }),
  logout: () => request<void>("DELETE", "/session"),
};

export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401;
}
