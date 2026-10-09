// Weekly shifts (SPEC §3 `shifts`): a timeline per weekday, men's and women's
// shifts in two fixed colors. Click a shift to edit it.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { CalendarClockIcon, PlusIcon, Trash2Icon } from 'lucide-react'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'

import { Field, PageHeader } from '@/components/page'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { ApiError, api, get, post, put } from '@/lib/api'
import { faDigits } from '@/lib/format'
import type { Gender } from '@/lib/types'
import { cn } from '@/lib/utils'

export type Shift = { id: number; weekday: number; start_time: string; end_time: string; gender: Gender }

export const DAYS = ['شنبه', 'یکشنبه', 'دوشنبه', 'سه‌شنبه', 'چهارشنبه', 'پنجشنبه', 'جمعه']
const PRESETS = [
  { label: 'شنبه تا چهارشنبه', days: [0, 1, 2, 3, 4] },
  { label: 'شنبه تا پنجشنبه', days: [0, 1, 2, 3, 4, 5] },
  { label: 'کل هفته', days: [0, 1, 2, 3, 4, 5, 6] },
]
export const SHIFT_LABEL: Record<Gender, string> = { male: 'آقایان', female: 'خانم‌ها' }
// fixed in every theme, like the status colors
export const SHIFT_COLOR: Record<Gender, string> = {
  male: 'bg-sky-500/15 text-sky-800 ring-sky-500/40 hover:bg-sky-500/25 dark:text-sky-200',
  female: 'bg-pink-500/15 text-pink-800 ring-pink-500/40 hover:bg-pink-500/25 dark:text-pink-200',
}

const errorText = (e: unknown) => (e instanceof ApiError ? e.message : String(e))
const minutes = (t: string) => Number(t.slice(0, 2)) * 60 + Number(t.slice(3, 5))

