import '@fontsource/vazirmatn/400.css'
import '@fontsource/vazirmatn/500.css'
import '@fontsource/vazirmatn/700.css'
import './index.css'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MotionConfig } from 'motion/react'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Direction } from 'radix-ui'
import { BrowserRouter, Route, Routes } from 'react-router'

import { AuthGate } from '@/components/AuthGate'
import { Layout } from '@/components/Layout'
import { ApiError } from '@/lib/api'
import { Toaster } from '@/components/ui/sonner'
import '@/lib/theme'
import { MemberPage } from '@/pages/members/MemberPage'
import { MembersList } from '@/pages/members/MembersList'
import { PlansPage } from '@/pages/Plans'
import { DashboardPage } from '@/pages/Dashboard'
import { ReceptionPage } from '@/pages/Reception'
import { ReportsPage } from '@/pages/Reports'
import { ShiftsPage } from '@/pages/Shifts'
import { SettingsPage } from '@/pages/Settings'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // a 401 means the session ended (e.g. server restart): back to the login screen
      retry: (n, e) => !(e instanceof ApiError && e.status === 401) && n < 2,
      refetchOnWindowFocus: false,
    },
  },
})
queryClient.getQueryCache().subscribe((ev) => {
  const err = ev.query.state.error
  if (err instanceof ApiError && err.status === 401 && ev.query.queryKey[0] !== 'auth') {
    void queryClient.invalidateQueries({ queryKey: ['auth'] })
  }
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    {/* honors the OS "reduce motion" setting everywhere */}
    <MotionConfig reducedMotion="user">
    <Direction.Provider dir="rtl">
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AuthGate>
          <Routes>
            <Route element={<Layout />}>
              <Route index element={<ReceptionPage />} />
              <Route path="members" element={<MembersList />} />
              <Route path="members/:id" element={<MemberPage />} />
              <Route path="plans" element={<PlansPage />} />
              <Route path="shifts" element={<ShiftsPage />} />
              <Route path="reports" element={<ReportsPage />} />
              <Route path="dashboard" element={<DashboardPage />} />
              <Route path="settings" element={<SettingsPage />} />
              <Route path="*" element={<p className="text-muted-foreground">این صفحه وجود ندارد.</p>} />
            </Route>
          </Routes>
        </AuthGate>
      </BrowserRouter>
      <Toaster />
    </QueryClientProvider>
    </Direction.Provider>
    </MotionConfig>
  </StrictMode>,
)
