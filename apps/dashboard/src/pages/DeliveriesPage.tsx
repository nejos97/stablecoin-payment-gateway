import { useState } from "react"
import { Braces, RefreshCw, RotateCcw } from "lucide-react"
import { Link, useSearchParams } from "react-router-dom"
import { toast } from "sonner"

import { CopyButton } from "@/components/shared/CopyButton"
import { JsonViewer } from "@/components/shared/JsonViewer"
import { Pagination } from "@/components/shared/Pagination"
import { QueryError } from "@/components/shared/QueryError"
import { StatusBadge } from "@/components/shared/StatusBadge"
import { Badge } from "@/components/ui/badge"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import {
  useRetryAllFailedWebhooks,
  useRetryWebhook,
  useRetryWebhookDelivery,
  useWebhooks,
} from "@/hooks/queries"
import { formatAmount, formatDate, networkLabel, shortHash } from "@/lib/format"
import type { WebhookDelivery } from "@/lib/types"

const PAGE_SIZE = 20
const ALL = "all"

export function DeliveriesPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const status = searchParams.get("status") ?? ALL
  const offset = Number(searchParams.get("offset") ?? 0)

  const webhooks = useWebhooks({
    status: status === ALL ? undefined : status,
    limit: PAGE_SIZE,
    offset,
  })
  const retryOne = useRetryWebhook()
  const retryDelivery = useRetryWebhookDelivery()
  const retryAll = useRetryAllFailedWebhooks()
  const [payloadWebhook, setPayloadWebhook] = useState<WebhookDelivery | null>(null)

  function updateParams(updates: Record<string, string | undefined>) {
    const next = new URLSearchParams(searchParams)
    for (const [key, value] of Object.entries(updates)) {
      if (value === undefined || value === ALL) next.delete(key)
      else next.set(key, value)
    }
    setSearchParams(next)
  }

  async function handleRetry(webhook: WebhookDelivery) {
    try {
      // Legacy rows (no endpoint) predate per-delivery retries — replay the
      // whole deposit towards every active paid endpoint instead.
      if (webhook.webhook_endpoint_id === null && webhook.deposit_id) {
        await retryOne.mutateAsync(webhook.deposit_id)
      } else {
        await retryDelivery.mutateAsync(webhook.id)
      }
      toast.success("Webhook re-queued")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Retry failed")
    }
  }

  function canRetry(webhook: WebhookDelivery): boolean {
    // Only a delivery whose last attempt failed can be retried; a delivered
    // (successful) or still-pending delivery is not retryable.
    if (webhook.status !== "failed") return false
    // Paid deliveries replay only once the deposit is confirmed.
    if (webhook.event === "paid") return webhook.deposit_status === "confirmed"
    return webhook.webhook_endpoint_id !== null
  }

  async function handleRetryAll() {
    try {
      const result = await retryAll.mutateAsync()
      toast.success(`${result.retried} webhook${result.retried === 1 ? "" : "s"} re-queued`)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Bulk retry failed")
    }
  }

  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Deliveries</h1>
          <p className="text-sm text-muted-foreground">
            Delivery state of deposit-confirmation callbacks (latest attempt per
            deposit and endpoint).
          </p>
        </div>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button variant="outline" disabled={retryAll.isPending}>
              <RotateCcw className="size-4" /> Retry all failed
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Retry all failed webhooks?</AlertDialogTitle>
              <AlertDialogDescription>
                Every failed delivery is reset and re-queued for an immediate
                attempt, restarting its full retry cycle. Paid deliveries are
                only retried once their deposit is confirmed.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={handleRetryAll}>Retry all</AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>

      <Select value={status} onValueChange={(value) => updateParams({ status: value, offset: undefined })}>
        <SelectTrigger className="w-40">
          <SelectValue placeholder="Status" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={ALL}>All statuses</SelectItem>
          <SelectItem value="pending">Pending</SelectItem>
          <SelectItem value="delivered">Delivered</SelectItem>
          <SelectItem value="failed">Failed</SelectItem>
        </SelectContent>
      </Select>

      {webhooks.isError ? (
        <QueryError error={webhooks.error} />
      ) : webhooks.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        <>
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Payment</TableHead>
                  <TableHead>Event</TableHead>
                  <TableHead>Endpoint</TableHead>
                  <TableHead>Tx hash</TableHead>
                  <TableHead>Amount</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Attempts</TableHead>
                  <TableHead>Last result</TableHead>
                  <TableHead>Updated</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {webhooks.data?.data.length === 0 && (
                  <TableRow>
                    <TableCell colSpan={10} className="h-24 text-center text-muted-foreground">
                      No webhook deliveries found.
                    </TableCell>
                  </TableRow>
                )}
                {webhooks.data?.data.map((webhook) => (
                  <TableRow key={webhook.id}>
                    <TableCell>
                      <Link
                        to={`/payments/${webhook.deposit_address_id}`}
                        className="font-mono text-xs underline-offset-4 hover:underline"
                      >
                        {shortHash(webhook.address, 6)}
                      </Link>
                      <p className="text-xs text-muted-foreground">
                        {networkLabel(webhook.network)}
                      </p>
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary" className="capitalize">
                        {webhook.event}
                      </Badge>
                    </TableCell>
                    <TableCell>
                      {webhook.endpoint_url ? (
                        <code className="block max-w-48 truncate text-xs" title={webhook.endpoint_url}>
                          {webhook.endpoint_url}
                        </code>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell>
                      {webhook.tx_hash ? (
                        <code className="text-xs">{shortHash(webhook.tx_hash, 6)}</code>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell className="tabular-nums">
                      {formatAmount(webhook.amount)}
                    </TableCell>
                    <TableCell>
                      <StatusBadge status={webhook.status} />
                    </TableCell>
                    <TableCell>{webhook.attempts}</TableCell>
                    <TableCell>
                      {webhook.last_response !== null && (
                        <span className="text-xs">HTTP {webhook.last_response}</span>
                      )}
                      {webhook.last_error && (
                        <p className="max-w-48 truncate text-xs text-destructive">
                          {webhook.last_error}
                        </p>
                      )}
                      {webhook.last_response === null && !webhook.last_error && (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell className="text-xs text-muted-foreground">
                      {formatDate(webhook.updated_at)}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex justify-end gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={webhook.payload === null}
                          title={webhook.payload === null ? "No delivery attempt yet" : undefined}
                          onClick={() => setPayloadWebhook(webhook)}
                        >
                          <Braces className="size-3.5" /> Payload
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          disabled={
                            !canRetry(webhook) ||
                            retryOne.isPending ||
                            retryDelivery.isPending
                          }
                          onClick={() => handleRetry(webhook)}
                        >
                          <RefreshCw className="size-3.5" /> Retry
                        </Button>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
          {webhooks.data && (
            <Pagination
              total={webhooks.data.total}
              limit={PAGE_SIZE}
              offset={offset}
              onOffsetChange={(next) => updateParams({ offset: next ? String(next) : undefined })}
            />
          )}
        </>
      )}

      <Dialog
        open={payloadWebhook !== null}
        onOpenChange={(open) => {
          if (!open) setPayloadWebhook(null)
        }}
      >
        <DialogContent className="sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Webhook payload</DialogTitle>
            <DialogDescription>
              JSON body sent on the most recent delivery attempt
              {payloadWebhook?.endpoint_url ? ` to ${payloadWebhook.endpoint_url}` : ""}.
            </DialogDescription>
          </DialogHeader>
          {payloadWebhook?.payload && (
            <>
              <div className="max-h-[60vh] overflow-auto">
                <JsonViewer value={payloadWebhook.payload} />
              </div>
              <div className="flex justify-end">
                <CopyButton
                  value={JSON.stringify(payloadWebhook.payload, null, 2)}
                  label="Copy JSON"
                />
              </div>
            </>
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
