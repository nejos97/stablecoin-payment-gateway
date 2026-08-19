import { useLocation } from "react-router-dom"

import { Separator } from "@/components/ui/separator"
import { SidebarTrigger } from "@/components/ui/sidebar"

const TITLES: Array<[string, string]> = [
  ["/payments", "Payments"],
  ["/deliveries", "Deliveries"],
  ["/webhooks", "Webhooks"],
  ["/wallets", "Wallets"],
  ["/api-keys", "API Keys"],
  ["/staff", "Staff"],
  ["/settings", "Settings"],
]

export function SiteHeader() {
  const location = useLocation()
  const title =
    TITLES.find(([prefix]) => location.pathname.startsWith(prefix))?.[1] ?? "Overview"

  return (
    <header className="flex h-(--header-height) shrink-0 items-center gap-2 border-b transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-(--header-height)">
      <div className="flex w-full items-center gap-1 px-4 lg:gap-2 lg:px-6">
        <SidebarTrigger className="-ml-1" />
        <Separator
          orientation="vertical"
          className="mx-2 data-[orientation=vertical]:h-4"
        />
        <h1 className="text-base font-medium">{title}</h1>
      </div>
    </header>
  )
}
