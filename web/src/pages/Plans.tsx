import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { PencilIcon, PlusIcon } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'

import { MoneyInput } from '@/components/inputs'
import { Field, PageHeader } from '@/components/page'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Switch } from '@/components/ui/switch'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { ApiError, get, post, put } from '@/lib/api'
import { num, toman } from '@/lib/format'
import { FREQ_LABEL, KIND_LABEL, type Frequency, type Kind, type Plan } from '@/lib/types'
import { cn } from '@/lib/utils'

export function planSummary(p: Pick<Plan, 'kind' | 'sessions' | 'duration_days'>): string {
  const s = p.sessions ? `${num(p.sessions)} جلسه` : ''
  const d = p.duration_days ? `${num(p.duration_days)} روز` : ''
  return p.kind === 'combined' ? `${s} در ${d}` : s || d
}

export function PlansPage() {
  const q = useQuery({ queryKey: ['plans', 'all'], queryFn: () => get<Plan[]>('/plans?all=true') })
  const [editing, setEditing] = useState<Plan | 'new' | null>(null)

  return (
    <>
      <PageHeader title="تعرفه‌ها" description="قیمت هر تعرفه پیش‌فرض است و موقع فروش اشتراک قابل تغییر است.">
        <Button onClick={() => setEditing('new')}>
          <PlusIcon />
          تعرفه‌ی جدید
        </Button>
      </PageHeader>

      {q.data && q.data.length === 0 ? (
        <Card className="text-muted-foreground items-center py-16 text-center">
          هنوز تعرفه‌ای تعریف نشده. اول تعرفه‌ها را بسازید تا بتوانید اشتراک بفروشید.
          <Button onClick={() => setEditing('new')}>
            <PlusIcon />
            تعرفه‌ی جدید
          </Button>
        </Card>
      ) : (
        <Card className="py-2">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>نام</TableHead>
                <TableHead>نوع</TableHead>
                <TableHead>تواتر</TableHead>
                <TableHead>امکانات</TableHead>
                <TableHead>قیمت</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {q.data?.map((p) => (
                <TableRow key={p.id} className={cn(!p.active && 'opacity-55')}>
                  <TableCell className="font-medium">
                    {p.name}
                    {!p.active && (
                      <Badge variant="secondary" className="ms-2">
                        غیرفعال
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell>
                    {KIND_LABEL[p.kind]} <span className="text-muted-foreground">· {planSummary(p)}</span>
                  </TableCell>
                  <TableCell>{FREQ_LABEL[p.frequency]}</TableCell>
                  <TableCell className="space-x-1 space-x-reverse">
                    {p.shower && <Badge variant="outline">دوش</Badge>}
                    {p.locker && <Badge variant="outline">کمد</Badge>}
                    {!p.shower && !p.locker && <span className="text-muted-foreground">—</span>}
                  </TableCell>
                  <TableCell>{toman(p.price)}</TableCell>
                  <TableCell className="text-end">
                    <Button variant="ghost" size="icon-sm" aria-label="ویرایش" onClick={() => setEditing(p)}>
                      <PencilIcon />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Card>
      )}

      <Dialog open={editing !== null} onOpenChange={(o) => !o && setEditing(null)}>
        {editing !== null && <PlanForm plan={editing === 'new' ? null : editing} onDone={() => setEditing(null)} />}
      </Dialog>
    </>
  )
}

type Draft = Omit<Plan, 'id'>
const EMPTY: Draft = { name: '', kind: 'sessions', sessions: 12, duration_days: 30, frequency: 'six_days', shower: false, locker: false, price: 0, active: true }

function PlanForm({ plan, onDone }: { plan: Plan | null; onDone: () => void }) {
  const qc = useQueryClient()
  const [d, setD] = useState<Draft>(plan ? { ...plan, sessions: plan.sessions ?? 12, duration_days: plan.duration_days ?? 30 } : EMPTY)
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => setD((p) => ({ ...p, [k]: v }))
  const save = useMutation({
    mutationFn: () => (plan ? put<Plan>(`/plans/${plan.id}`, d) : post<Plan>('/plans', d)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['plans'] })
      toast.success(plan ? 'تعرفه ذخیره شد' : 'تعرفه ساخته شد')
      onDone()
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  })
  const hasSessions = d.kind !== 'duration'
  const hasDuration = d.kind !== 'sessions'

  return (
    <DialogContent className="sm:max-w-xl">
      <DialogHeader>
        <DialogTitle>{plan ? 'ویرایش تعرفه' : 'تعرفه‌ی جدید'}</DialogTitle>
      </DialogHeader>
      <form
        className="grid gap-5"
        onSubmit={(e: FormEvent) => {
          e.preventDefault()
          save.mutate()
        }}
      >
        <Field label="نام" htmlFor="pname" hint="همان چیزی که موقع فروش انتخاب می‌کنید؛ مثلاً «۱۲ جلسه، با دوش»">
          <Input id="pname" value={d.name} onChange={(e) => set('name', e.target.value)} autoFocus />
        </Field>

        <div className="grid gap-3">
          <Label>نوع</Label>
          <RadioGroup value={d.kind} onValueChange={(v) => set('kind', v as Kind)} className="flex flex-wrap gap-5">
            {(Object.keys(KIND_LABEL) as Kind[]).map((k) => (
              <div key={k} className="flex items-center gap-2">
                <RadioGroupItem value={k} id={`kind-${k}`} />
                <Label htmlFor={`kind-${k}`} className="font-normal">
                  {KIND_LABEL[k]}
                </Label>
              </div>
            ))}
          </RadioGroup>
        </div>

        <div className="grid grid-cols-2 gap-4">
          {hasSessions && (
            <Field label="تعداد جلسات" htmlFor="psess">
              <Input id="psess" type="number" min={1} value={d.sessions ?? ''} onChange={(e) => set('sessions', Number(e.target.value))} />
            </Field>
          )}
          {hasDuration && (
            <Field label="مدت (روز)" htmlFor="pdays" hint={d.kind === 'combined' ? 'جلسات باید در این مدت استفاده شوند' : undefined}>
              <Input id="pdays" type="number" min={1} value={d.duration_days ?? ''} onChange={(e) => set('duration_days', Number(e.target.value))} />
            </Field>
          )}
        </div>

        <div className="grid gap-3">
          <Label>تواتر</Label>
          <RadioGroup value={d.frequency} onValueChange={(v) => set('frequency', v as Frequency)} className="flex flex-wrap gap-5">
            {(Object.keys(FREQ_LABEL) as Frequency[]).map((f) => (
              <div key={f} className="flex items-center gap-2">
                <RadioGroupItem value={f} id={`freq-${f}`} />
                <Label htmlFor={`freq-${f}`} className="font-normal">
                  {FREQ_LABEL[f]}
                </Label>
              </div>
            ))}
          </RadioGroup>
        </div>

        <div className="flex flex-wrap gap-6">
          <div className="flex items-center gap-2">
            <Switch id="shower" checked={d.shower} onCheckedChange={(v) => set('shower', v)} />
            <Label htmlFor="shower" className="font-normal">
              با دوش
            </Label>
          </div>
          <div className="flex items-center gap-2">
            <Switch id="locker" checked={d.locker} onCheckedChange={(v) => set('locker', v)} />
            <Label htmlFor="locker" className="font-normal">
              با کمد
            </Label>
          </div>
          {plan && (
            <div className="flex items-center gap-2">
              <Switch id="active" checked={d.active} onCheckedChange={(v) => set('active', v)} />
              <Label htmlFor="active" className="font-normal">
                فعال برای فروش
              </Label>
            </div>
          )}
        </div>

        <Field label="قیمت" htmlFor="pprice">
          <MoneyInput id="pprice" value={d.price} onChange={(v) => set('price', v)} />
        </Field>

        {plan && <p className="text-muted-foreground text-sm">تغییر تعرفه روی اشتراک‌هایی که قبلاً فروخته شده‌اند اثری ندارد.</p>}

        <DialogFooter>
          <Button type="submit" disabled={save.isPending}>
            {plan ? 'ذخیره' : 'ساخت تعرفه'}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
