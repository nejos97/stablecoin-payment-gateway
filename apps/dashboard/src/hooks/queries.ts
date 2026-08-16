import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import { api } from "@/lib/api"
import type {
  AdminStats,
  ApiKey,
  AppSettings,
  AppSettingsUpdate,
  CreatedApiKey,
  DepositAddress,
  DepositAddressDetail,
  Paginated,
  Staff,
  WalletBalances,
  WebhookDelivery,
  WebhookEndpoint,
} from "@/lib/types"

function listParams(params: Record<string, string | number | undefined>): string {
  const query = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== "") query.set(key, String(value))
  }
  const encoded = query.toString()
  return encoded ? `?${encoded}` : ""
}

export function useStats() {
  return useQuery({
    queryKey: ["stats"],
    queryFn: () => api<AdminStats>("/stats"),
    refetchInterval: 30_000,
  })
}

export interface DepositPoint {
  date: string
  count: number
  volume: string
}

export function useDepositsTimeseries(days: number) {
  return useQuery({
    queryKey: ["deposits-timeseries", days],
    queryFn: () => api<{ days: number; data: DepositPoint[] }>(
      `/stats/deposits-timeseries?days=${days}`,
    ),
    refetchInterval: 60_000,
  })
}

export function useWalletBalances() {
  return useQuery({
    queryKey: ["wallet-balances"],
    queryFn: () => api<WalletBalances>("/wallet-balances"),
  })
}

/** Slow endpoint (one RPC call per address) — only ever fetched on demand. */
export function useLiveBalances() {
  return useQuery({
    queryKey: ["live-balances"],
    queryFn: () => api<WalletBalances>("/balances"),
    enabled: false,
    gcTime: 0,
  })
}

export interface PaymentFilters {
  status?: string
  network?: string
  limit: number
  offset: number
}

export function usePayments(filters: PaymentFilters) {
  return useQuery({
    queryKey: ["payments", filters],
    queryFn: () =>
      api<Paginated<DepositAddress>>(`/deposit-addresses${listParams({ ...filters })}`),
  })
}

export function usePayment(id: string | undefined) {
  return useQuery({
    queryKey: ["payments", id],
    queryFn: () => api<DepositAddressDetail>(`/deposit-addresses/${id}`),
    enabled: Boolean(id),
  })
}

export interface CreatePaymentPayload {
  network: string
  token: string
  expected_amount: string
  expires_at?: string
  metadata?: Record<string, unknown>
}

export function useCreatePayment() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: CreatePaymentPayload) =>
      api<DepositAddress>("/deposit-addresses", { body: payload }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["payments"] })
      queryClient.invalidateQueries({ queryKey: ["stats"] })
    },
  })
}

export interface WebhookFilters {
  status?: string
  limit: number
  offset: number
}

export function useWebhooks(filters: WebhookFilters) {
  return useQuery({
    queryKey: ["webhooks", filters],
    queryFn: () => api<Paginated<WebhookDelivery>>(`/webhooks${listParams({ ...filters })}`),
  })
}

export function useRetryWebhook() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (depositId: string) =>
      api<{ status: string }>(`/deposits/${depositId}/webhooks/retry`, { method: "POST" }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["webhooks"] })
      queryClient.invalidateQueries({ queryKey: ["payments"] })
      queryClient.invalidateQueries({ queryKey: ["stats"] })
    },
  })
}

export function useRetryAllFailedWebhooks() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: () => api<{ retried: number }>("/webhooks/retry-failed", { method: "POST" }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["webhooks"] })
      queryClient.invalidateQueries({ queryKey: ["stats"] })
    },
  })
}

export function useSettings() {
  return useQuery({
    queryKey: ["settings"],
    queryFn: () => api<AppSettings>("/settings"),
  })
}

export function useUpdateSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (settings: AppSettingsUpdate) =>
      api<AppSettings>("/settings", { method: "PATCH", body: settings }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["settings"] }),
  })
}

export function useWebhookEndpoints() {
  return useQuery({
    queryKey: ["webhook-endpoints"],
    queryFn: () => api<{ data: WebhookEndpoint[] }>("/webhook-endpoints"),
  })
}

export function useCreateWebhookEndpoint() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (url: string) => api<WebhookEndpoint>("/webhook-endpoints", { body: { url } }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["webhook-endpoints"] }),
  })
}

export function useToggleWebhookEndpoint() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, is_active }: { id: string; is_active: boolean }) =>
      api<WebhookEndpoint>(`/webhook-endpoints/${id}`, {
        method: "PATCH",
        body: { is_active },
      }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["webhook-endpoints"] })
      queryClient.invalidateQueries({ queryKey: ["webhooks"] })
    },
  })
}

export function useApiKeys() {
  return useQuery({
    queryKey: ["api-keys"],
    queryFn: () => api<{ data: ApiKey[] }>("/api-keys"),
  })
}

export function useCreateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: { name: string; expires_in_days: number | null }) =>
      api<CreatedApiKey>("/api-keys", { body: payload }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["api-keys"] }),
  })
}

export function useRevokeApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => api<{ status: string }>(`/api-keys/${id}`, { method: "DELETE" }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["api-keys"] }),
  })
}

export function useStaffList() {
  return useQuery({
    queryKey: ["staff"],
    queryFn: () => api<Paginated<Staff>>("/staff?limit=100"),
  })
}

export interface CreateStaffPayload {
  email: string
  password: string
  first_name: string
  last_name: string
  role: string
}

export function useCreateStaff() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (payload: CreateStaffPayload) => api<Staff>("/staff", { body: payload }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["staff"] }),
  })
}

export interface UpdateStaffPayload {
  id: string
  first_name?: string
  last_name?: string
  role?: string
  is_active?: boolean
  password?: string
}

export function useUpdateStaff() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, ...payload }: UpdateStaffPayload) =>
      api<Staff>(`/staff/${id}`, { method: "PATCH", body: payload }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["staff"] }),
  })
}
