import { toast } from "sonner"

import { QueryError } from "@/components/shared/QueryError"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
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
          System configuration — payment expiry.
        </p>
      </div>
      <PaymentExpiryCard />
    </div>
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

