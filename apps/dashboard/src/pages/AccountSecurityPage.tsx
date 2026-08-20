import { useState } from "react"
import { ShieldCheck, ShieldOff } from "lucide-react"
import { QRCodeSVG } from "qrcode.react"
import { toast } from "sonner"

import { CopyButton } from "@/components/shared/CopyButton"
import { QueryError } from "@/components/shared/QueryError"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import {
  useDisableTotp,
  useEnableTotp,
  useMe,
  useTotpSetup,
} from "@/hooks/queries"
import type { TotpSetup } from "@/lib/types"

export function AccountSecurityPage() {
  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Two-factor authentication</h1>
        <p className="text-sm text-muted-foreground">
          Protect your account with a one-time code from an authenticator app.
        </p>
      </div>
      <TwoFactorCard />
    </div>
  )
}

const CODE_PATTERN = /^\d{6}$/

function TwoFactorCard() {
  const me = useMe()
  const enabled = me.data?.totp_enabled ?? false

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between gap-4">
          <div className="space-y-1.5">
            <CardTitle>Authenticator app</CardTitle>
            <CardDescription>
              When enabled, signing in requires your password and a 6-digit code
              from an app such as Google Authenticator, Authy or 1Password.
            </CardDescription>
          </div>
          {!me.isLoading && !me.isError && (
            <Badge variant={enabled ? "default" : "outline"} className="shrink-0 gap-1">
              {enabled ? (
                <>
                  <ShieldCheck className="size-3.5" /> Enabled
                </>
              ) : (
                <>
                  <ShieldOff className="size-3.5" /> Disabled
                </>
              )}
            </Badge>
          )}
        </div>
      </CardHeader>
      <CardContent>
        {me.isError ? (
          <QueryError error={me.error} />
        ) : me.isLoading ? (
          <Skeleton className="h-9 w-40" />
        ) : enabled ? (
          <DisableSection />
        ) : (
          <EnableSection />
        )}
      </CardContent>
    </Card>
  )
}

function EnableSection() {
  const totpSetup = useTotpSetup()
  const enableTotp = useEnableTotp()
  const [open, setOpen] = useState(false)
  const [setup, setSetup] = useState<TotpSetup | null>(null)
  const [code, setCode] = useState("")

  async function openDialog() {
    setCode("")
    setSetup(null)
    setOpen(true)
    try {
      // Each open generates a fresh pending secret (10-minute lifetime).
      setSetup(await totpSetup.mutateAsync())
    } catch (error) {
      setOpen(false)
      toast.error(error instanceof Error ? error.message : "Failed to start 2FA setup")
    }
  }

  async function onEnable(event: React.FormEvent) {
    event.preventDefault()
    if (!CODE_PATTERN.test(code.trim())) {
      toast.error("Enter the 6-digit code from your app")
      return
    }
    try {
      await enableTotp.mutateAsync(code.trim())
      toast.success("Two-factor authentication enabled")
      setOpen(false)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Verification failed")
    }
  }

  return (
    <>
      <Button onClick={openDialog}>Enable 2FA</Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <form onSubmit={onEnable} className="space-y-4">
            <DialogHeader>
              <DialogTitle>Set up two-factor authentication</DialogTitle>
              <DialogDescription>
                Scan the QR code with your authenticator app, then enter the
                6-digit code it shows to confirm.
              </DialogDescription>
            </DialogHeader>
            {setup ? (
              <div className="space-y-4">
                <div className="flex justify-center">
                  {/* White backing so the QR scans in dark mode too. */}
                  <div className="rounded-lg bg-white p-3">
                    <QRCodeSVG value={setup.otpauth_url} size={192} />
                  </div>
                </div>
                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">
                    Can't scan? Enter this key manually:
                  </p>
                  <div className="flex items-center justify-between gap-2 rounded-md border bg-muted/40 px-3 py-2">
                    <code className="break-all font-mono text-xs">{setup.secret}</code>
                    <CopyButton value={setup.secret} />
                  </div>
                </div>
                <div className="space-y-2">
                  <Label htmlFor="enable-code">Authentication code</Label>
                  <Input
                    id="enable-code"
                    inputMode="numeric"
                    autoComplete="one-time-code"
                    maxLength={6}
                    placeholder="123456"
                    autoFocus
                    value={code}
                    onChange={(event) => setCode(event.target.value)}
                    className="text-center font-mono text-lg tracking-[0.5em]"
                  />
                </div>
              </div>
            ) : (
              <div className="flex justify-center py-10">
                <Skeleton className="size-48" />
              </div>
            )}
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={!setup || enableTotp.isPending}>
                {enableTotp.isPending ? "Verifying…" : "Verify & enable"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </>
  )
}

function DisableSection() {
  const disableTotp = useDisableTotp()
  const [open, setOpen] = useState(false)
  const [code, setCode] = useState("")

  async function onDisable(event: React.FormEvent) {
    event.preventDefault()
    if (!CODE_PATTERN.test(code.trim())) {
      toast.error("Enter the 6-digit code from your app")
      return
    }
    try {
      await disableTotp.mutateAsync(code.trim())
      toast.success("Two-factor authentication disabled")
      setOpen(false)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Verification failed")
    }
  }

  return (
    <div className="space-y-3">
      <p className="text-sm text-muted-foreground">
        Your account is protected by an authenticator app. If you lose access to
        it, an administrator can reset your 2FA from the Staff page.
      </p>
      <Button
        variant="outline"
        className="text-destructive hover:text-destructive"
        onClick={() => {
          setCode("")
          setOpen(true)
        }}
      >
        Disable 2FA
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-md">
          <form onSubmit={onDisable} className="space-y-4">
            <DialogHeader>
              <DialogTitle>Disable two-factor authentication</DialogTitle>
              <DialogDescription>
                Your account will no longer require a code at sign-in. Enter the
                current 6-digit code from your authenticator app to confirm.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-2">
              <Label htmlFor="disable-code">Authentication code</Label>
              <Input
                id="disable-code"
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={6}
                placeholder="123456"
                autoFocus
                value={code}
                onChange={(event) => setCode(event.target.value)}
                className="text-center font-mono text-lg tracking-[0.5em]"
              />
            </div>
            <DialogFooter>
              <Button type="button" variant="outline" onClick={() => setOpen(false)}>
                Cancel
              </Button>
              <Button type="submit" variant="destructive" disabled={disableTotp.isPending}>
                {disableTotp.isPending ? "Verifying…" : "Disable 2FA"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
