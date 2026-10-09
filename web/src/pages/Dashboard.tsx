// Dashboard (SPEC §6): today at a glance, last 30 days, busy hours, warnings.

import { useQuery } from '@tanstack/react-query'
import {
  BanknoteIcon,
  CameraOffIcon,
  ClockAlertIcon,
  DoorOpenIcon,
  HandCoinsIcon,
  ScanFaceIcon,
  UsersIcon,
  WalletIcon,
  type LucideIcon,
} from 'lucide-react'
import { motion } from 'motion/react'
import { useState } from 'react'
import { Link } from 'react-router'

import { PageHeader } from '@/components/page'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { get } from '@/lib/api'
import { jalaliShort, num, toman, weekday } from '@/lib/format'
import { cn } from '@/lib/utils'
import { Heatmap, type ReportTable } from './Reports'

type Dash = {
  inside: number
  entries_today: number
  revenue_today: number
  revenue_month: number
  active_members: number
  expiring_week: number
  total_debt: number
  no_face: number
  days: [string, number, number][]
  busy: ReportTable
}
type FaceHealth = { reachable: boolean; camera?: { connected: boolean }; events?: boolean }

export function DashboardPage() {
  const q = useQuery({ queryKey: ['dashboard'], queryFn: () => get<Dash>('/dashboard'), refetchInterval: 60_000 })
  const health = useQuery({ queryKey: ['face-health'], queryFn: () => get<FaceHealth>('/face/health'), refetchInterval: 10_000 })
  const d = q.data
  const h = health.data
  const faceProblem = h && (!h.reachable ? 'سرویس تشخیص چهره اجرا نیست' : !h.camera?.connected ? 'دوربین وصل نیست' : h.events === false ? 'رویدادهای دوربین دریافت نمی‌شود' : null)

  return (
    <>
      <PageHeader title="داشبورد" />
      <div className="grid gap-6">
        {(faceProblem || (d && d.no_face > 0)) && (
          <div className="grid gap-3">
            {faceProblem && (
              <Alert variant="destructive">
                <CameraOffIcon />
                <AlertTitle>{faceProblem}</AlertTitle>
                <AlertDescription>ورود و خروج خودکار ثبت نمی‌شود.</AlertDescription>
              </Alert>
            )}
            {d && d.no_face > 0 && (
              <Alert variant="warning">
                <ScanFaceIcon />
                <AlertTitle>{num(d.no_face)} عضو چهره‌ی ثبت‌شده ندارند</AlertTitle>
                <AlertDescription>
                  <Link to="/members" className="underline">
                    ورودشان خودکار تشخیص داده نمی‌شود؛ از صفحه‌ی اعضا ثبت کنید.
                  </Link>
                </AlertDescription>
              </Alert>
            )}
          </div>
        )}

        <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
          <Stat icon={DoorOpenIcon} i={0} label="الان داخل" value={d && num(d.inside)} to="/" />
          <Stat icon={UsersIcon} i={1} label="ورود امروز" value={d && num(d.entries_today)} to="/reports?r=visits" />
          <Stat icon={BanknoteIcon} i={2} label="درآمد امروز" value={d && toman(d.revenue_today)} to="/reports?r=revenue" />
          <Stat icon={WalletIcon} i={3} label="درآمد این ماه" value={d && toman(d.revenue_month)} to="/reports?r=revenue" />
          <Stat icon={UsersIcon} i={4} label="اعضای فعال" value={d && num(d.active_members)} to="/members" />
          <Stat icon={ClockAlertIcon} i={5} label="در حال اتمام" hint="حداکثر ۳ جلسه یا ۷ روز مانده" value={d && num(d.expiring_week)} to="/reports?r=expiring" />
          <Stat icon={HandCoinsIcon} i={6} label="بدهی کل" value={d && toman(d.total_debt)} to="/reports?r=debtors" danger={!!d?.total_debt} />
        </div>

        {d && (
          <div className="grid gap-6 xl:grid-cols-2">
            <ChartCard title="درآمد ۳۰ روز اخیر">
              <Bars days={d.days} pick={(x) => x[1]} format={toman} />
            </ChartCard>
            <ChartCard title="ورود روزانه‌ی ۳۰ روز اخیر">
              <Bars days={d.days} pick={(x) => x[2]} format={(n) => `${num(n)} نفر`} />
            </ChartCard>
          </div>
        )}

        {d && (
          <div>
            <h2 className="mb-3 font-semibold">ساعات شلوغی (میانگین ورود، ۳۰ روز اخیر)</h2>
            <Heatmap t={d.busy} />
          </div>
        )}
      </div>
    </>
  )
}

