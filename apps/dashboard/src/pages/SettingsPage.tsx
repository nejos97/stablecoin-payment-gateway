import { useEffect, useState } from "react"
import { ShieldCheck } from "lucide-react"
import { toast } from "sonner"

import { QueryError } from "@/components/shared/QueryError"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { useSettings, useUpdateSettings } from "@/hooks/queries"

const EXPIRY_OPTIONS = [
  { value: 15, label: "15 minutes" },
  { value: 30, label: "30 minutes" },
  { value: 60, label: "1 hour" },
  { value: 120, label: "2 hours" },
  { value: 360, label: "6 hours" },
  { value: 720, label: "12 hours" },
  { value: 1440, label: "24 hours" },
]

export function SettingsPage() {
  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">Settings</h1>
        <p className="text-sm text-muted-foreground">
          System configuration — payment expiry, two-factor authentication.
        </p>
      </div>
      <PaymentExpiryCard />
      <TotpIssuerCard />
    </div>
  )
}

const DEFAULT_TOTP_ISSUER = "Stablecoin Payment Gateway"

function TotpIssuerCard() {
  const settings = useSettings()
  const updateSettings = useUpdateSettings()
  const [issuer, setIssuer] = useState("")

  useEffect(() => {
    if (settings.data) setIssuer(settings.data.totp_issuer)
  }, [settings.data])

  const saved = settings.data?.totp_issuer ?? ""
  const dirty = issuer.trim() !== saved

  async function save() {
    // An empty (or default-equal) value resets the stored setting so the
    // backend falls back to its default.
    const next = issuer.trim() === DEFAULT_TOTP_ISSUER ? "" : issuer.trim()
    try {
      const updated = await updateSettings.mutateAsync({ totp_issuer: next })
      toast.success(`Authenticator apps will now show “${updated.totp_issuer}”`)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Two-factor issuer</CardTitle>
        <CardDescription>
          Name shown by authenticator apps next to staff 2FA codes. Applies to
          new enrollments only — existing entries keep their label. Leave empty
          to use the default “{DEFAULT_TOTP_ISSUER}”.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {settings.isError ? (
          <QueryError error={settings.error} />
        ) : settings.isLoading ? (
          <Skeleton className="h-9 w-64" />
        ) : (
          <div className="space-y-3">
            <div className="flex items-center gap-2">
              <Input
                className="w-72"
                maxLength={40}
                placeholder={DEFAULT_TOTP_ISSUER}
                value={issuer}
                onChange={(event) => setIssuer(event.target.value)}
                aria-label="Two-factor issuer"
              />
              <Button onClick={save} disabled={!dirty || updateSettings.isPending} size="sm">
                {updateSettings.isPending ? "Saving…" : "Save"}
              </Button>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <ShieldCheck className="size-3.5 shrink-0" />
              <span>
                Shown as <code className="text-xs">{issuer.trim() || DEFAULT_TOTP_ISSUER}: user@company.com</code>
              </span>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}

function PaymentExpiryCard() {
  const settings = useSettings()
  const updateSettings = useUpdateSettings()

  async function onChange(value: string) {
    const minutes = Number(value)
    try {
      await updateSettings.mutateAsync({ deposit_expiry_minutes: minutes })
      const label = EXPIRY_OPTIONS.find((o) => o.value === minutes)?.label ?? `${minutes} min`
      toast.success(`Default payment expiry set to ${label}`)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    }
  }

  const current = settings.data?.deposit_expiry_minutes
  const isPreset = EXPIRY_OPTIONS.some((o) => o.value === current)

  return (
    <Card>
      <CardHeader>
        <CardTitle>Payment expiry</CardTitle>
        <CardDescription>
          How long a newly created deposit address stays valid when no explicit
          expiry is provided. Maximum 24 hours.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {settings.isError ? (
          <QueryError error={settings.error} />
        ) : settings.isLoading ? (
          <Skeleton className="h-9 w-48" />
        ) : (
          <Select
            value={current !== undefined ? String(current) : undefined}
            onValueChange={onChange}
            disabled={updateSettings.isPending}
          >
            <SelectTrigger className="w-48">
              <SelectValue placeholder="Select duration" />
            </SelectTrigger>
            <SelectContent>
              {EXPIRY_OPTIONS.map((option) => (
                <SelectItem key={option.value} value={String(option.value)}>
                  {option.label}
                </SelectItem>
              ))}
              {!isPreset && current !== undefined && (
                <SelectItem value={String(current)}>{current} minutes</SelectItem>
              )}
            </SelectContent>
          </Select>
        )}
      </CardContent>
    </Card>
  )
}

