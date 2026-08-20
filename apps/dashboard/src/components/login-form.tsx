import { useState } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import { Coins, ShieldCheck } from "lucide-react"
import { useForm } from "react-hook-form"
import { useNavigate } from "react-router-dom"
import { toast } from "sonner"
import { z } from "zod"

import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import {
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { useAuth } from "@/lib/auth"
import { cn } from "@/lib/utils"

const schema = z.object({
  email: z.string().trim().email("Invalid email address"),
  password: z.string().min(1, "Password is required"),
})

const totpSchema = z.object({
  code: z.string().trim().regex(/^\d{6}$/, "Enter the 6-digit code"),
})

type FormValues = z.infer<typeof schema>
type TotpValues = z.infer<typeof totpSchema>

export function LoginForm({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const { login, loginTotp } = useAuth()
  const navigate = useNavigate()
  const form = useForm<FormValues>({ resolver: zodResolver(schema) })
  const totpForm = useForm<TotpValues>({ resolver: zodResolver(totpSchema) })
  const [mfaToken, setMfaToken] = useState<string | null>(null)

  async function onSubmit(values: FormValues) {
    try {
      const challenge = await login(values.email, values.password)
      if (challenge) {
        totpForm.reset()
        setMfaToken(challenge.mfaToken)
      } else {
        navigate("/", { replace: true })
      }
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Login failed")
    }
  }

  async function onSubmitTotp(values: TotpValues) {
    if (!mfaToken) return
    try {
      await loginTotp(mfaToken, values.code)
      navigate("/", { replace: true })
    } catch (error) {
      // 401 here covers expired challenges and the 5-wrong-codes limit —
      // "Back to sign in" restarts from the password step.
      toast.error(error instanceof Error ? error.message : "Verification failed")
    }
  }

  const { errors, isSubmitting } = form.formState
  const totpState = totpForm.formState

  return (
    <div className={cn("flex flex-col gap-6", className)} {...props}>
      <Card className="overflow-hidden p-0">
        <CardContent className="grid p-0 md:grid-cols-2">
          {mfaToken ? (
            <form className="p-6 md:p-8" onSubmit={totpForm.handleSubmit(onSubmitTotp)}>
              <FieldGroup>
                <div className="flex flex-col items-center gap-2 text-center">
                  <div className="flex size-10 items-center justify-center rounded-full bg-primary/10">
                    <ShieldCheck className="size-5 text-primary" />
                  </div>
                  <h1 className="text-2xl font-bold">Two-factor authentication</h1>
                  <p className="text-balance text-muted-foreground">
                    Enter the 6-digit code from your authenticator app
                  </p>
                </div>
                <Field>
                  <FieldLabel htmlFor="totp-code">Authentication code</FieldLabel>
                  <Input
                    id="totp-code"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    maxLength={6}
                    placeholder="123456"
                    autoFocus
                    className="text-center font-mono text-lg tracking-[0.5em]"
                    {...totpForm.register("code")}
                  />
                  {totpState.errors.code && (
                    <p className="text-xs text-destructive">
                      {totpState.errors.code.message}
                    </p>
                  )}
                </Field>
                <Field>
                  <Button type="submit" disabled={totpState.isSubmitting}>
                    {totpState.isSubmitting ? "Verifying…" : "Verify"}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    onClick={() => setMfaToken(null)}
                  >
                    Back to sign in
                  </Button>
                </Field>
              </FieldGroup>
            </form>
          ) : (
            <form className="p-6 md:p-8" onSubmit={form.handleSubmit(onSubmit)}>
              <FieldGroup>
                <div className="flex flex-col items-center gap-2 text-center">
                  <h1 className="text-2xl font-bold">Welcome back</h1>
                  <p className="text-balance text-muted-foreground">
                    Sign in to the payment gateway dashboard
                  </p>
                </div>
                <Field>
                  <FieldLabel htmlFor="email">Email</FieldLabel>
                  <Input
                    id="email"
                    type="email"
                    placeholder="you@company.com"
                    autoComplete="email"
                    {...form.register("email")}
                  />
                  {errors.email && (
                    <p className="text-xs text-destructive">{errors.email.message}</p>
                  )}
                </Field>
                <Field>
                  <FieldLabel htmlFor="password">Password</FieldLabel>
                  <Input
                    id="password"
                    type="password"
                    autoComplete="current-password"
                    {...form.register("password")}
                  />
                  {errors.password && (
                    <p className="text-xs text-destructive">{errors.password.message}</p>
                  )}
                </Field>
                <Field>
                  <Button type="submit" disabled={isSubmitting}>
                    {isSubmitting ? "Signing in…" : "Sign in"}
                  </Button>
                </Field>
                <FieldDescription className="text-center">
                  Accounts are created by an administrator.
                </FieldDescription>
              </FieldGroup>
            </form>
          )}
          <div className="relative hidden flex-col items-center justify-center gap-4 bg-linear-to-br from-primary via-primary/90 to-primary/70 p-10 text-primary-foreground md:flex">
            <div className="flex size-14 items-center justify-center rounded-2xl bg-primary-foreground/10 backdrop-blur">
              <Coins className="size-7" />
            </div>
            <div className="text-center leading-tight">
              <p className="text-lg font-semibold">Stablecoin Payment Gateway</p>
              <p className="mt-1 text-sm text-primary-foreground/70">
                USDT deposits on Tron, Ethereum &amp; Solana
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
      <FieldDescription className="px-6 text-center">
        Access restricted to authorized staff.
      </FieldDescription>
    </div>
  )
}
