import { useState } from "react"
import { KeyRound, Plus, ShieldOff, TriangleAlert } from "lucide-react"
import { toast } from "sonner"

import { CopyButton } from "@/components/shared/CopyButton"
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
import { useApiKeys, useCreateApiKey, useRevokeApiKey } from "@/hooks/queries"
import { formatDate } from "@/lib/format"
import type { CreatedApiKey } from "@/lib/types"

export function ApiKeysPage() {
  const keys = useApiKeys()
  const revokeKey = useRevokeApiKey()

  async function handleRevoke(id: string) {
    try {
      await revokeKey.mutateAsync(id)
      toast.success("API key revoked")
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Revocation failed")
    }
  }

  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">API keys</h1>
          <p className="text-sm text-muted-foreground">
            Merchant keys for the <code className="text-xs">/api/v1</code> endpoints
            (sent via the <code className="text-xs">x-api-key</code> header).
          </p>
        </div>
        <CreateApiKeyDialog />
      </div>

      {keys.isError ? (
        <QueryError error={keys.error} />
      ) : keys.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Prefix</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Expires</TableHead>
                <TableHead>Created by</TableHead>
                <TableHead>Last used</TableHead>
                <TableHead>Created</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {keys.data?.data.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="h-24 text-center text-muted-foreground">
                    No API keys yet. Merchant calls to /api/v1 require one.
                  </TableCell>
                </TableRow>
              )}
              {keys.data?.data.map((key) => (
                <TableRow key={key.id}>
                  <TableCell className="font-medium">
                    <div className="flex items-center gap-2">
                      <KeyRound className="size-3.5 text-muted-foreground" />
                      {key.name}
                    </div>
                  </TableCell>
                  <TableCell>
                    <code className="text-xs">{key.prefix}_…</code>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={key.status} />
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {key.expires_at ? formatDate(key.expires_at) : "Never"}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {key.created_by ?? "—"}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {formatDate(key.last_used_at)}
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {formatDate(key.created_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    {key.status === "active" && (
                      <AlertDialog>
                        <AlertDialogTrigger asChild>
                          <Button variant="outline" size="sm" disabled={revokeKey.isPending}>
                            <ShieldOff className="size-3.5" /> Revoke
                          </Button>
                        </AlertDialogTrigger>
                        <AlertDialogContent>
                          <AlertDialogHeader>
                            <AlertDialogTitle>Revoke “{key.name}”?</AlertDialogTitle>
                            <AlertDialogDescription>
                              Requests using this key will be rejected immediately.
                              This cannot be undone.
                            </AlertDialogDescription>
                          </AlertDialogHeader>
                          <AlertDialogFooter>
                            <AlertDialogCancel>Cancel</AlertDialogCancel>
                            <AlertDialogAction onClick={() => handleRevoke(key.id)}>
                              Revoke key
                            </AlertDialogAction>
                          </AlertDialogFooter>
                        </AlertDialogContent>
                      </AlertDialog>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}
    </div>
  )
}

const EXPIRY_CHOICES = [
  { value: "never", label: "Indefinite — until revoked", days: null },
  { value: "30", label: "1 month", days: 30 },
  { value: "90", label: "3 months", days: 90 },
  { value: "180", label: "6 months", days: 180 },
  { value: "365", label: "1 year", days: 365 },
] as const

function CreateApiKeyDialog() {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState("")
  const [expiry, setExpiry] = useState<string>("never")
  const [created, setCreated] = useState<CreatedApiKey | null>(null)
  const createKey = useCreateApiKey()

  async function handleCreate() {
    if (!name.trim()) {
      toast.error("Name is required")
      return
    }
    const choice = EXPIRY_CHOICES.find((c) => c.value === expiry)
    try {
      const key = await createKey.mutateAsync({
        name: name.trim(),
        expires_in_days: choice?.days ?? null,
      })
      setCreated(key)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Creation failed")
    }
  }

  function handleOpenChange(next: boolean) {
    setOpen(next)
    if (!next) {
      setCreated(null)
      setName("")
      setExpiry("never")
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button>
          <Plus className="size-4" /> New API key
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        {created ? (
          <>
            <DialogHeader>
              <DialogTitle>API key created</DialogTitle>
              <DialogDescription>“{created.name}”</DialogDescription>
            </DialogHeader>
            <div className="space-y-3">
              <div className="flex items-center gap-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm dark:border-amber-800 dark:bg-amber-950/40">
                <TriangleAlert className="size-4 shrink-0 text-amber-600" />
                <p>
                  Copy this key now — it is shown <span className="font-semibold">only once</span>.
                  Only a hash is stored on the server.
                </p>
              </div>
              <div className="flex items-center justify-between gap-2 rounded-lg border bg-muted/40 p-3">
                <code className="break-all text-sm">{created.key}</code>
                <CopyButton value={created.key} />
              </div>
              <p className="text-sm text-muted-foreground">
                {created.expires_at
                  ? `Expires on ${formatDate(created.expires_at)}.`
                  : "Never expires — valid until revoked."}
              </p>
            </div>
            <DialogFooter>
              <Button onClick={() => handleOpenChange(false)}>Done</Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader>
              <DialogTitle>New API key</DialogTitle>
              <DialogDescription>
                The key grants full access to the merchant /api/v1 endpoints.
              </DialogDescription>
            </DialogHeader>
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="key-name">Name</Label>
                <Input
                  id="key-name"
                  placeholder="e.g. production-shop"
                  value={name}
                  onChange={(event) => setName(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") handleCreate()
                  }}
                />
              </div>
              <div className="space-y-2">
                <Label>Expiration</Label>
                <Select value={expiry} onValueChange={setExpiry}>
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {EXPIRY_CHOICES.map((choice) => (
                      <SelectItem key={choice.value} value={choice.value}>
                        {choice.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <p className="text-xs text-muted-foreground">
                  An expired key stops working immediately and cannot be reactivated.
                </p>
              </div>
            </div>
            <DialogFooter>
              <Button onClick={handleCreate} disabled={createKey.isPending}>
                {createKey.isPending ? "Creating…" : "Create key"}
              </Button>
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
