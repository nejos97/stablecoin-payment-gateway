import * as React from "react"
import { Area, AreaChart, CartesianGrid, XAxis } from "recharts"

import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Skeleton } from "@/components/ui/skeleton"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { useDepositsTimeseries } from "@/hooks/queries"
import { useIsMobile } from "@/hooks/use-mobile"

const chartConfig = {
  volume: {
    label: "Volume (USDT)",
    color: "var(--primary)",
  },
  count: {
    label: "Deposits",
    color: "var(--primary)",
  },
} satisfies ChartConfig

const RANGES: Record<string, number> = { "90d": 90, "30d": 30, "7d": 7 }

/** Fill days with no deposits so the area reads as a continuous series. */
function fillDays(
  days: number,
  data: { date: string; count: number; volume: string }[],
) {
  const byDate = new Map(data.map((point) => [point.date, point]))
  const result: { date: string; volume: number; count: number }[] = []
  const cursor = new Date()
  cursor.setDate(cursor.getDate() - (days - 1))
  for (let i = 0; i < days; i++) {
    const key = cursor.toISOString().slice(0, 10)
    const point = byDate.get(key)
    result.push({
      date: key,
      volume: point ? Number.parseFloat(point.volume) : 0,
      count: point?.count ?? 0,
    })
    cursor.setDate(cursor.getDate() + 1)
  }
  return result
}

export function ChartAreaInteractive() {
  const isMobile = useIsMobile()
  const [timeRange, setTimeRange] = React.useState("90d")

  React.useEffect(() => {
    if (isMobile) {
      setTimeRange("7d")
    }
  }, [isMobile])

  const days = RANGES[timeRange] ?? 90
  const timeseries = useDepositsTimeseries(days)
  const chartData = React.useMemo(
    () => fillDays(days, timeseries.data?.data ?? []),
    [days, timeseries.data],
  )
  const total = React.useMemo(
    () => chartData.reduce((sum, point) => sum + point.volume, 0),
    [chartData],
  )

  return (
    <Card className="@container/card">
      <CardHeader>
        <CardTitle>Confirmed deposits</CardTitle>
        <CardDescription>
          <span className="hidden @[540px]/card:block">
            {total.toLocaleString()} USDT confirmed over the selected period
          </span>
          <span className="@[540px]/card:hidden">{total.toLocaleString()} USDT</span>
        </CardDescription>
        <CardAction>
          <ToggleGroup
            type="single"
            value={timeRange}
            onValueChange={(value) => value && setTimeRange(value)}
            variant="outline"
            className="hidden *:data-[slot=toggle-group-item]:px-4! @[767px]/card:flex"
          >
            <ToggleGroupItem value="90d">Last 3 months</ToggleGroupItem>
            <ToggleGroupItem value="30d">Last 30 days</ToggleGroupItem>
            <ToggleGroupItem value="7d">Last 7 days</ToggleGroupItem>
          </ToggleGroup>
          <Select value={timeRange} onValueChange={setTimeRange}>
            <SelectTrigger
              className="flex w-40 **:data-[slot=select-value]:block **:data-[slot=select-value]:truncate @[767px]/card:hidden"
              size="sm"
              aria-label="Select a value"
            >
              <SelectValue placeholder="Last 3 months" />
            </SelectTrigger>
            <SelectContent className="rounded-xl">
              <SelectItem value="90d" className="rounded-lg">
                Last 3 months
              </SelectItem>
              <SelectItem value="30d" className="rounded-lg">
                Last 30 days
              </SelectItem>
              <SelectItem value="7d" className="rounded-lg">
                Last 7 days
              </SelectItem>
            </SelectContent>
          </Select>
        </CardAction>
      </CardHeader>
      <CardContent className="px-2 pt-4 sm:px-6 sm:pt-6">
        {timeseries.isLoading ? (
          <Skeleton className="h-[250px] w-full" />
        ) : (
          <ChartContainer
            config={chartConfig}
            className="aspect-auto h-[250px] w-full"
          >
            <AreaChart data={chartData}>
              <defs>
                <linearGradient id="fillVolume" x1="0" y1="0" x2="0" y2="1">
                  <stop
                    offset="5%"
                    stopColor="var(--color-volume)"
                    stopOpacity={0.8}
                  />
                  <stop
                    offset="95%"
                    stopColor="var(--color-volume)"
                    stopOpacity={0.1}
                  />
                </linearGradient>
              </defs>
              <CartesianGrid vertical={false} />
              <XAxis
                dataKey="date"
                tickLine={false}
                axisLine={false}
                tickMargin={8}
                minTickGap={32}
                tickFormatter={(value) => {
                  const date = new Date(value)
                  return date.toLocaleDateString("en-US", {
                    month: "short",
                    day: "numeric",
                  })
                }}
              />
              <ChartTooltip
                cursor={false}
                content={
                  <ChartTooltipContent
                    labelFormatter={(value) => {
                      return new Date(value).toLocaleDateString("en-US", {
                        month: "short",
                        day: "numeric",
                      })
                    }}
                    indicator="dot"
                  />
                }
              />
              <Area
                dataKey="volume"
                type="natural"
                fill="url(#fillVolume)"
                stroke="var(--color-volume)"
              />
            </AreaChart>
          </ChartContainer>
        )}
      </CardContent>
    </Card>
  )
}