function Stat({ icon: Icon, label, value, hint, to, danger, i }: { icon: LucideIcon; label: string; value?: string; hint?: string; to: string; danger?: boolean; i: number }) {
  return (
    <MotionLink
      to={to}
      className="group"
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: i * 0.04, type: 'spring', stiffness: 380, damping: 30 }}
    >
      <Card className="group-hover:border-primary/40 h-full gap-2 p-5 transition-colors">
        <div className="text-muted-foreground flex items-center gap-2 text-sm">
          <Icon className="size-4" />
          {label}
        </div>
        <div className={cn('text-2xl font-bold tracking-tight tabular-nums', danger && 'text-destructive')}>{value ?? '…'}</div>
        {hint && <div className="text-muted-foreground text-xs">{hint}</div>}
      </Card>
    </MotionLink>
  )
}

const MotionLink = motion.create(Link)

function ChartCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-base">{title}</CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  )
}

/** One series, one bar per day (newest on the left in RTL reading order: oldest first = right). */
function Bars({ days, pick, format }: { days: Dash['days']; pick: (d: Dash['days'][number]) => number; format: (n: number) => string }) {
  const [hover, setHover] = useState<number | null>(null)
  const values = days.map(pick)
  const max = Math.max(...values, 0)
  const total = values.reduce((a, b) => a + b, 0)
  if (max === 0) return <p className="text-muted-foreground py-16 text-center text-sm">در این ۳۰ روز چیزی ثبت نشده.</p>
  const h = hover ?? values.length - 1
  return (
    <div>
      <div className="mb-3 flex items-baseline justify-between gap-2 text-sm">
        <span>
          <span className="text-muted-foreground">
            {weekday(days[h][0])} {jalaliShort(days[h][0])}:{' '}
          </span>
          <span className="font-semibold tabular-nums">{format(values[h])}</span>
        </span>
        <span className="text-muted-foreground tabular-nums">جمع: {format(total)}</span>
      </div>
      <div className="relative h-44" onMouseLeave={() => setHover(null)}>
        {[0.5, 1].map((g) => (
          <div key={g} className="border-border/60 absolute inset-x-0 border-t border-dashed" style={{ bottom: `${g * 100}%` }} />
        ))}
        <div className="border-border absolute inset-x-0 bottom-0 border-t" />
        <div className="absolute inset-0 flex items-end gap-[2px]">
          {values.map((v, i) => (
            <div key={days[i][0]} className="flex h-full flex-1 items-end" onMouseEnter={() => setHover(i)}>
              <motion.div
                className={cn('bg-primary w-full origin-bottom rounded-t-[4px] transition-opacity', hover !== null && hover !== i && 'opacity-50')}
                style={{ height: v ? `max(2px, ${(v / max) * 100}%)` : 0 }}
                initial={{ scaleY: 0 }}
                animate={{ scaleY: 1 }}
                transition={{ delay: i * 0.012, duration: 0.45, ease: [0.22, 1, 0.36, 1] }}
              />
            </div>
          ))}
        </div>
      </div>
      <div className="text-muted-foreground mt-2 flex justify-between text-xs tabular-nums">
        <span>{jalaliShort(days[0][0])}</span>
        <span>{jalaliShort(days[days.length - 1][0])}</span>
      </div>
    </div>
  )
}
