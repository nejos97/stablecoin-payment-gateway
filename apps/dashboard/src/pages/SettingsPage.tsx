import { useEffect, useState } from "react"
import { KeyRound, Plus, Webhook } from "lucide-react"
import { toast } from "sonner"

import { QueryError } from "@/components/shared/QueryError"
import { StatusBadge } from "@/components/shared/StatusBadge"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
  DialogTrigger,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  useCreateWebhookEndpoint,
  useSettings,
  useToggleWebhookEndpoint,
  useUpdateSettings,
  useWebhookEndpoints,
} from "@/hooks/queries"
import { formatDate } from "@/lib/format"
import type { WebhookEndpoint } from "@/lib/types"

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
          System configuration — payment expiry and outgoing webhooks.
        </p>
      </div>
      <PaymentExpiryCard />
      <ApiKeyPrefixCard />
      <WebhookEndpointsCard />
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

function WebhookEndpointsCard() {
  const endpoints = useWebhookEndpoints()
  const toggleEndpoint = useToggleWebhookEndpoint()
  const [deactivating, setDeactivating] = useState<WebhookEndpoint | null>(null)

  async function setActive(endpoint: WebhookEndpoint, isActive: boolean) {
    try {
      await toggleEndpoint.mutateAsync({ id: endpoint.id, is_active: isActive })
      toast.success(isActive ? "Endpoint activated" : "Endpoint deactivated")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    } finally {
      setDeactivating(null)
    }
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-4 space-y-0">
        <div className="space-y-1.5">
          <CardTitle>Webhook endpoints</CardTitle>
          <CardDescription>
            Confirmed deposits are notified to every active endpoint. No active
            endpoint means no webhooks are sent.
          </CardDescription>
        </div>
        <AddEndpointDialog />
      </CardHeader>
      <CardContent>
        {endpoints.isError ? (
          <QueryError error={endpoints.error} />
        ) : endpoints.isLoading ? (
          <Skeleton className="h-40 w-full" />
        ) : (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>URL</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Created by</TableHead>
                  <TableHead>Created</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className="text-right">Active</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {endpoints.data?.data.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={6} className="h-24 text-center text-muted-foreground">
                      No webhook endpoints yet — outgoing webhooks are disabled.
                    </TableCell>
                  </TableRow>
                )}
                {endpoints.data?.data.map((endpoint) => (
                  <TableRow key={endpoint.id}>
                    <TableCell>
                      <div className="flex items-center gap-2">
                        <Webhook className="size-3.5 shrink-0 text-muted-foreground" />
                        <code className="max-w-96 truncate text-xs">{endpoint.url}</code>
                      </div>
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={endpoint.is_active ? "active" : "inactive"} />
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {endpoint.created_by ?? "—"}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(endpoint.created_at)}
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {formatDate(endpoint.updated_at)}
                    </TableCell>
                    <TableCell className="text-right">
                      <Switch
                        checked={endpoint.is_active}
                        disabled={toggleEndpoint.isPending}
                        onCheckedChange={(checked) => {
                          if (checked) {
                            setActive(endpoint, true)
                          } else {
                            setDeactivating(endpoint)
                          }
                        }}
                        aria-label={`Toggle ${endpoint.url}`}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>

      <AlertDialog
        open={deactivating !== null}
        onOpenChange={(open) => !open && setDeactivating(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Deactivate this endpoint?</AlertDialogTitle>
            <AlertDialogDescription className="break-all">
              {deactivating?.url} will stop receiving webhooks immediately —
              queued deliveries towards it will be skipped. You can reactivate
              it at any time.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => deactivating && setActive(deactivating, false)}
            >
              Deactivate
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Card>
  )
}

function AddEndpointDialog() {
  const [open, setOpen] = useState(false)
  const [url, setUrl] = useState("")
  const createEndpoint = useCreateWebhookEndpoint()

  async function handleCreate() {
    const trimmed = url.trim()
    if (!/^https?:\/\/.+/.test(trimmed)) {
      toast.error("URL must start with http:// or https://")
      return
    }
    try {
      await createEndpoint.mutateAsync(trimmed)
      toast.success("Webhook endpoint added")
      setOpen(false)
      setUrl("")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Creation failed")
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm">
          <Plus className="size-4" /> Add endpoint
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add webhook endpoint</DialogTitle>
          <DialogDescription>
            This URL will receive a POST for every confirmed deposit, with
            automatic retries on failure.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="endpoint-url">URL</Label>
          <Input
            id="endpoint-url"
            placeholder="https://example.com/webhooks/deposits"
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") handleCreate()
            }}
          />
        </div>
        <DialogFooter>
          <Button onClick={handleCreate} disabled={createEndpoint.isPending}>
            {createEndpoint.isPending ? "Adding…" : "Add endpoint"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
