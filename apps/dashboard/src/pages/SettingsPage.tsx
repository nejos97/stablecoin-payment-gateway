import { useEffect, useState } from "react"
import { KeyRound } from "lucide-react"
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
          System configuration — payment expiry and API key prefix.
        </p>
      </div>
      <PaymentExpiryCard />
      <ApiKeyPrefixCard />
    </div>
  )
}

function ApiKeyPrefixCard() {
  const settings = useSettings()
  const updateSettings = useUpdateSettings()
  const [prefix, setPrefix] = useState("")

  useEffect(() => {
    if (settings.data) setPrefix(settings.data.api_key_prefix ?? "")
  }, [settings.data])

  const saved = settings.data?.api_key_prefix ?? ""
  const dirty = prefix !== saved
  const preview = `${prefix ? `${prefix}_` : ""}xxxxxxxx_${"·".repeat(24)}`

  async function save() {
    try {
      await updateSettings.mutateAsync({ api_key_prefix: prefix })
      toast.success(
        prefix
          ? `New API keys will start with “${prefix}_”`
          : "API key prefix removed — new keys have no tag",
      )
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>API key prefix</CardTitle>
        <CardDescription>
          Optional tag prepended to newly generated API keys (max 5 alphanumeric
          characters). Existing keys keep working unchanged.
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
                className="w-32"
                maxLength={5}
                placeholder="none"
                value={prefix}
                onChange={(event) =>
                  setPrefix(event.target.value.replace(/[^a-zA-Z0-9]/g, ""))
                }
                aria-label="API key prefix"
              />
              <Button onClick={save} disabled={!dirty || updateSettings.isPending} size="sm">
                {updateSettings.isPending ? "Saving…" : "Save"}
              </Button>
            </div>
            <div className="flex items-center gap-2 text-sm text-muted-foreground">
              <KeyRound className="size-3.5 shrink-0" />
              <span>
                New keys will look like <code className="text-xs">{preview}</code>
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

