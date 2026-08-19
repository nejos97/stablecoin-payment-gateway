import {
  AlertTriangleIcon,
  CheckCircle2Icon,
  ClockIcon,
  WalletIcon,
} from "lucide-react"
import { Link } from "react-router-dom"

import { Badge } from "@/components/ui/badge"
import {
  Card,
  CardAction,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"
import { useStats, useWalletBalances } from "@/hooks/queries"
import { formatDate } from "@/lib/format"

export function SectionCards() {
  const stats = useStats()
  const balances = useWalletBalances()

  const availableNetworks =
    balances.data?.networks.filter((n) => n.available).length ?? 0
  const failed = stats.data?.webhooks.failed ?? 0

  return (
    <div className="grid grid-cols-1 gap-4 px-4 *:data-[slot=card]:bg-linear-to-t *:data-[slot=card]:from-primary/5 *:data-[slot=card]:to-card *:data-[slot=card]:shadow-xs lg:px-6 @xl/main:grid-cols-2 @5xl/main:grid-cols-4 dark:*:data-[slot=card]:bg-card">
      <Card className="@container/card">
        <CardHeader>
          <CardDescription>Total balance</CardDescription>
          <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {balances.isLoading ? (
              <Skeleton className="h-8 w-28" />
            ) : (
              `${balances.data?.total_usdt ?? "0"} USDT`
            )}
          </CardTitle>
          <CardAction>
            <Badge variant="outline">
              <WalletIcon />
              {availableNetworks}/3 networks
            </Badge>
          </CardAction>
        </CardHeader>
        <CardFooter className="flex-col items-start gap-1.5 text-sm">
          <div className="line-clamp-1 flex gap-2 font-medium">
            Across all HD wallet addresses
          </div>
          <div className="text-muted-foreground">
            {balances.data?.updated_at
              ? `Synced ${formatDate(balances.data.updated_at)}`
              : "No sync yet — check provider API keys"}
          </div>
        </CardFooter>
      </Card>
      <Card className="@container/card">
        <CardHeader>
          <CardDescription>Confirmed volume</CardDescription>
          <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {stats.isLoading ? (
              <Skeleton className="h-8 w-28" />
            ) : (
              `${stats.data?.confirmed_volume ?? "0"} USDT`
            )}
          </CardTitle>
          <CardAction>
            <Badge variant="outline">
              <CheckCircle2Icon />
              {stats.data?.deposits.confirmed ?? 0} deposits
            </Badge>
          </CardAction>
        </CardHeader>
        <CardFooter className="flex-col items-start gap-1.5 text-sm">
          <div className="line-clamp-1 flex gap-2 font-medium">
            All-time confirmed deposits
          </div>
          <div className="text-muted-foreground">
            {stats.data?.deposits.detected ?? 0} more detected, awaiting confirmations
          </div>
        </CardFooter>
      </Card>
      <Card className="@container/card">
        <CardHeader>
          <CardDescription>Pending payments</CardDescription>
          <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {stats.isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              (stats.data?.addresses.pending ?? 0)
            )}
          </CardTitle>
          <CardAction>
            <Badge variant="outline">
              <ClockIcon />
              {stats.data?.addresses.paid ?? 0} paid
            </Badge>
          </CardAction>
        </CardHeader>
        <CardFooter className="flex-col items-start gap-1.5 text-sm">
          <div className="line-clamp-1 flex gap-2 font-medium">
            Deposit addresses awaiting funds
          </div>
          <div className="text-muted-foreground">
            {stats.data?.addresses.expired ?? 0} expired without payment
          </div>
        </CardFooter>
      </Card>
      <Card className="@container/card">
        <CardHeader>
          <CardDescription>Webhooks delivered</CardDescription>
          <CardTitle className="text-2xl font-semibold tabular-nums @[250px]/card:text-3xl">
            {stats.isLoading ? (
              <Skeleton className="h-8 w-16" />
            ) : (
              (stats.data?.webhooks.delivered ?? 0)
            )}
          </CardTitle>
          <CardAction>
            <Badge variant={failed > 0 ? "destructive" : "outline"}>
              <AlertTriangleIcon />
              {failed} failed
            </Badge>
          </CardAction>
        </CardHeader>
        <CardFooter className="flex-col items-start gap-1.5 text-sm">
          <div className="line-clamp-1 flex gap-2 font-medium">
            {stats.data?.webhooks.pending ?? 0} pending delivery
          </div>
          <div className="text-muted-foreground">
            {failed > 0 ? (
              <Link to="/deliveries?status=failed" className="underline underline-offset-4">
                Review failed deliveries
              </Link>
            ) : (
              "All callbacks acknowledged"
            )}
          </div>
        </CardFooter>
      </Card>
    </div>
  )
}
