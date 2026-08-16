import { zodResolver } from "@hookform/resolvers/zod"
import { useForm } from "react-hook-form"
import { Navigate, useNavigate } from "react-router-dom"
import { toast } from "sonner"
import { z } from "zod"

import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { useAuth } from "@/lib/auth"

const schema = z
  .object({
    first_name: z.string().trim().min(1, "First name is required"),
    last_name: z.string().trim().min(1, "Last name is required"),
    email: z.string().trim().email("Invalid email address"),
    password: z.string().min(8, "At least 8 characters"),
    confirm_password: z.string(),
  })
  .refine((data) => data.password === data.confirm_password, {
    message: "Passwords do not match",
    path: ["confirm_password"],
  })

type FormValues = z.infer<typeof schema>

export function SetupPage() {
  const { needsSetup, loading, completeSetup } = useAuth()
  const navigate = useNavigate()
  const form = useForm<FormValues>({ resolver: zodResolver(schema) })

  if (!loading && !needsSetup) {
    return <Navigate to="/" replace />
  }

  async function onSubmit(values: FormValues) {
    try {
      await completeSetup({
        email: values.email,
        password: values.password,
        first_name: values.first_name,
        last_name: values.last_name,
      })
      toast.success("Admin account created — welcome!")
      navigate("/", { replace: true })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Setup failed")
    }
  }

  const { errors, isSubmitting } = form.formState

  return (
    <div className="flex min-h-svh items-center justify-center bg-muted/40 p-4">
      <Card className="w-full max-w-md">
        <CardHeader>
          <CardTitle>Initial setup</CardTitle>
          <CardDescription>
            No admin account exists yet. Create the first administrator to start
            managing the payment gateway.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-4">
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="first_name">First name</Label>
                <Input id="first_name" autoComplete="given-name" {...form.register("first_name")} />
                <FieldError message={errors.first_name?.message} />
              </div>
              <div className="space-y-2">
                <Label htmlFor="last_name">Last name</Label>
                <Input id="last_name" autoComplete="family-name" {...form.register("last_name")} />
                <FieldError message={errors.last_name?.message} />
              </div>
            </div>
            <div className="space-y-2">
              <Label htmlFor="email">Email</Label>
              <Input id="email" type="email" autoComplete="email" {...form.register("email")} />
              <FieldError message={errors.email?.message} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="password">Password</Label>
              <Input
                id="password"
                type="password"
                autoComplete="new-password"
                {...form.register("password")}
              />
              <FieldError message={errors.password?.message} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="confirm_password">Confirm password</Label>
              <Input
                id="confirm_password"
                type="password"
                autoComplete="new-password"
                {...form.register("confirm_password")}
              />
              <FieldError message={errors.confirm_password?.message} />
            </div>
            <Button type="submit" className="w-full" disabled={isSubmitting}>
              {isSubmitting ? "Creating account…" : "Create admin account"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  )
}

function FieldError({ message }: { message?: string }) {
  if (!message) return null
  return <p className="text-xs text-destructive">{message}</p>
}
