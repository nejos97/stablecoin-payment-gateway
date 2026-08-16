import { useState } from "react"
import { zodResolver } from "@hookform/resolvers/zod"
import { Ban, CircleCheck, MoreHorizontal, Plus, UserCog } from "lucide-react"
import { useForm } from "react-hook-form"
import { toast } from "sonner"
import { z } from "zod"

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
} from "@/components/ui/alert-dialog"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
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
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
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
import { useCreateStaff, useStaffList, useUpdateStaff } from "@/hooks/queries"
import { useAuth } from "@/lib/auth"
import { formatDate, initials } from "@/lib/format"
import type { Staff } from "@/lib/types"

export function StaffPage() {
  const { staff: me } = useAuth()
  const staffList = useStaffList()
  const updateStaff = useUpdateStaff()
  const [editing, setEditing] = useState<Staff | null>(null)
  const [blocking, setBlocking] = useState<Staff | null>(null)

  async function blockMember(member: Staff) {
    try {
      await updateStaff.mutateAsync({ id: member.id, is_active: false })
      toast.success(
        `${member.first_name} ${member.last_name} is blocked — their sessions were revoked`,
      )
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Blocking failed")
    } finally {
      setBlocking(null)
    }
  }

  async function unblockMember(member: Staff) {
    try {
      await updateStaff.mutateAsync({ id: member.id, is_active: true })
      toast.success(`${member.first_name} ${member.last_name} can sign in again`)
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Unblocking failed")
    }
  }

  return (
    <div className="space-y-6 p-4 lg:p-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Staff</h1>
          <p className="text-sm text-muted-foreground">
            Dashboard accounts. Admins manage everything; operators handle daily
            operations only.
          </p>
        </div>
        <CreateStaffDialog />
      </div>

      {staffList.isError ? (
        <QueryError error={staffList.error} />
      ) : staffList.isLoading ? (
        <Skeleton className="h-64 w-full" />
      ) : (
        <div className="rounded-lg border">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Member</TableHead>
                <TableHead>Role</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Created</TableHead>
                <TableHead className="text-right">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {staffList.data?.data.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="h-24 text-center text-muted-foreground">
                    No staff accounts found.
                  </TableCell>
                </TableRow>
              )}
              {staffList.data?.data.map((member) => (
                <TableRow key={member.id}>
                  <TableCell>
                    <div className="flex items-center gap-3">
                      <Avatar className="size-8">
                        <AvatarFallback className="text-xs">
                          {initials(member.first_name, member.last_name)}
                        </AvatarFallback>
                      </Avatar>
                      <div className="leading-tight">
                        <p className="font-medium">
                          {member.first_name} {member.last_name}
                          {member.id === me?.id && (
                            <span className="ml-2 text-xs text-muted-foreground">(you)</span>
                          )}
                        </p>
                        <p className="text-xs text-muted-foreground">{member.email}</p>
                      </div>
                    </div>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={member.role} />
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={member.is_active ? "active" : "blocked"} />
                  </TableCell>
                  <TableCell className="text-muted-foreground">
                    {formatDate(member.created_at)}
                  </TableCell>
                  <TableCell className="text-right">
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button variant="ghost" size="icon-sm" aria-label="Actions">
                          <MoreHorizontal className="size-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        <DropdownMenuItem onClick={() => setEditing(member)}>
                          <UserCog /> Edit
                        </DropdownMenuItem>
                        {member.id !== me?.id &&
                          (member.is_active ? (
                            <DropdownMenuItem
                              variant="destructive"
                              onClick={() => setBlocking(member)}
                            >
                              <Ban /> Block
                            </DropdownMenuItem>
                          ) : (
                            <DropdownMenuItem onClick={() => unblockMember(member)}>
                              <CircleCheck /> Unblock
                            </DropdownMenuItem>
                          ))}
                      </DropdownMenuContent>
                    </DropdownMenu>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      )}

      {editing && (
        <EditStaffDialog member={editing} onClose={() => setEditing(null)} />
      )}

      <AlertDialog open={blocking !== null} onOpenChange={(open) => !open && setBlocking(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              Block {blocking?.first_name} {blocking?.last_name}?
            </AlertDialogTitle>
            <AlertDialogDescription>
              They will be signed out immediately and unable to log in until an
              admin unblocks them. Their account and history are kept.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={() => blocking && blockMember(blocking)}>
              Block account
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  )
}

const createSchema = z.object({
  first_name: z.string().trim().min(1, "Required"),
  last_name: z.string().trim().min(1, "Required"),
  email: z.string().trim().email("Invalid email"),
  password: z.string().min(8, "At least 8 characters"),
  role: z.enum(["admin", "operator"]),
})

type CreateValues = z.infer<typeof createSchema>

function CreateStaffDialog() {
  const [open, setOpen] = useState(false)
  const createStaff = useCreateStaff()
  const form = useForm<CreateValues>({
    resolver: zodResolver(createSchema),
    defaultValues: { role: "operator" },
  })

  async function onSubmit(values: CreateValues) {
    try {
      await createStaff.mutateAsync(values)
      toast.success(`${values.first_name} ${values.last_name} added`)
      setOpen(false)
      form.reset({ role: "operator" })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Creation failed")
    }
  }

  const { errors, isSubmitting } = form.formState

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button>
          <Plus className="size-4" /> Add staff
        </Button>
      </DialogTrigger>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add staff member</DialogTitle>
          <DialogDescription>
            They can sign in immediately with the password you set.
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="create-first-name">First name</Label>
              <Input id="create-first-name" {...form.register("first_name")} />
              {errors.first_name && (
                <p className="text-xs text-destructive">{errors.first_name.message}</p>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="create-last-name">Last name</Label>
              <Input id="create-last-name" {...form.register("last_name")} />
              {errors.last_name && (
                <p className="text-xs text-destructive">{errors.last_name.message}</p>
              )}
            </div>
          </div>
          <div className="space-y-2">
            <Label htmlFor="create-email">Email</Label>
            <Input id="create-email" type="email" {...form.register("email")} />
            {errors.email && <p className="text-xs text-destructive">{errors.email.message}</p>}
          </div>
          <div className="space-y-2">
            <Label htmlFor="create-password">Password</Label>
            <Input id="create-password" type="password" {...form.register("password")} />
            {errors.password && (
              <p className="text-xs text-destructive">{errors.password.message}</p>
            )}
          </div>
          <div className="space-y-2">
            <Label>Role</Label>
            <Select
              value={form.watch("role")}
              onValueChange={(value) => form.setValue("role", value as CreateValues["role"])}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="operator">Operator — daily operations</SelectItem>
                <SelectItem value="admin">Admin — full access</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <DialogFooter>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Adding…" : "Add member"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

const editSchema = z.object({
  first_name: z.string().trim().min(1, "Required"),
  last_name: z.string().trim().min(1, "Required"),
  role: z.enum(["admin", "operator"]),
  password: z
    .string()
    .optional()
    .refine((value) => !value || value.length >= 8, {
      message: "At least 8 characters",
    }),
})

type EditValues = z.infer<typeof editSchema>

function EditStaffDialog({ member, onClose }: { member: Staff; onClose: () => void }) {
  const updateStaff = useUpdateStaff()
  const form = useForm<EditValues>({
    resolver: zodResolver(editSchema),
    defaultValues: {
      first_name: member.first_name,
      last_name: member.last_name,
      role: member.role,
    },
  })

  async function onSubmit(values: EditValues) {
    try {
      await updateStaff.mutateAsync({
        id: member.id,
        first_name: values.first_name,
        last_name: values.last_name,
        role: values.role,
        password: values.password || undefined,
      })
      toast.success("Staff member updated")
      onClose()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Update failed")
    }
  }

  const { errors, isSubmitting } = form.formState

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            Edit {member.first_name} {member.last_name}
          </DialogTitle>
          <DialogDescription>{member.email}</DialogDescription>
        </DialogHeader>
        <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label htmlFor="edit-first-name">First name</Label>
              <Input id="edit-first-name" {...form.register("first_name")} />
              {errors.first_name && (
                <p className="text-xs text-destructive">{errors.first_name.message}</p>
              )}
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-last-name">Last name</Label>
              <Input id="edit-last-name" {...form.register("last_name")} />
              {errors.last_name && (
                <p className="text-xs text-destructive">{errors.last_name.message}</p>
              )}
            </div>
          </div>
          <div className="space-y-2">
            <Label>Role</Label>
            <Select
              value={form.watch("role")}
              onValueChange={(value) => form.setValue("role", value as EditValues["role"])}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="operator">Operator — daily operations</SelectItem>
                <SelectItem value="admin">Admin — full access</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-2">
            <Label htmlFor="edit-password">New password (optional)</Label>
            <Input
              id="edit-password"
              type="password"
              placeholder="Leave empty to keep current"
              {...form.register("password")}
            />
            {errors.password && (
              <p className="text-xs text-destructive">{errors.password.message}</p>
            )}
          </div>
          <DialogFooter>
            <Button type="submit" disabled={isSubmitting}>
              {isSubmitting ? "Saving…" : "Save changes"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
