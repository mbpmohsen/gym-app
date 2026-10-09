// Reports (SPEC §6). The server builds every report as one table; this page
// only renders it. Excel and SMS exports download the same table from the
// server with the same filters, so they always match the screen.

import { useQuery } from '@tanstack/react-query'
import { toGregorian, toJalaali } from 'jalaali-js'
import {
  BanknoteIcon,
  CalendarX2Icon,
  ClockAlertIcon,
  DoorOpenIcon,
  FileSpreadsheetIcon,
  FlameIcon,
  HandCoinsIcon,
  MessageSquareTextIcon,
  UserXIcon,
  type LucideIcon,
} from 'lucide-react'
import { useState } from 'react'
import { Link, useSearchParams } from 'react-router'
import { toast } from 'sonner'

import { JalaliDateInput, MoneyInput } from '@/components/inputs'
import { PageHeader } from '@/components/page'
import { StatusBadge } from '@/components/StatusBadge'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { ApiError, get } from '@/lib/api'
import { faDigits, jalaliShort, num, time, toman } from '@/lib/format'
import { todayIso } from '@/lib/jalali'
import { FLAG_LABEL, KIND_LABEL, type Flag, type Plan, type Status } from '@/lib/types'
import { cn } from '@/lib/utils'

// ---------- server shapes (mirror server/src/reports.rs) ----------

type ColKind = 'text' | 'member' | 'phone' | 'int' | 'money' | 'date' | 'datetime' | 'time' | 'minutes' | 'status' | 'flags' | 'heat'
type Col = { key: string; label: string; kind: ColKind }
type Cell = string | number | null | [number, string] | Flag[]
export type ReportTable = { title: string; columns: Col[]; rows: Cell[][]; totals: { label: string; value: number; kind: ColKind }[] }

// ---------- filters ----------

type Filters = Record<string, string>
type FilterDef =
  | { type: 'range'; optional?: boolean }
  | { type: 'select'; key: string; label: string; options: [string, string][] | 'plans' }
  | { type: 'number'; key: string; label: string; suffix: string }
  | { type: 'money'; key: string; label: string }

const GENDER: FilterDef = { type: 'select', key: 'gender', label: 'جنسیت', options: [['all', 'آقا و خانم'], ['male', 'آقایان'], ['female', 'خانم‌ها']] }
const PLAN: FilterDef = { type: 'select', key: 'plan', label: 'تعرفه', options: 'plans' }

const pad = (n: number) => String(n).padStart(2, '0')
const iso = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
const daysAgo = (n: number) => iso(new Date(Date.now() - n * 86_400_000))
function jalaliMonthStart(): string {
  const n = new Date()
  const j = toJalaali(n)
  const g = toGregorian(j.jy, j.jm, 1)
  return `${g.gy}-${pad(g.gm)}-${pad(g.gd)}`
}
function weekStart(): string {
  const n = new Date()
  return daysAgo((n.getDay() + 1) % 7) // Saturday
}

type ReportDef = { id: string; label: string; icon: LucideIcon; filters: FilterDef[]; defaults: () => Filters }

