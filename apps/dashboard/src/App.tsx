import { Navigate, Route, Routes } from "react-router-dom"

import { AppLayout } from "@/components/layout/AppLayout"
import { RequireAdmin, RequireAuth } from "@/lib/auth"
import { AccountSecurityPage } from "@/pages/AccountSecurityPage"
import { ApiKeysPage } from "@/pages/ApiKeysPage"
import { DeliveriesPage } from "@/pages/DeliveriesPage"
import { LoginPage } from "@/pages/LoginPage"
import { OverviewPage } from "@/pages/OverviewPage"
import { PaymentDetailPage } from "@/pages/PaymentDetailPage"
import { PaymentsPage } from "@/pages/PaymentsPage"
import { SettingsPage } from "@/pages/SettingsPage"
import { SetupPage } from "@/pages/SetupPage"
import { StaffPage } from "@/pages/StaffPage"
import { WalletsPage } from "@/pages/WalletsPage"
import { WebhooksPage } from "@/pages/WebhooksPage"

export default function App() {
  return (
    <Routes>
      <Route path="/setup" element={<SetupPage />} />
      <Route path="/login" element={<LoginPage />} />
      <Route element={<RequireAuth />}>
        <Route element={<AppLayout />}>
          <Route path="/" element={<OverviewPage />} />
          <Route path="/payments" element={<PaymentsPage />} />
          <Route path="/payments/:id" element={<PaymentDetailPage />} />
          <Route path="/deliveries" element={<DeliveriesPage />} />
          <Route path="/wallets" element={<WalletsPage />} />
          <Route path="/account/2fa" element={<AccountSecurityPage />} />
          <Route element={<RequireAdmin />}>
            <Route path="/api-keys" element={<ApiKeysPage />} />
            <Route path="/webhooks" element={<WebhooksPage />} />
            <Route path="/staff" element={<StaffPage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Route>
        </Route>
      </Route>
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
