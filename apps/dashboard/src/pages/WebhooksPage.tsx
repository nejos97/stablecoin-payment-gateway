import { useState } from "react"
import { KeyRound, Pencil, Plus, Wand2, Webhook } from "lucide-react"
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
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
  useToggleWebhookEndpoint,
  useUpdateWebhookEndpoint,
  useWebhookEndpoints,
} from "@/hooks/queries"
import { formatDate } from "@/lib/format"
import type { WebhookEndpoint, WebhookEventType } from "@/lib/types"

const EVENT_OPTIONS: Array<{
  value: WebhookEventType
  label: string
  description: string
}> = [
  { value: "pending", label: "Pending", description: "A payment address is created" },
  { value: "paid", label: "Paid", description: "A deposit is confirmed on-chain" },
  { value: "expired", label: "Expired", description: "An address expires unpaid" },
]

/** 32 random alphanumeric chars for the signing secret. */
function generateSecret(): string {
  const chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
  const bytes = new Uint8Array(32)
  crypto.getRandomValues(bytes)
  return Array.from(bytes, (byte) => chars[byte % chars.length]).join("")
}

function validSecret(secret: string): boolean {
  return secret.length >= 16 && secret.length <= 128 && /^[\x21-\x7e]+$/.test(secret)
}

export function WebhooksPage() {
  const endpoints = useWebhookEndpoints()
  const toggleEndpoint = useToggleWebhookEndpoint()
  const [deactivating, setDeactivating] = useState<WebhookEndpoint | null>(null)
  const [editing, setEditing] = useState<WebhookEndpoint | null>(null)

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
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Webhooks</h1>
          <p className="text-sm text-muted-foreground">
            Each active endpoint receives a POST for the payment events it
            subscribes to — pending (address created), paid (deposit
            confirmed) and expired.
          </p>
        </div>
        <AddEndpointDialog />
      </div>

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
                <TableHead>Events</TableHead>
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
                  <TableCell colSpan={7} className="h-24 text-center text-muted-foreground">
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
                      {endpoint.has_secret && (
                        <KeyRound
                          className="size-3.5 shrink-0 text-muted-foreground"
                          aria-label="Signed deliveries"
                        >
                          <title>Deliveries are signed (X-Webhook-Signature)</title>
                        </KeyRound>
                      )}
                    </div>
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1.5">
                      <div className="flex flex-wrap gap-1">
                        {endpoint.events.map((event) => (
                          <Badge key={event} variant="secondary" className="capitalize">
                            {event}
                          </Badge>
                        ))}
                      </div>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="size-6"
                        onClick={() => setEditing(endpoint)}
                        aria-label={`Edit ${endpoint.url}`}
                      >
                        <Pencil className="size-3" />
                      </Button>
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

      <EditEndpointDialog endpoint={editing} onClose={() => setEditing(null)} />

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
    </div>
  )
}

function EventChecklist({
  selected,
  onToggle,
}: {
  selected: WebhookEventType[]
  onToggle: (event: WebhookEventType, checked: boolean) => void
}) {
  return (
    <div className="space-y-3">
      {EVENT_OPTIONS.map((option) => (
        <div key={option.value} className="flex items-start gap-3">
          <Checkbox
            id={`event-${option.value}`}
            checked={selected.includes(option.value)}
            onCheckedChange={(checked) => onToggle(option.value, checked === true)}
          />
          <div className="grid gap-0.5 leading-none">
            <Label htmlFor={`event-${option.value}`}>{option.label}</Label>
            <p className="text-xs text-muted-foreground">{option.description}</p>
          </div>
        </div>
      ))}
    </div>
  )
}

function toggleEvent(
  events: WebhookEventType[],
  event: WebhookEventType,
  checked: boolean,
): WebhookEventType[] {
  const without = events.filter((e) => e !== event)
  return checked ? [...without, event] : without
}