export function ShiftsPage() {
  const q = useQuery({ queryKey: ['shifts'], queryFn: () => get<Shift[]>('/shifts') })
  const [editing, setEditing] = useState<Shift | 'new' | null>(null)
  const shifts = q.data ?? []

  // visible range: 6:00–24:00, widened to whatever is defined
  const from = Math.min(6 * 60, ...shifts.map((s) => Math.floor(minutes(s.start_time) / 60) * 60))
  const to = 24 * 60
  const span = to - from
  const hours = Array.from({ length: span / 60 + 1 }, (_, i) => from / 60 + i)

  return (
    <>
      <PageHeader title="سانس‌ها" description="ورود در سانس جنس مخالف هشدار و صدای «سانس نامعتبر» دارد.">
        <Button onClick={() => setEditing('new')}>
          <PlusIcon />
          سانس جدید
        </Button>
      </PageHeader>

      {q.data && shifts.length === 0 && (
        <Alert className="mb-6">
          <CalendarClockIcon />
          <AlertDescription>هنوز سانسی تعریف نشده؛ تا وقتی سانسی نباشد، هیچ هشدار سانسی هم داده نمی‌شود.</AlertDescription>
        </Alert>
      )}

      <Card>
        <CardContent>
          <div className="mb-5 flex gap-4 text-sm">
            {(['male', 'female'] as const).map((g) => (
              <span key={g} className="flex items-center gap-2">
                <span className={cn('size-3 rounded-sm ring-1', SHIFT_COLOR[g])} />
                {SHIFT_LABEL[g]}
              </span>
            ))}
          </div>

          <div className="grid grid-cols-[5.5rem_1fr] gap-y-2">
            {/* hour ruler */}
            <span />
            <div className="text-muted-foreground relative mb-1 h-5 text-xs tabular-nums">
              {hours.map((h) => (
                <span key={h} className="absolute -translate-x-1/2 rtl:translate-x-1/2" style={{ insetInlineStart: `${((h * 60 - from) / span) * 100}%` }}>
                  {h % 3 === 0 ? faDigits(String(h)) : ''}
                </span>
              ))}
            </div>

            {DAYS.map((day, d) => (
              <div key={d} className="contents">
                <span className="flex items-center text-sm font-medium">{day}</span>
                <div className="bg-muted/50 relative h-11 rounded-lg">
                  {hours.slice(1, -1).map((h) => (
                    <span
                      key={h}
                      className={cn('absolute inset-y-0 w-px', h % 3 === 0 ? 'bg-border' : 'bg-border/40')}
                      style={{ insetInlineStart: `${((h * 60 - from) / span) * 100}%` }}
                    />
                  ))}
                  {shifts
                    .filter((s) => s.weekday === d)
                    .map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        onClick={() => setEditing(s)}
                        title={`${SHIFT_LABEL[s.gender]} ${faDigits(s.start_time)} تا ${faDigits(s.end_time)}`}
                        className={cn('absolute inset-y-1 overflow-hidden rounded-md px-2 text-xs font-medium whitespace-nowrap ring-1 transition-colors', SHIFT_COLOR[s.gender])}
                        style={{
                          insetInlineStart: `${((minutes(s.start_time) - from) / span) * 100}%`,
                          width: `${((minutes(s.end_time) - minutes(s.start_time)) / span) * 100}%`,
                        }}
                      >
                        {SHIFT_LABEL[s.gender]} {faDigits(s.start_time)} تا {faDigits(s.end_time)}
                      </button>
                    ))}
                </div>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <Dialog open={editing !== null} onOpenChange={(o) => !o && setEditing(null)}>
        {editing && <ShiftForm shift={editing === 'new' ? null : editing} onDone={() => setEditing(null)} />}
      </Dialog>
    </>
  )
}

function ShiftForm({ shift, onDone }: { shift: Shift | null; onDone: () => void }) {
  const qc = useQueryClient()
  const [days, setDays] = useState<number[]>(shift ? [shift.weekday] : [0, 1, 2, 3, 4])
  const [start, setStart] = useState(shift?.start_time ?? '')
  const [end, setEnd] = useState(shift?.end_time ?? '')
  const [gender, setGender] = useState<Gender>(shift?.gender ?? 'male')

  const done = (rows?: Shift[]) => {
    if (rows) qc.setQueryData(['shifts'], rows)
    else void qc.invalidateQueries({ queryKey: ['shifts'] })
    void qc.invalidateQueries({ queryKey: ['shifts-current'] })
    onDone()
  }
  const save = useMutation({
    mutationFn: () => {
      const body = { weekdays: days, start_time: start, end_time: end, gender }
      return shift ? put<Shift[]>(`/shifts/${shift.id}`, body) : post<Shift[]>('/shifts', body)
    },
    onSuccess: (rows) => (toast.success('سانس ذخیره شد'), done(rows)),
    onError: (e) => toast.error(errorText(e)),
  })
  const remove = useMutation({
    mutationFn: () => api('DELETE', `/shifts/${shift!.id}`),
    onSuccess: () => (toast.success('سانس حذف شد'), done()),
    onError: (e) => toast.error(errorText(e)),
  })

  const toggle = (d: number) => setDays((cur) => (shift ? [d] : cur.includes(d) ? cur.filter((x) => x !== d) : [...cur, d].sort()))
  const submit = (e: FormEvent) => {
    e.preventDefault()
    save.mutate()
  }

  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>{shift ? 'ویرایش سانس' : 'سانس جدید'}</DialogTitle>
        <DialogDescription>{shift ? DAYS[shift.weekday] : 'برای هر روز انتخاب‌شده یک سانس ساخته می‌شود.'}</DialogDescription>
      </DialogHeader>
      <form onSubmit={submit} className="grid gap-5">
        <Field label="سانس">
          <RadioGroup dir="rtl" value={gender} onValueChange={(v) => setGender(v as Gender)} className="flex gap-6">
            {(['male', 'female'] as const).map((g) => (
              <div key={g} className="flex items-center gap-2">
                <RadioGroupItem value={g} id={`shift-${g}`} />
                <Label htmlFor={`shift-${g}`} className="font-normal">
                  {SHIFT_LABEL[g]}
                </Label>
              </div>
            ))}
          </RadioGroup>
        </Field>

        <Field label="روزها">
          {!shift && (
            <div className="flex flex-wrap gap-2">
              {PRESETS.map((p) => (
                <Button
                  key={p.label}
                  type="button"
                  size="sm"
                  variant={days.join() === p.days.join() ? 'secondary' : 'ghost'}
                  onClick={() => setDays(p.days)}
                >
                  {p.label}
                </Button>
              ))}
            </div>
          )}
          <div className="flex flex-wrap gap-1.5">
            {DAYS.map((name, d) => (
              <button
                key={d}
                type="button"
                aria-pressed={days.includes(d)}
                onClick={() => toggle(d)}
                className={cn(
                  'h-8 rounded-md border px-2.5 text-sm transition-colors',
                  days.includes(d) ? 'bg-primary text-primary-foreground border-primary' : 'hover:bg-accent',
                )}
              >
                {name}
              </button>
            ))}
          </div>
        </Field>

        <div className="grid grid-cols-2 gap-4">
          <Field label="از ساعت" htmlFor="start">
            <Input id="start" dir="ltr" inputMode="numeric" placeholder="08:00" required value={start} onChange={(e) => setStart(e.target.value)} />
          </Field>
          <Field label="تا ساعت" htmlFor="end">
            <Input id="end" dir="ltr" inputMode="numeric" placeholder="14:00" required value={end} onChange={(e) => setEnd(e.target.value)} />
          </Field>
        </div>

        <DialogFooter className="sm:justify-between">
          {shift ? (
            <Button type="button" variant="ghost" className="text-destructive" disabled={remove.isPending} onClick={() => remove.mutate()}>
              <Trash2Icon />
              حذف
            </Button>
          ) : (
            <span />
          )}
          <Button type="submit" disabled={save.isPending || days.length === 0}>
            ذخیره
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
