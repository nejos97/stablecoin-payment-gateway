import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

const STYLES: Record<string, string> = {
  // deposit addresses
  pending: "bg-amber-100 text-amber-900 dark:bg-amber-900/40 dark:text-amber-200",
  paid: "bg-emerald-100 text-emerald-900 dark:bg-emerald-900/40 dark:text-emerald-200",
  expired: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  // deposits
  detected: "bg-sky-100 text-sky-900 dark:bg-sky-900/40 dark:text-sky-200",
  confirmed: "bg-emerald-100 text-emerald-900 dark:bg-emerald-900/40 dark:text-emerald-200",
  // webhooks
  delivered: "bg-emerald-100 text-emerald-900 dark:bg-emerald-900/40 dark:text-emerald-200",
  failed: "bg-red-100 text-red-900 dark:bg-red-900/40 dark:text-red-200",
  // staff / api keys
  active: "bg-emerald-100 text-emerald-900 dark:bg-emerald-900/40 dark:text-emerald-200",
  inactive: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  blocked: "bg-red-100 text-red-900 dark:bg-red-900/40 dark:text-red-200",
  revoked: "bg-red-100 text-red-900 dark:bg-red-900/40 dark:text-red-200",
  admin: "bg-violet-100 text-violet-900 dark:bg-violet-900/40 dark:text-violet-200",
  operator: "bg-sky-100 text-sky-900 dark:bg-sky-900/40 dark:text-sky-200",
}

export function StatusBadge({ status }: { status: string }) {
  return (
    <Badge variant="secondary" className={cn("capitalize", STYLES[status])}>
      {status}
    </Badge>
  )
}
