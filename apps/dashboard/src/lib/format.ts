export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—"
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) return "—"
  return date.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  })
}

/** Amounts arrive as decimal strings — displayed as-is, never parsed to float. */
export function formatAmount(amount: string, token = "USDT"): string {
  return `${amount} ${token}`
}

export function shortHash(value: string, keep = 8): string {
  if (value.length <= keep * 2 + 1) return value
  return `${value.slice(0, keep)}…${value.slice(-keep)}`
}

export function initials(firstName: string, lastName: string): string {
  return `${firstName.charAt(0)}${lastName.charAt(0)}`.toUpperCase()
}

export const NETWORK_LABELS: Record<string, string> = {
  tron: "Tron",
  ethereum: "Ethereum",
  solana: "Solana",
}

export function networkLabel(network: string): string {
  return NETWORK_LABELS[network] ?? network
}
