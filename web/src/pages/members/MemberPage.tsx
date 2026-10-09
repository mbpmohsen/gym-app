import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { ArchiveIcon, ArchiveRestoreIcon, ArrowRightIcon, BanknoteIcon, HandIcon, LogInIcon, LogOutIcon, PencilIcon, PlusIcon, ScanFaceIcon, TimerOffIcon, Trash2Icon } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { Link, useParams, useSearchParams } from 'react-router'
import { toast } from 'sonner'

import { JalaliDateInput, MoneyInput } from '@/components/inputs'
import { Field } from '@/components/page'
import { StatusBadge } from '@/components/StatusBadge'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { ApiError, api, get, post } from '@/lib/api'
import { faDigits, jalaliDate, jalaliShort, num, time, toman } from '@/lib/format'
import { FLAG_LABEL, FREQ_LABEL, GENDER_LABEL, type MemberDetail, type Plan, type Subscription, type Visit } from '@/lib/types'
import { cn } from '@/lib/utils'
import { planSummary } from '@/pages/Plans'
import { cancelFaceEnroll, FaceEnrollDialog } from './FaceEnroll'
import { MemberForm } from './MemberForm'

const errorText = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

export function MemberPage() {
  const id = Number(useParams().id)
  const qc = useQueryClient()
  const q = useQuery({ queryKey: ['member', id], queryFn: () => get<MemberDetail>(`/members/${id}`) })
  const [dialog, setDialog] = useState<'edit' | 'sell' | 'face' | { pay: Subscription } | null>(null)
  const [params, setParams] = useSearchParams()
  const wizard = params.get('step') === 'face' // just created: face, then first subscription

  useEffect(() => {
    if (wizard && q.data) setDialog('face')
  }, [wizard, q.data?.id])

  function closeDialog() {
    if (dialog === 'face') void cancelFaceEnroll()
    if (wizard) setParams({}, { replace: true })
    setDialog(null)
  }

  const removeFace = useMutation({
    mutationFn: () => api('DELETE', `/members/${id}/face`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['member', id] })
      void qc.invalidateQueries({ queryKey: ['members'] })
      toast.success('چهره حذف شد')
    },
    onError: (e) => toast.error(errorText(e)),
  })

  const visits = useQuery({ queryKey: ['visits', id], queryFn: () => get<Visit[]>(`/members/${id}/visits`) })
  const inside = visits.data?.some((v) => v.exited_at === null) ?? false
  const door = useMutation({
    mutationFn: () => post(`/members/${id}/${inside ? 'exit' : 'enter'}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ['visits', id] })
      void qc.invalidateQueries({ queryKey: ['member', id] })
      void qc.invalidateQueries({ queryKey: ['reception'] })
      toast.success(inside ? 'خروج ثبت شد' : 'ورود ثبت شد')
    },
    onError: (e) => toast.error(errorText(e)),
  })

  const setMember = (m: MemberDetail) => {
    qc.setQueryData(['member', id], m)
    void qc.invalidateQueries({ queryKey: ['members'] })
  }
  const archive = useMutation({
    mutationFn: (archived: boolean) => post<MemberDetail>(`/members/${id}/archive`, { archived }),
    onSuccess: (m) => {
      setMember(m)
      toast.success(m.archived ? 'عضو بایگانی شد' : 'عضو از بایگانی خارج شد')
    },
    onError: (e) => toast.error(errorText(e)),
  })

  if (q.isError) {
    return (
      <Alert variant="destructive">
        <AlertTitle>{errorText(q.error)}</AlertTitle>
      </Alert>
    )
  }
  const m = q.data
  if (!m) return null

  return (
    <>
      <Link to="/members" className="text-muted-foreground hover:text-foreground mb-4 inline-flex items-center gap-1 text-sm">
        <ArrowRightIcon className="size-4" />
        اعضا
      </Link>

      <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-bold tracking-tight">{m.full_name}</h1>
            <StatusBadge status={m.status} />
            {m.archived && <Badge variant="secondary">بایگانی‌شده</Badge>}
          </div>
          <p className="text-muted-foreground mt-1.5 text-sm">
            {GENDER_LABEL[m.gender]} · <span dir="ltr">{faDigits(m.phone)}</span> · متولد {jalaliDate(m.birth_date)}
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          {!m.archived && visits.data && (
            <Button variant="outline" disabled={door.isPending} onClick={() => door.mutate()}>
              {inside ? <LogOutIcon /> : <LogInIcon />}
              {inside ? 'ثبت خروج' : 'ثبت ورود'}
            </Button>
          )}
          <Button variant="outline" onClick={() => setDialog('edit')}>
            <PencilIcon />
            ویرایش
          </Button>
          <Button variant="outline" disabled={archive.isPending} onClick={() => archive.mutate(!m.archived)}>
            {m.archived ? <ArchiveRestoreIcon /> : <ArchiveIcon />}
            {m.archived ? 'خروج از بایگانی' : 'بایگانی'}
          </Button>
          {!m.archived && (
            <Button onClick={() => setDialog('sell')}>
              <PlusIcon />
              فروش اشتراک
            </Button>
          )}
        </div>
      </header>

      <div className="grid gap-6">
        {!m.face_enrolled && !m.archived && (
          <Alert variant="warning">
            <ScanFaceIcon />
            <AlertTitle>چهره ثبت نشده</AlertTitle>
            <AlertDescription>
              تا چهره ثبت نشود، ورود این عضو خودکار تشخیص داده نمی‌شود.
              <Button size="sm" className="mt-2" onClick={() => setDialog('face')}>
                <ScanFaceIcon />
                ثبت چهره
              </Button>
            </AlertDescription>
          </Alert>
        )}

        <Card>
          <CardHeader>
            <CardTitle>اشتراک‌ها</CardTitle>
          </CardHeader>
          <CardContent className="px-2">
            {m.subscriptions.length === 0 ? (
              <p className="text-muted-foreground px-4 py-6 text-sm">هنوز اشتراکی فروخته نشده.</p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>تعرفه</TableHead>
                    <TableHead>بازه</TableHead>
                    <TableHead>باقی‌مانده</TableHead>
                    <TableHead>مبلغ</TableHead>
                    <TableHead>پرداختی</TableHead>
                    <TableHead>بدهی</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {m.subscriptions.map((s) => (
                    <TableRow key={s.id} className={cn(!s.valid && !s.upcoming && 'text-muted-foreground')}>
                      <TableCell>
                        <div className="flex items-center gap-2 font-medium">
                          {s.plan_name}
                          {s.valid && <Badge variant="success">فعال</Badge>}
                          {s.upcoming && <Badge variant="outline">شروع نشده</Badge>}
                        </div>
                        <div className="text-muted-foreground mt-0.5 text-xs">
                          {planSummary(s)} · {FREQ_LABEL[s.frequency]}
                          {s.shower && ' · دوش'}
                          {s.locker && ' · کمد'}
                        </div>
                      </TableCell>
                      <TableCell>
                        {jalaliShort(s.start_date)}
                        {s.end_date && ` تا ${jalaliShort(s.end_date)}`}
                      </TableCell>
                      <TableCell>
                        {[
                          s.sessions_remaining !== null && `${num(s.sessions_remaining)} از ${num(s.sessions ?? 0)} جلسه`,
                          s.days_remaining !== null && `${num(s.days_remaining)} روز`,
                        ]
                          .filter(Boolean)
                          .join(' / ')}
                      </TableCell>
                      <TableCell>{toman(s.price)}</TableCell>
                      <TableCell>{toman(s.paid)}</TableCell>
                      <TableCell className={s.debt ? 'text-destructive font-medium' : ''}>{s.debt ? toman(s.debt) : '—'}</TableCell>
                      <TableCell className="text-end">
                        {s.debt > 0 && (
                          <Button size="sm" variant="outline" onClick={() => setDialog({ pay: s })}>
                            <BanknoteIcon />
                            ثبت پرداخت
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>

        <div className="grid gap-6 lg:grid-cols-2">
          <Card>
            <CardHeader>
              <CardTitle>پرداخت‌ها</CardTitle>
            </CardHeader>
            <CardContent className="px-2">
              {m.payments.length === 0 ? (
                <p className="text-muted-foreground px-4 text-sm">پرداختی ثبت نشده.</p>
              ) : (
                <Table>
                  <TableBody>
                    {m.payments.map((p) => (
                      <TableRow key={p.id}>
                        <TableCell>
                          {jalaliShort(p.paid_at)} <span className="text-muted-foreground">{time(p.paid_at)}</span>
                        </TableCell>
                        <TableCell className="text-muted-foreground">{p.plan_name}</TableCell>
                        <TableCell className="text-muted-foreground max-w-40 truncate">{p.note}</TableCell>
                        <TableCell className="text-end font-medium">{toman(p.amount)}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
          <Card>
            <CardHeader>
              <CardTitle>یادداشت و چهره</CardTitle>
            </CardHeader>
            <CardContent>
              <p className={cn('text-sm whitespace-pre-wrap', !m.notes && 'text-muted-foreground')}>{m.notes || 'یادداشتی ندارد.'}</p>
              <p className="text-muted-foreground mt-4 text-xs">عضو از {jalaliDate(m.created_at)}</p>
              {m.face_enrolled && !m.archived && (
                <div className="mt-4 flex flex-wrap items-center gap-2 border-t pt-4 text-sm">
                  <ScanFaceIcon className="text-success size-4" />
                  <span>چهره ثبت شده</span>
                  <Button size="sm" variant="outline" className="ms-auto" onClick={() => setDialog('face')}>
                    ثبت دوباره
                  </Button>
                  <Button size="sm" variant="ghost" className="text-destructive" disabled={removeFace.isPending} onClick={() => removeFace.mutate()}>
                    <Trash2Icon />
                    حذف چهره
                  </Button>
                </div>
              )}
            </CardContent>
          </Card>
        </div>

        <Card>
          <CardHeader>
            <CardTitle>ورود و خروج</CardTitle>
          </CardHeader>
          <CardContent className="px-2">
            {!visits.data?.length ? (
              <p className="text-muted-foreground px-4 text-sm">هنوز ورودی ثبت نشده.</p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>تاریخ</TableHead>
                    <TableHead>ورود</TableHead>
                    <TableHead>خروج</TableHead>
                    <TableHead>شهریه</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {visits.data.map((v) => (
                    <TableRow key={v.id}>
                      <TableCell>{jalaliShort(v.entered_at)}</TableCell>
                      <TableCell className="tabular-nums">
                        <span className="inline-flex items-center gap-1.5">
                          {time(v.entered_at)}
                          {v.entry_source !== 'camera' && <HandIcon className="text-muted-foreground size-3.5" aria-label="دستی" />}
                        </span>
                      </TableCell>
                      <TableCell className="tabular-nums">
                        {v.exited_at ? (
                          <span className="text-muted-foreground inline-flex items-center gap-1.5">
                            {time(v.exited_at)}
                            {v.exit_source === 'manual' && <HandIcon className="size-3.5" aria-label="دستی" />}
                            {v.exit_source === 'auto' && <TimerOffIcon className="size-3.5" aria-label="خروج خودکار" />}
                          </span>
                        ) : (
                          <span className="text-success font-medium">داخل</span>
                        )}
                      </TableCell>
                      <TableCell>
                        <StatusBadge status={v.status} />
                      </TableCell>
                      <TableCell>
                        <div className="flex flex-wrap gap-1">
                          {v.flags.map((f) => (
                            <Badge key={f} variant={FLAG_LABEL[f].variant}>
                              {FLAG_LABEL[f].label}
                            </Badge>
                          ))}
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      </div>

      <Dialog open={dialog !== null} onOpenChange={(o) => !o && closeDialog()}>
        {dialog === 'face' && (
          <FaceEnrollDialog
            member={m}
            onDone={() => {
              void qc.invalidateQueries({ queryKey: ['member', id] })
              void qc.invalidateQueries({ queryKey: ['members'] })
              // wizard: new member without a subscription goes on to sell one
              if (wizard && m.subscriptions.length === 0) {
                setParams({}, { replace: true })
                setDialog('sell')
              } else closeDialog()
            }}
          />
        )}
        {dialog === 'edit' && (
          <MemberForm
            member={m}
            onDone={() => {
              setDialog(null)
            }}
          />
        )}
        {dialog === 'sell' && <SellForm member={m} onDone={(x) => (setMember(x), closeDialog())} />}
        {typeof dialog === 'object' && dialog && <PayForm sub={dialog.pay} onDone={(x) => (setMember(x), closeDialog())} />}
      </Dialog>
    </>
  )
}

function SellForm({ member, onDone }: { member: MemberDetail; onDone: (m: MemberDetail) => void }) {
  const plans = useQuery({ queryKey: ['plans', 'active'], queryFn: () => get<Plan[]>('/plans') })
  const [planId, setPlanId] = useState<number | null>(null)
  const [price, setPrice] = useState(0)
  const [start, setStart] = useState<string | null>(null)
  const [paid, setPaid] = useState(0)
  const [note, setNote] = useState('')
  const plan = plans.data?.find((p) => p.id === planId)

  // the server knows the renewal rule (§4.8): ask it for the default start date and the end date
  const preview = useQuery({
    queryKey: ['preview', member.id, planId, start],
    queryFn: () =>
      get<{ start_date: string; end_date: string | null; price: number }>(
        `/members/${member.id}/subscriptions/preview?plan_id=${planId}${start ? `&start_date=${start}` : ''}`,
      ),
    enabled: planId !== null,
  })
  useEffect(() => {
    if (preview.data && start === null) setStart(preview.data.start_date)
  }, [preview.data, start])

  function choose(id: number) {
    const p = plans.data!.find((x) => x.id === id)!
    setPlanId(id)
    setPrice(p.price)
    setPaid(p.price)
    setStart(null) // re-ask for the default start of this plan
  }

  const sell = useMutation({
    mutationFn: () => post<MemberDetail>(`/members/${member.id}/subscriptions`, { plan_id: planId, price, start_date: start, paid, note }),
    onSuccess: (m) => {
      toast.success('اشتراک ثبت شد')
      onDone(m)
    },
    onError: (e) => toast.error(errorText(e)),
  })

  const debt = Math.max(price - paid, 0)
  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>فروش اشتراک</DialogTitle>
        <DialogDescription>{member.full_name}</DialogDescription>
      </DialogHeader>
      {plans.data?.length === 0 ? (
        <p className="text-sm">
          تعرفه‌ی فعالی وجود ندارد.{' '}
          <Link to="/plans" className="text-primary underline">
            ساخت تعرفه
          </Link>
        </p>
      ) : (
        <form
          className="grid gap-5"
          onSubmit={(e: FormEvent) => {
            e.preventDefault()
            sell.mutate()
          }}
        >
          <Field label="تعرفه">
            <Select value={planId ? String(planId) : ''} onValueChange={(v) => choose(Number(v))}>
              <SelectTrigger className="w-full">
                <SelectValue placeholder="انتخاب تعرفه" />
              </SelectTrigger>
              <SelectContent>
                {plans.data?.map((p) => (
                  <SelectItem key={p.id} value={String(p.id)}>
                    {p.name} <span className="text-muted-foreground">· {toman(p.price)}</span>
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>

          {plan && (
            <>
              <p className="text-muted-foreground -mt-2 text-sm">
                {planSummary(plan)} · {FREQ_LABEL[plan.frequency]}
                {plan.shower && ' · دوش'}
                {plan.locker && ' · کمد'}
              </p>
              <div className="grid gap-5 sm:grid-cols-2">
                <Field label="تاریخ شروع" htmlFor="s-start">
                  <JalaliDateInput id="s-start" value={start} onChange={(v) => v && setStart(v)} />
                </Field>
                <Field label="تاریخ پایان">
                  <p className="flex h-9 items-center text-sm">{preview.data?.end_date ? jalaliDate(preview.data.end_date) : plan.kind === 'sessions' ? 'تا پایان جلسات' : '—'}</p>
                </Field>
              </div>
              <div className="grid gap-5 sm:grid-cols-2">
                <Field label="مبلغ اشتراک" htmlFor="s-price" hint={price !== plan.price ? `قیمت تعرفه: ${toman(plan.price)}` : undefined}>
                  <MoneyInput id="s-price" value={price} onChange={(v) => (setPrice(v), setPaid((p) => Math.min(p, v)))} />
                </Field>
                <Field
                  label="مبلغ پرداختی"
                  htmlFor="s-paid"
                  error={paid > price ? 'بیشتر از مبلغ اشتراک است' : undefined}
                  hint={debt ? <span className="text-destructive">بدهی: {toman(debt)}</span> : 'تسویه‌ی کامل'}
                >
                  <MoneyInput id="s-paid" value={paid} onChange={setPaid} invalid={paid > price} />
                </Field>
              </div>
              <Field label="یادداشت پرداخت" htmlFor="s-note">
                <Input id="s-note" value={note} onChange={(e) => setNote(e.target.value)} placeholder="مثلاً کارت‌خوان" />
              </Field>
            </>
          )}

          <DialogFooter>
            <Button type="submit" disabled={!plan || !start || paid > price || sell.isPending}>
              ثبت اشتراک
            </Button>
          </DialogFooter>
        </form>
      )}
    </DialogContent>
  )
}

function PayForm({ sub, onDone }: { sub: Subscription; onDone: (m: MemberDetail) => void }) {
  const [amount, setAmount] = useState(sub.debt)
  const [note, setNote] = useState('')
  const pay = useMutation({
    mutationFn: () => post<MemberDetail>(`/subscriptions/${sub.id}/payments`, { amount, note }),
    onSuccess: (m) => {
      toast.success('پرداخت ثبت شد')
      onDone(m)
    },
    onError: (e) => toast.error(errorText(e)),
  })
  return (
    <DialogContent className="sm:max-w-md">
      <DialogHeader>
        <DialogTitle>ثبت پرداخت</DialogTitle>
        <DialogDescription>
          {sub.plan_name} · بدهی {toman(sub.debt)}
        </DialogDescription>
      </DialogHeader>
      <form
        className="grid gap-5"
        onSubmit={(e) => {
          e.preventDefault()
          pay.mutate()
        }}
      >
        <Field label="مبلغ" htmlFor="p-amount" error={amount > sub.debt ? 'بیشتر از بدهی است' : undefined}>
          <MoneyInput id="p-amount" value={amount} onChange={setAmount} invalid={amount > sub.debt} />
        </Field>
        <Field label="یادداشت" htmlFor="p-note">
          <Input id="p-note" value={note} onChange={(e) => setNote(e.target.value)} placeholder="اختیاری" />
        </Field>
        <DialogFooter>
          <Button type="submit" disabled={amount <= 0 || amount > sub.debt || pay.isPending}>
            ثبت پرداخت
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