function AddEndpointDialog() {
  const [open, setOpen] = useState(false)
  const [url, setUrl] = useState("")
  const [events, setEvents] = useState<WebhookEventType[]>(["paid"])
  const [secret, setSecret] = useState("")
  const createEndpoint = useCreateWebhookEndpoint()

  async function handleCreate() {
    const trimmed = url.trim()
    const trimmedSecret = secret.trim()
    if (!/^https?:\/\/.+/.test(trimmed)) {
      toast.error("URL must start with http:// or https://")
      return
    }
    if (events.length === 0) {
      toast.error("Select at least one event")
      return
    }
    if (trimmedSecret && !validSecret(trimmedSecret)) {
      toast.error("Secret must be 16-128 printable characters without spaces")
      return
    }
    try {
      await createEndpoint.mutateAsync({
        url: trimmed,
        events,
        ...(trimmedSecret ? { secret: trimmedSecret } : {}),
      })
      toast.success("Webhook endpoint added")
      setOpen(false)
      setUrl("")
      setEvents(["paid"])
      setSecret("")
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
            This URL will receive a POST for each selected event, with
            automatic retries on failure.
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
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
          <div className="space-y-2">
            <Label>Events</Label>
            <EventChecklist
              selected={events}
              onToggle={(event, checked) =>
                setEvents((current) => toggleEvent(current, event, checked))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="endpoint-secret">Signing secret (optional)</Label>
            <div className="flex items-center gap-2">
              <Input
                id="endpoint-secret"
                className="font-mono text-xs"
                placeholder="No signature"
                value={secret}
                onChange={(event) => setSecret(event.target.value)}
                autoComplete="off"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setSecret(generateSecret())}
              >
                <Wand2 className="size-3.5" /> Generate
              </Button>
            </div>
            <p className="text-xs text-muted-foreground">
              If set, each delivery carries an X-Webhook-Signature header
              (HMAC-SHA256 of the body). Store it now — it cannot be read back
              later.
            </p>
          </div>
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

function EditEndpointDialog({
  endpoint,
  onClose,
}: {
  endpoint: WebhookEndpoint | null
  onClose: () => void
}) {
  const [events, setEvents] = useState<WebhookEventType[]>([])
  const [secret, setSecret] = useState("")
  const [removeSecret, setRemoveSecret] = useState(false)
  const [editingId, setEditingId] = useState<string | null>(null)
  const updateEndpoint = useUpdateWebhookEndpoint()

  // Reset the local state when a different endpoint is opened.
  if (endpoint && endpoint.id !== editingId) {
    setEditingId(endpoint.id)
    setEvents(endpoint.events)
    setSecret("")
    setRemoveSecret(false)
  }

  async function handleSave() {
    if (!endpoint) return
    if (events.length === 0) {
      toast.error("Select at least one event")
      return
    }
    const trimmedSecret = secret.trim()
    if (!removeSecret && trimmedSecret && !validSecret(trimmedSecret)) {
      toast.error("Secret must be 16-128 printable characters without spaces")
      return
    }
    // Only touch the secret when the user acted: empty string removes it,
    // a new value rotates it, otherwise it stays as-is server-side.
    const secretPatch = removeSecret
      ? { secret: "" }
      : trimmedSecret
        ? { secret: trimmedSecret }
        : {}
    try {
      await updateEndpoint.mutateAsync({ id: endpoint.id, events, ...secretPatch })
      toast.success("Endpoint updated")
      onClose()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    }
  }

  return (
    <Dialog
      open={endpoint !== null}
      onOpenChange={(open) => {
        if (!open) {
          setEditingId(null)
          onClose()
        }
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Edit endpoint</DialogTitle>
          <DialogDescription className="break-all">
            {endpoint?.url}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-2">
            <Label>Events</Label>
            <EventChecklist
              selected={events}
              onToggle={(event, checked) =>
                setEvents((current) => toggleEvent(current, event, checked))
              }
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="edit-endpoint-secret">Signing secret</Label>
            <div className="flex items-center gap-2">
              <Input
                id="edit-endpoint-secret"
                className="font-mono text-xs"
                placeholder={
                  endpoint?.has_secret ? "Unchanged — type to rotate" : "No signature"
                }
                value={secret}
                disabled={removeSecret}
                onChange={(event) => setSecret(event.target.value)}
                autoComplete="off"
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={removeSecret}
                onClick={() => setSecret(generateSecret())}
              >
                <Wand2 className="size-3.5" /> Generate
              </Button>
            </div>
            {endpoint?.has_secret && (
              <div className="flex items-center gap-2">
                <Checkbox
                  id="remove-secret"
                  checked={removeSecret}
                  onCheckedChange={(checked) => setRemoveSecret(checked === true)}
                />
                <Label htmlFor="remove-secret" className="text-xs font-normal">
                  Remove the secret (deliveries become unsigned)
                </Label>
              </div>
            )}
            <p className="text-xs text-muted-foreground">
              The current secret cannot be read back — set a new one to rotate
              it.
            </p>
          </div>
        </div>
        <DialogFooter>
          <Button onClick={handleSave} disabled={updateEndpoint.isPending}>
            {updateEndpoint.isPending ? "Saving…" : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