const REPORTS: ReportDef[] = [
  {
    id: 'revenue',
    label: 'درآمد',
    icon: BanknoteIcon,
    filters: [
      { type: 'range' },
      { type: 'select', key: 'group', label: 'گروه‌بندی', options: [['none', 'بدون گروه‌بندی'], ['day', 'روزانه'], ['week', 'هفتگی'], ['month', 'ماهانه']] },
      PLAN,
      { type: 'select', key: 'kind', label: 'نوع', options: [['all', 'همه‌ی انواع'], ...Object.entries(KIND_LABEL)] },
      GENDER,
    ],
    defaults: () => ({ from: jalaliMonthStart(), to: todayIso(), group: 'none' }),
  },
  {
    id: 'visits',
    label: 'ورودها',
    icon: DoorOpenIcon,
    filters: [
      { type: 'range' },
      GENDER,
      { type: 'select', key: 'status', label: 'شهریه', options: [['all', 'همه‌ی وضعیت‌ها'], ['ok', 'شهریه دارد'], ['debt', 'بدهکار'], ['expired', 'پایان شهریه'], ['none', 'بدون اشتراک']] },
      { type: 'select', key: 'flag', label: 'برچسب', options: [['all', 'همه‌ی برچسب‌ها'], ...Object.entries(FLAG_LABEL).map(([k, v]) => [k, v.label] as [string, string])] },
      { type: 'select', key: 'source', label: 'منبع', options: [['all', 'همه‌ی منابع'], ['camera', 'دوربین'], ['manual', 'دستی'], ['picked', 'انتخاب از دوربین']] },
    ],
    defaults: () => ({ from: todayIso(), to: todayIso() }),
  },
  {
    id: 'expiring',
    label: 'در حال اتمام',
    icon: ClockAlertIcon,
    filters: [
      { type: 'number', key: 'sessions', label: 'حداکثر جلسه‌ی باقی‌مانده', suffix: 'جلسه' },
      { type: 'number', key: 'days', label: 'یا حداکثر روز باقی‌مانده', suffix: 'روز' },
      PLAN,
      GENDER,
    ],
    defaults: () => ({ sessions: '3', days: '7' }),
  },
  {
    id: 'expired',
    label: 'تمام‌شده‌ها',
    icon: CalendarX2Icon,
    filters: [{ type: 'range', optional: true }, { type: 'number', key: 'min_days', label: 'حداقل روز از اتمام', suffix: 'روز' }, GENDER],
    defaults: () => ({}),
  },
  {
    id: 'debtors',
    label: 'بدهکاران',
    icon: HandCoinsIcon,
    filters: [{ type: 'money', key: 'min_debt', label: 'حداقل بدهی' }, GENDER],
    defaults: () => ({}),
  },
  {
    id: 'absent',
    label: 'غایبین',
    icon: UserXIcon,
    filters: [{ type: 'number', key: 'days', label: 'حداقل روز غیبت', suffix: 'روز' }, GENDER],
    defaults: () => ({ days: '7' }),
  },
  {
    id: 'busy',
    label: 'ساعات شلوغی',
    icon: FlameIcon,
    filters: [{ type: 'range' }, GENDER],
    defaults: () => ({ from: daysAgo(29), to: todayIso() }),
  },
]

/** Excel comes from the real server; the demo has none. */
function exportProps(href: string) {
  if (!import.meta.env.VITE_DEMO) return { href, download: true }
  return {
    href: '#',
    onClick: (e: React.MouseEvent) => {
      e.preventDefault()
      toast.info('خروجی اکسل در نسخه‌ی نمایشی غیرفعال است؛ در برنامه‌ی نصب‌شده کار می‌کند.')
    },
  }
}

const query = (f: Filters) => new URLSearchParams(Object.entries(f).filter(([, v]) => v && v !== 'all')).toString()

// ---------- page ----------

export function ReportsPage() {
  const [params, setParams] = useSearchParams()
  const report = REPORTS.find((r) => r.id === params.get('r')) ?? REPORTS[0]
  return (
    <>
      <PageHeader title="گزارش‌ها" />
      <div className="grid gap-6 lg:grid-cols-[13rem_1fr]">
        <nav className="flex gap-1 overflow-x-auto lg:flex-col">
          {REPORTS.map((r) => (
            <button
              key={r.id}
              type="button"
              onClick={() => setParams({ r: r.id })}
              className={cn(
                'flex h-10 shrink-0 items-center gap-3 rounded-lg px-3 text-sm font-medium transition-colors',
                r.id === report.id ? 'bg-secondary text-secondary-foreground' : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground',
              )}
            >
              <r.icon className="size-4" />
              {r.label}
            </button>
          ))}
        </nav>
        {/* key: fresh filters per report */}
        <ReportView key={report.id} def={report} />
      </div>
    </>
  )
}

