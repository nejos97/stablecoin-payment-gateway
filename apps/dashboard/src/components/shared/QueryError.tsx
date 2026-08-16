import { AlertTriangle } from "lucide-react"

export function QueryError({ error }: { error: unknown }) {
  const message =
    error instanceof Error ? error.message : "Something went wrong while loading data"
  return (
    <div className="flex items-center gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm">
      <AlertTriangle className="size-4 shrink-0 text-destructive" />
      <div>
        <p className="font-medium">Failed to load</p>
        <p className="text-muted-foreground">
          {message} — check that the backend is running, then refresh.
        </p>
      </div>
    </div>
  )
}
