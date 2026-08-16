import { ArrowLeft, RefreshCw } from "lucide-react"
import { Link, useParams } from "react-router-dom"
import { toast } from "sonner"

import { CopyButton } from "@/components/shared/CopyButton"
import { StatusBadge } from "@/components/shared/StatusBadge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { usePayment, useRetryWebhook } from "@/hooks/queries"
import { formatAmount, formatDate, networkLabel, shortHash } from "@/lib/format"

export function PaymentDetailPage() {
  const { id } = useParams<{ id: string }>()
  const payment = usePayment(id)
  const retryWebhook = useRetryWebhook()

  async function retry(depositId: string) {
    try {
      await retryWebhook.mutateAsync(depositId)
      toast.success("Webhook re-queued")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Retry failed")
    }
  }

  if (payment.isLoading) {
    return <Skeleton className="h-96 w-full" />
  }
  if (payment.isError || !payment.data) {
    return (
      <div className="space-y-4 p-4 lg:p-6">
        <Button asChild variant="ghost" size="sm">
          <Link to="/payments">
            <ArrowLeft className="size-4" /> Back to payments
          </Link>
        </Button>
        <p className="text-sm text-destructive">
          {payment.error instanceof Error ? payment.error.message : "Payment not found"}
        </p>
      </div>
    )
  }

  const data = payment.data
  const metadata = JSON.stringify(data.metadata ?? {}, null, 2)

  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex items-center gap-3">
        <Button asChild variant="ghost" size="sm">
          <Link to="/payments">
            <ArrowLeft className="size-4" /> Back
          </Link>
        </Button>
        <h1 className="text-2xl font-semibold tracking-tight">Payment detail</h1>
        <StatusBadge status={data.status} />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Deposit address</CardTitle>
            <CardDescription>
              {networkLabel(data.network)} · {data.token}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between gap-2 rounded-lg border bg-muted/40 p-3">
              <code className="break-all text-sm">{data.address}</code>
              <CopyButton value={data.address} />
            </div>
            <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
              <dt className="text-muted-foreground">Expected amount</dt>
              <dd className="tabular-nums">{formatAmount(data.expected_amount, data.token)}</dd>
              <dt className="text-muted-foreground">Expires at</dt>
              <dd>{formatDate(data.expires_at)}</dd>
              <dt className="text-muted-foreground">Created at</dt>
              <dd>{formatDate(data.created_at)}</dd>
              <dt className="text-muted-foreground">ID</dt>
              <dd className="break-all font-mono text-xs">{data.id}</dd>
            </dl>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Metadata</CardTitle>
          </CardHeader>
          <CardContent>
            <pre className="max-h-56 overflow-auto rounded-lg bg-muted/40 p-3 text-xs">
              {metadata}
            </pre>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Deposits</CardTitle>
          <CardDescription>
            On-chain transfers detected for this address, with their webhook state.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {data.deposits.length === 0 ? (
            <p className="py-8 text-center text-sm text-muted-foreground">
              No deposits detected yet.
            </p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Tx hash</TableHead>
                  <TableHead>Amount</TableHead>
                  <TableHead>Confirmations</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Webhook</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.deposits.map((deposit) => (
                  <TableRow key={deposit.id}>
                    <TableCell>
                      <div className="flex items-center gap-1">
                        <code className="text-xs">{shortHash(deposit.tx_hash)}</code>
                        <CopyButton value={deposit.tx_hash} />
                      </div>
                    </TableCell>
                    <TableCell className="tabular-nums">
                      {formatAmount(deposit.amount, data.token)}
                    </TableCell>
                    <TableCell>{deposit.confirmations}</TableCell>
                    <TableCell>
                      <StatusBadge status={deposit.status} />
                    </TableCell>
                    <TableCell>
                      {(deposit.webhooks?.length ?? 0) > 0 ? (
                        <div className="space-y-2">
                          {deposit.webhooks.map((entry) => (
                            <div key={entry.endpoint_id} className="space-y-0.5">
                              <div className="flex items-center gap-2">
                                <StatusBadge status={entry.status} />
                                {entry.endpoint_url && (
                                  <code
                                    className="max-w-44 truncate text-xs text-muted-foreground"
                                    title={entry.endpoint_url}
                                  >
                                    {entry.endpoint_url}
                                  </code>
                                )}
                              </div>
                              <p className="text-xs text-muted-foreground">
                                {entry.attempts} attempt{entry.attempts === 1 ? "" : "s"}
                                {entry.last_response !== null && ` · HTTP ${entry.last_response}`}
                              </p>
                              {entry.last_error && (
                                <p className="max-w-56 truncate text-xs text-destructive">
                                  {entry.last_error}
                                </p>
                              )}
                            </div>
                          ))}
                        </div>
                      ) : deposit.webhook ? (
                        <div className="space-y-1">
                          <StatusBadge status={deposit.webhook.status} />
                          <p className="text-xs text-muted-foreground">
                            {deposit.webhook.attempts} attempt
                            {deposit.webhook.attempts === 1 ? "" : "s"}
                            {deposit.webhook.last_response !== null &&
                              ` · HTTP ${deposit.webhook.last_response}`}
                          </p>
                          {deposit.webhook.last_error && (
                            <p className="max-w-56 truncate text-xs text-destructive">
                              {deposit.webhook.last_error}
                            </p>
                          )}
                        </div>
                      ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="outline"
                        size="sm"
                        disabled={deposit.status !== "confirmed" || retryWebhook.isPending}
                        onClick={() => retry(deposit.id)}
                      >
                        <RefreshCw className="size-3.5" /> Retry webhook
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
