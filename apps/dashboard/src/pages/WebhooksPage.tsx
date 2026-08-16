import { RefreshCw, RotateCcw } from "lucide-react"
import { Link, useSearchParams } from "react-router-dom"
import { toast } from "sonner"

import { Pagination } from "@/components/shared/Pagination"
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
  AlertDialogTrigger,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
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
  useWebhooks,
} from "@/hooks/queries"
import { formatAmount, formatDate, networkLabel, shortHash } from "@/lib/format"

const PAGE_SIZE = 20
const ALL = "all"

export function WebhooksPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const status = searchParams.get("status") ?? ALL
  const offset = Number(searchParams.get("offset") ?? 0)

  const webhooks = useWebhooks({
    status: status === ALL ? undefined : status,
    limit: PAGE_SIZE,
    offset,
  })
  const retryOne = useRetryWebhook()
  const retryAll = useRetryAllFailedWebhooks()

  function updateParams(updates: Record<string, string | undefined>) {
    const next = new URLSearchParams(searchParams)
    for (const [key, value] of Object.entries(updates)) {
      if (value === undefined || value === ALL) next.delete(key)
      else next.set(key, value)
    }
    setSearchParams(next)
  }

  async function handleRetry(depositId: string) {
    try {
      await retryOne.mutateAsync(depositId)
      toast.success("Webhook re-queued")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Retry failed")
    }
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
          <h1 className="text-2xl font-semibold tracking-tight">Webhooks</h1>
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
                Every failed delivery for a confirmed deposit is reset and re-queued
                for an immediate attempt, restarting its full retry cycle.
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
                    <TableCell colSpan={9} className="h-24 text-center text-muted-foreground">
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
                      {webhook.endpoint_url ? (
                        <code className="block max-w-48 truncate text-xs" title={webhook.endpoint_url}>
                          {webhook.endpoint_url}
                        </code>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell>
                      <code className="text-xs">{shortHash(webhook.tx_hash, 6)}</code>
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
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={
                          webhook.deposit_status !== "confirmed" || retryOne.isPending
                        }
                        onClick={() => handleRetry(webhook.deposit_id)}
                      >
                        <RefreshCw className="size-3.5" /> Retry
                      </Button>
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
    </div>
  )
}