function ReportView({ def }: { def: ReportDef }) {
  const [filters, setFilters] = useState<Filters>(def.defaults)
  const qs = query(filters)
  const q = useQuery({ queryKey: ['report', def.id, qs], queryFn: () => get<ReportTable>(`/reports/${def.id}?${qs}`), placeholderData: (p) => p })
  const set = (k: string, v: string) => setFilters((f) => ({ ...f, [k]: v }))
  const t = q.data
  const hasPhone = t?.columns.some((c) => c.kind === 'phone')

  return (
    <div className="grid min-w-0 content-start gap-4">
      <Card className="gap-0 p-4">
        <div className="flex flex-wrap items-end gap-4">
          {def.filters.map((f, i) => (
            <FilterInput key={i} def={f} filters={filters} set={set} />
          ))}
        </div>
      </Card>

      <div className="flex flex-wrap items-center gap-2">
        {t?.totals.map((x) => (
          <div key={x.label} className="bg-card flex h-9 items-center gap-2 rounded-md border px-3 text-sm">
            <span className="text-muted-foreground">{x.label}</span>
            <span className="font-bold tabular-nums">{x.kind === 'money' ? toman(x.value) : num(x.value)}</span>
          </div>
        ))}
        <div className="ms-auto flex gap-2">
          {hasPhone && (
            <Button variant="outline" asChild>
              <a {...exportProps(`/api/reports/${def.id}.xlsx?${qs}${qs ? '&' : ''}sms=1`)}>
                <MessageSquareTextIcon />
                خروجی پیامک
              </a>
            </Button>
          )}
          <Button variant="outline" asChild>
            <a {...exportProps(`/api/reports/${def.id}.xlsx?${qs}`)}>
              <FileSpreadsheetIcon />
              خروجی اکسل
            </a>
          </Button>
        </div>
      </div>

      {q.isError && (
        <Alert variant="destructive">
          <AlertTitle>{q.error instanceof ApiError ? q.error.message : String(q.error)}</AlertTitle>
        </Alert>
      )}

      {t && (def.id === 'busy' ? <Heatmap t={t} /> : <ReportGrid t={t} loading={q.isFetching} />)}
    </div>
  )
}

