import { Navigate, useLocation } from "react-router-dom"

import { LoginForm } from "@/components/login-form"
import { useAuth } from "@/lib/auth"

export function LoginPage() {
  const { staff, needsSetup, loading } = useAuth()
  const location = useLocation()

  if (!loading && needsSetup) return <Navigate to="/setup" replace />
  if (staff) {
    const from = (location.state as { from?: { pathname: string } })?.from?.pathname ?? "/"
    return <Navigate to={from} replace />
  }

  return (
    <div className="flex min-h-svh flex-col items-center justify-center bg-muted/40 p-6 md:p-10">
      <div className="w-full max-w-sm md:max-w-3xl">
        <LoginForm />
      </div>
    </div>
  )
}
