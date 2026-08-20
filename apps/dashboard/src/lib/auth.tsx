/* eslint-disable react-refresh/only-export-components */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { Navigate, Outlet, useLocation } from "react-router-dom"

import {
  api,
  getRefreshToken,
  refreshSession,
  setAccessToken,
  setOnSessionExpired,
  setRefreshToken,
} from "@/lib/api"
import type { AuthResponse, LoginResult, Staff } from "@/lib/types"

interface SetupPayload {
  email: string
  password: string
  first_name: string
  last_name: string
}

interface AuthContextValue {
  staff: Staff | null
  loading: boolean
  needsSetup: boolean
  /** Resolves to an MFA challenge when 2FA is required, null when signed in. */
  login: (email: string, password: string) => Promise<{ mfaToken: string } | null>
  loginTotp: (mfaToken: string, code: string) => Promise<void>
  completeSetup: (payload: SetupPayload) => Promise<void>
  logout: () => Promise<void>
}

const AuthContext = createContext<AuthContextValue | null>(null)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [staff, setStaff] = useState<Staff | null>(null)
  const [needsSetup, setNeedsSetup] = useState(false)
  const [loading, setLoading] = useState(true)

  const applyAuth = useCallback((auth: AuthResponse) => {
    setAccessToken(auth.access_token)
    setRefreshToken(auth.refresh_token)
    setStaff(auth.staff)
    setNeedsSetup(false)
  }, [])

  const clearSession = useCallback(() => {
    setAccessToken(null)
    setRefreshToken(null)
    setStaff(null)
  }, [])

  useEffect(() => {
    setOnSessionExpired(clearSession)
    return () => setOnSessionExpired(null)
  }, [clearSession])

  useEffect(() => {
    let cancelled = false
    async function bootstrap() {
      try {
        const setup = await api<{ needs_setup: boolean }>("/setup", { anonymous: true })
        if (cancelled) return
        setNeedsSetup(setup.needs_setup)
        if (!setup.needs_setup && getRefreshToken()) {
          const refreshed = await refreshSession()
          if (refreshed && !cancelled) {
            const me = await api<Staff>("/auth/me")
            if (!cancelled) setStaff(me)
          }
        }
      } catch {
        // Backend unreachable — land on the login page, errors surface there.
      } finally {
        if (!cancelled) setLoading(false)
      }
    }
    bootstrap()
    return () => {
      cancelled = true
    }
  }, [])

  const login = useCallback(
    async (email: string, password: string) => {
      const result = await api<LoginResult>("/auth/login", {
        body: { email, password },
        anonymous: true,
      })
      if ("mfa_required" in result) {
        return { mfaToken: result.mfa_token }
      }
      applyAuth(result)
      return null
    },
    [applyAuth],
  )

  const loginTotp = useCallback(
    async (mfaToken: string, code: string) => {
      const auth = await api<AuthResponse>("/auth/login/totp", {
        body: { mfa_token: mfaToken, code },
        anonymous: true,
      })
      applyAuth(auth)
    },
    [applyAuth],
  )

  const completeSetup = useCallback(
    async (payload: SetupPayload) => {
      const auth = await api<AuthResponse>("/setup", { body: payload, anonymous: true })
      applyAuth(auth)
    },
    [applyAuth],
  )

  const logout = useCallback(async () => {
    const refreshToken = getRefreshToken()
    if (refreshToken) {
      try {
        await api("/auth/logout", { body: { refresh_token: refreshToken } })
      } catch {
        // Best effort — the local session is cleared regardless.
      }
    }
    clearSession()
  }, [clearSession])

  const value = useMemo(
    () => ({ staff, loading, needsSetup, login, loginTotp, completeSetup, logout }),
    [staff, loading, needsSetup, login, loginTotp, completeSetup, logout],
  )

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>
}

export function useAuth(): AuthContextValue {
  const context = useContext(AuthContext)
  if (!context) throw new Error("useAuth must be used within AuthProvider")
  return context
}

export function RequireAuth() {
  const { staff, loading, needsSetup } = useAuth()
  const location = useLocation()

  if (loading) return <FullPageSpinner />
  if (needsSetup) return <Navigate to="/setup" replace />
  if (!staff) return <Navigate to="/login" replace state={{ from: location }} />
  return <Outlet />
}

export function RequireAdmin() {
  const { staff } = useAuth()
  if (staff?.role !== "admin") {
    return (
      <div className="flex h-full min-h-64 flex-col items-center justify-center gap-2 p-8 text-center">
        <p className="text-lg font-semibold">Access denied</p>
        <p className="text-sm text-muted-foreground">
          This section requires the admin role.
        </p>
      </div>
    )
  }
  return <Outlet />
}

function FullPageSpinner() {
  return (
    <div className="flex min-h-svh items-center justify-center">
      <div className="size-6 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent" />
    </div>
  )
}