function FilterInput({ def, filters, set }: { def: FilterDef; filters: Filters; set: (k: string, v: string) => void }) {
  const plans = useQuery({ queryKey: ['plans', 'all'], queryFn: () => get<Plan[]>('/plans?all=true'), enabled: def.type === 'select' && def.options === 'plans' })

  if (def.type === 'range') {
    const presets: [string, () => [string, string]][] = [
      ['امروز', () => [todayIso(), todayIso()]],
      ['این هفته', () => [weekStart(), todayIso()]],
      ['این ماه', () => [jalaliMonthStart(), todayIso()]],
      ['۳۰ روز اخیر', () => [daysAgo(29), todayIso()]],
    ]
    return (
      <div className="grid gap-2">
        <Label>بازه{def.optional && <span className="text-muted-foreground font-normal"> (اختیاری)</span>}</Label>
        <div className="flex flex-wrap items-center gap-2">
          <div className="w-32">
            <JalaliDateInput value={filters.from || null} onChange={(v) => set('from', v ?? '')} />
          </div>
          <span className="text-muted-foreground text-sm">تا</span>
          <div className="w-32">
            <JalaliDateInput value={filters.to || null} onChange={(v) => set('to', v ?? '')} />
          </div>
          {presets.map(([label, f]) => {
            const [a, b] = f()
            return (
              <Button
                key={label}
                type="button"
                size="sm"
                variant={filters.from === a && filters.to === b ? 'secondary' : 'ghost'}
                onClick={() => (set('from', a), set('to', b))}
              >
                {label}
              </Button>
            )
          })}
        </div>
      </div>
    )
  }
  if (def.type === 'select') {
    const options: [string, string][] =
      def.options === 'plans' ? [['all', 'همه‌ی تعرفه‌ها'], ...(plans.data ?? []).map((p) => [String(p.id), p.name] as [string, string])] : def.options
    return (
      <div className="grid gap-2">
        <Label>{def.label}</Label>
        <Select value={filters[def.key] || options[0][0]} onValueChange={(v) => set(def.key, v)}>
          <SelectTrigger className="min-w-36">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {options.map(([v, l]) => (
              <SelectItem key={v} value={v}>
                {l}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    )
  }
  if (def.type === 'money') {
    return (
      <div className="grid w-48 gap-2">
        <Label>{def.label}</Label>
        <MoneyInput value={Number(filters[def.key] || 0)} onChange={(n) => set(def.key, n ? String(n) : '')} />
      </div>
    )
  }
  return (
    <div className="grid gap-2">
      <Label>{def.label}</Label>
      <div className="relative w-36">
        <Input
          dir="ltr"
          inputMode="numeric"
          className="pe-14 text-end tabular-nums"
          value={faDigits(filters[def.key] ?? '')}
          onChange={(e) => set(def.key, e.target.value.replace(/[۰-۹]/g, (d) => String(d.charCodeAt(0) - 0x06f0)).replace(/\D/g, ''))}
        />
        <span className="text-muted-foreground pointer-events-none absolute inset-y-0 end-3 flex items-center text-sm">{def.suffix}</span>
      </div>
    </div>
  )
}

function CellView({ kind, v }: { kind: ColKind; v: Cell }) {
  if (v === null || v === undefined || v === '') return <span className="text-muted-foreground">—</span>
  switch (kind) {
    case 'member': {
      const [id, name] = v as [number, string]
      return (
        <Link to={`/members/${id}`} className="font-medium hover:underline">
          {name}
        </Link>
      )
    }
    case 'phone':
      return <span dir="ltr">{faDigits(String(v))}</span>
    case 'int':
      return <>{num(v as number)}</>
    case 'money':
      return <>{toman(v as number)}</>
    case 'date':
      return <>{jalaliShort(v as string)}</>
    case 'datetime':
      return (
        <>
          {jalaliShort(v as string)} <span className="text-muted-foreground">{time(v as string)}</span>
        </>
      )
    case 'time':
      return <>{time(v as string)}</>
    case 'minutes': {
      const m = v as number
      return <>{m >= 60 ? `${num(Math.floor(m / 60))} ساعت ${m % 60 ? `و ${num(m % 60)} دقیقه` : ''}` : `${num(m)} دقیقه`}</>
    }
    case 'status':
      return <StatusBadge status={v as Status} />
    case 'flags':
      return (
        <span className="flex flex-wrap gap-1">
          {(v as Flag[]).map((f) => (
            <Badge key={f} variant={FLAG_LABEL[f].variant}>
              {FLAG_LABEL[f].label}
            </Badge>
          ))}
        </span>
      )
    default:
      return <>{faDigits(String(v))}</>
  }
}

const RIGHT_ALIGNED: ColKind[] = ['money', 'int', 'minutes']

function ReportGrid({ t, loading }: { t: ReportTable; loading: boolean }) {
  const shown = t.rows.slice(0, 500)
  return (
    <Card className={cn('py-2 transition-opacity', loading && 'opacity-60')}>
      {t.rows.length === 0 ? (
        <p className="text-muted-foreground py-14 text-center">موردی با این فیلترها پیدا نشد.</p>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                {t.columns.map((c) => (
                  <TableHead key={c.key} className={cn(RIGHT_ALIGNED.includes(c.kind) && 'text-end')}>
                    {c.label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {shown.map((row, i) => (
                <TableRow key={i}>
                  {t.columns.map((c, j) => (
                    <TableCell key={c.key} className={cn('tabular-nums', RIGHT_ALIGNED.includes(c.kind) && 'text-end')}>
                      <CellView kind={c.kind} v={row[j]} />
                    </TableCell>
                  ))}
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {t.rows.length > shown.length && (
            <p className="text-muted-foreground border-t px-4 pt-3 pb-1 text-sm">
              {num(shown.length)} ردیف از {num(t.rows.length)} نمایش داده شده؛ همه در خروجی اکسل هستند.
            </p>
          )}
        </>
      )}
    </Card>
  )
}

/** Weekday × hour, average entries; darker = busier. */
export function Heatmap({ t }: { t: ReportTable }) {
  const values = t.rows.flatMap((r) => r.slice(1) as number[])
  const max = Math.max(...values, 0)
  return (
    <Card className="overflow-x-auto p-4">
      {max === 0 ? (
        <p className="text-muted-foreground py-10 text-center">در این بازه ورودی ثبت نشده.</p>
      ) : (
        <div className="grid min-w-[40rem] gap-1" style={{ gridTemplateColumns: `5rem repeat(${t.columns.length - 1}, minmax(0, 1fr))` }}>
          <span />
          {t.columns.slice(1).map((c) => (
            <span key={c.key} className="text-muted-foreground text-center text-xs">
              {c.label}
            </span>
          ))}
          {t.rows.map((r) => (
            <div key={String(r[0])} className="contents">
              <span className="flex items-center text-sm">{String(r[0])}</span>
              {(r.slice(1) as number[]).map((v, i) => (
                <span
                  key={i}
                  title={`${String(r[0])} ساعت ${t.columns[i + 1].label}: میانگین ${num(v)} ورود`}
                  className="flex aspect-[4/3] items-center justify-center rounded text-[10px] font-medium tabular-nums"
                  style={{
                    background: `color-mix(in oklch, var(--primary) ${v === 0 ? 6 : Math.round(18 + 82 * (v / max))}%, transparent)`,
                    color: v / max > 0.55 ? 'var(--primary-foreground)' : 'var(--foreground)',
                  }}
                >
                  {v ? num(v) : ''}
                </span>
              ))}
            </div>
          ))}
        </div>
      )}
    </Card>
  )
}
