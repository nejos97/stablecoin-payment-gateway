const ADMIN_BASE = "/api/admin/v1"
const REFRESH_TOKEN_KEY = "spg.refresh_token"

// The access token only lives in memory; the refresh token survives reloads.
let accessToken: string | null = null

export function setAccessToken(token: string | null) {
  accessToken = token
}

export function getRefreshToken(): string | null {
  return localStorage.getItem(REFRESH_TOKEN_KEY)
}

export function setRefreshToken(token: string | null) {
  if (token === null) {
    localStorage.removeItem(REFRESH_TOKEN_KEY)
  } else {
    localStorage.setItem(REFRESH_TOKEN_KEY, token)
  }
}

export class ApiError extends Error {
  statusCode: number

  constructor(statusCode: number, message: string) {
    super(message)
    this.statusCode = statusCode
  }
}

// Called when a 401 survives a refresh attempt — the auth provider registers
// its logout handler here.
let onSessionExpired: (() => void) | null = null
export function setOnSessionExpired(handler: (() => void) | null) {
  onSessionExpired = handler
}

interface RequestOptions {
  method?: string
  body?: unknown
  /** Skip the Authorization header and the 401-refresh dance (login, setup). */
  anonymous?: boolean
}

export async function api<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const response = await send(path, options)

  if (response.status === 401 && !options.anonymous) {
    const refreshed = await refreshSession()
    if (refreshed) {
      const retried = await send(path, options)
      return handleResponse<T>(retried)
    }
    onSessionExpired?.()
  }

  return handleResponse<T>(response)
}

function send(path: string, options: RequestOptions): Promise<Response> {
  const headers: Record<string, string> = {}
  if (options.body !== undefined) {
    headers["Content-Type"] = "application/json"
  }
  if (!options.anonymous && accessToken) {
    headers["Authorization"] = `Bearer ${accessToken}`
  }
  return fetch(`${ADMIN_BASE}${path}`, {
    method: options.method ?? (options.body !== undefined ? "POST" : "GET"),
    headers,
    body: options.body !== undefined ? JSON.stringify(options.body) : undefined,
  })
}

async function handleResponse<T>(response: Response): Promise<T> {
  if (response.ok) {
    return (await response.json()) as T
  }
  let message = `HTTP ${response.status}`
  try {
    const body = (await response.json()) as { message?: string }
    if (body.message) message = body.message
  } catch {
    // non-JSON error body — keep the generic message
  }
  throw new ApiError(response.status, message)
}

// Single-flight refresh: concurrent 401s share one refresh call.
let refreshPromise: Promise<boolean> | null = null

export function refreshSession(): Promise<boolean> {
  refreshPromise ??= doRefresh().finally(() => {
    refreshPromise = null
  })
  return refreshPromise
}

async function doRefresh(): Promise<boolean> {
  const refreshToken = getRefreshToken()
  if (!refreshToken) return false

  try {
    const response = await fetch(`${ADMIN_BASE}/auth/refresh`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ refresh_token: refreshToken }),
    })
    if (!response.ok) {
      setAccessToken(null)
      setRefreshToken(null)
      return false
    }
    const data = (await response.json()) as {
      access_token: string
      refresh_token: string
    }
    setAccessToken(data.access_token)
    setRefreshToken(data.refresh_token)
    return true
  } catch {
    return false
  }
}
