import { ChartAreaInteractive } from "@/components/chart-area-interactive"
import { SectionCards } from "@/components/section-cards"
import { StatusBadge } from "@/components/shared/StatusBadge"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { useWalletBalances } from "@/hooks/queries"
import { formatAmount, networkLabel } from "@/lib/format"

export function OverviewPage() {
  const balances = useWalletBalances()

  return (
    <div className="flex flex-col gap-4 py-4 md:gap-6 md:py-6">
      <SectionCards />
      <div className="px-4 lg:px-6">
        <ChartAreaInteractive />
      </div>
      <div className="px-4 lg:px-6">
        <Card>
          <CardHeader>
            <CardTitle>Wallet balances by network</CardTitle>
            <CardDescription>
              From the hourly HD-wallet sync — see the Wallets page for a live refresh.
            </CardDescription>
          </CardHeader>
          <CardContent>
            {balances.isLoading ? (
              <Skeleton className="h-24 w-full" />
            ) : (
              <div className="grid gap-4 sm:grid-cols-3">
                {balances.data?.networks.map((network) => (
                  <div key={network.network} className="rounded-lg border p-4">
                    <div className="flex items-center justify-between">
                      <p className="font-medium">{networkLabel(network.network)}</p>
                      {!network.available && <StatusBadge status="inactive" />}
                    </div>
                    <p className="mt-1 text-xl font-semibold tabular-nums">
                      {formatAmount(network.amount, network.token)}
                    </p>
                    {network.message && (
                      <p className="mt-1 text-xs text-muted-foreground">{network.message}</p>
                    )}
                  </div>
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
