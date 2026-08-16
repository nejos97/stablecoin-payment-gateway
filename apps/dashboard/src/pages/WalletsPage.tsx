import { RefreshCw } from "lucide-react"
import { toast } from "sonner"

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
import { useLiveBalances, useWalletBalances } from "@/hooks/queries"
import { formatAmount, formatDate, networkLabel } from "@/lib/format"
import type { WalletBalances } from "@/lib/types"

export function WalletsPage() {
  const cached = useWalletBalances()
  const live = useLiveBalances()

  async function refreshLive() {
    toast.info("Querying balances on-chain — this can take a while…")
    const result = await live.refetch()
    if (result.error) {
      toast.error(result.error instanceof Error ? result.error.message : "Live refresh failed")
    } else {
      toast.success("Live balances loaded")
    }
  }

  const source: WalletBalances | undefined = live.data ?? cached.data
  const isLive = live.data !== undefined

  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Wallets</h1>
          <p className="text-sm text-muted-foreground">
            USDT held on the HD wallet across all derived addresses.
          </p>
        </div>
        <Button onClick={refreshLive} disabled={live.isFetching} variant="outline">
          <RefreshCw className={live.isFetching ? "size-4 animate-spin" : "size-4"} />
          {live.isFetching ? "Refreshing on-chain…" : "Refresh live"}
        </Button>
      </div>

      {cached.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        <>
          <Card>
            <CardHeader className="pb-2">
              <CardDescription>
                Total balance{" "}
                {isLive ? "(live)" : source?.updated_at ? `(synced ${formatDate(source.updated_at)})` : "(no sync yet)"}
              </CardDescription>
            </CardHeader>
            <CardContent>
              <p className="text-3xl font-semibold tabular-nums">
                {source ? formatAmount(source.total_usdt) : "—"}
              </p>
            </CardContent>
          </Card>

          <div className="grid gap-4 sm:grid-cols-3">
            {source?.networks.map((network) => (
              <Card key={network.network}>
                <CardHeader className="pb-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-base">
                      {networkLabel(network.network)}
                    </CardTitle>
                    {!network.available && <StatusBadge status="inactive" />}
                  </div>
                </CardHeader>
                <CardContent>
                  <p className="text-2xl font-semibold tabular-nums">
                    {formatAmount(network.amount, network.token)}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {network.indices_scanned !== undefined &&
                      `${network.indices_scanned} HD indices scanned`}
                    {network.addresses_checked !== undefined &&
                      `${network.addresses_checked} addresses checked`}
                  </p>
                  {network.message && (
                    <p className="mt-1 text-xs text-muted-foreground">{network.message}</p>
                  )}
                </CardContent>
              </Card>
            ))}
          </div>

          <p className="text-xs text-muted-foreground">
            Cached balances refresh hourly in the background. “Refresh live” queries every
            address on-chain and can take several seconds per network.
          </p>
        </>
      )}
    </div>
  )
}
