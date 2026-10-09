// Reception: the page that stays open all day (SPEC §5). Live list of today's
// entries and exits, cards for faces the camera couldn't place, manual entry.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  CameraIcon,
  DoorClosedIcon,
  EllipsisVerticalIcon,
  HandIcon,
  LogInIcon,
  LogOutIcon,
  TimerOffIcon,
  Undo2Icon,
  UserRoundSearchIcon,
  VideoIcon,
  VideoOffIcon,
  XIcon,
} from 'lucide-react'
import { AnimatePresence, motion } from 'motion/react'
import { useState } from 'react'
import { Link } from 'react-router'
import { toast } from 'sonner'

import { MemberPicker } from '@/components/MemberPicker'
import { PageHeader } from '@/components/page'
import { StatusBadge } from '@/components/StatusBadge'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from '@/components/ui/dropdown-menu'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { ApiError, get, post } from '@/lib/api'
import { faDigits, num, time } from '@/lib/format'
import { previewSrc, snapshotSrc } from '@/lib/media'
import { FLAG_LABEL, GENDER_LABEL, type FaceEvent, type Reception, type Visit } from '@/lib/types'
import { cn } from '@/lib/utils'
import { remainingText } from './members/MembersList'
import { SHIFT_COLOR, SHIFT_LABEL, type Shift } from './Shifts'

const errorText = (e: unknown) => (e instanceof ApiError ? e.message : String(e))
const PREVIEW_KEY = 'reception-preview'

function readPreviewPref(): boolean {
  try {
    return localStorage.getItem(PREVIEW_KEY) === '1'
  } catch {
    return false
  }
}

type FaceHealth = { reachable: boolean; camera?: { connected: boolean; active?: boolean; name?: string }; events?: boolean }

export function ReceptionPage() {
  const qc = useQueryClient()
  // the live stream (Layout) invalidates this on every event; the interval is only a safety net
  const q = useQuery({ queryKey: ['reception'], queryFn: () => get<Reception>('/reception'), refetchInterval: 60_000 })
  const health = useQuery({ queryKey: ['face-health'], queryFn: () => get<FaceHealth>('/face/health'), refetchInterval: 10_000 })
  const [picking, setPicking] = useState<null | { face?: FaceEvent }>(null)
  const [preview, setPreview] = useState(readPreviewPref)

  const refresh = () => {
    void qc.invalidateQueries({ queryKey: ['reception'] })
    void qc.invalidateQueries({ queryKey: ['members'] })
  }
  const onError = (e: unknown) => toast.error(errorText(e))

  const enter = useMutation({
    mutationFn: ({ member, face }: { member: number; face?: number }) => post(`/members/${member}/enter`, face ? { face_event_id: face } : {}),
    onSuccess: () => (setPicking(null), refresh()),
    onError,
  })
  const exit = useMutation({ mutationFn: (member: number) => post(`/members/${member}/exit`), onSuccess: refresh, onError })
  const cancel = useMutation({
    mutationFn: (visit: number) => post(`/visits/${visit}/cancel`),
    onSuccess: () => (refresh(), toast.success('ورود حذف شد و جلسه برگشت')),
    onError,
  })
  const dismiss = useMutation({ mutationFn: (id: number) => post(`/face-events/${id}/dismiss`), onSuccess: refresh, onError })

  function togglePreview() {
    setPreview((p) => {
      try {
        localStorage.setItem(PREVIEW_KEY, p ? '0' : '1')
      } catch {
        /* private mode: just don't remember */
      }
      return !p
    })
  }

  const h = health.data
  const problem = !h
    ? null
    : !h.reachable
      ? 'سرویس تشخیص چهره اجرا نیست'
      : !h.camera?.connected
        ? (h.camera?.active === false || h.camera?.name === 'paused' ? null : 'دوربین وصل نیست')
        : h.events === false
          ? 'اتصال به رویدادهای دوربین برقرار نیست'
          : null

  const data = q.data
  return (
    <>
      <PageHeader title="پذیرش" description="این پنجره را باز بگذارید: دوربین فقط وقتی برنامه باز است روشن است و ۲ دقیقه بعد از بستن آن خاموش می‌شود.">
        <div className="flex flex-wrap items-center gap-2">
          <ShiftNow />
          <Stat label="الان داخل" value={data?.inside} />
          <Stat label="ورود امروز" value={data?.entries_today} />
          <Button variant="outline" onClick={togglePreview}>
            {preview ? <VideoOffIcon /> : <VideoIcon />}
            {preview ? 'بستن دوربین' : 'نمایش دوربین'}
          </Button>
          <Button onClick={() => setPicking({})}>
            <LogInIcon />
            ورود دستی
          </Button>
        </div>
      </PageHeader>

      <div className="grid gap-6">
        {problem && (
          <Alert variant="destructive">
            <CameraIcon />
            <AlertTitle>{problem}</AlertTitle>
            <AlertDescription>تا رفع مشکل، ورود و خروج خودکار ثبت نمی‌شود؛ از «ورود دستی» استفاده کنید.</AlertDescription>
          </Alert>
        )}

        {preview && (
          <Card className="overflow-hidden p-0">
            <img src={previewSrc()} alt="تصویر زنده‌ی دوربین" className="mx-auto aspect-[4/3] max-h-80 bg-black object-contain" />
          </Card>
        )}

        {data && data.face_events.length > 0 && (
          <section>
            <h2 className="text-muted-foreground mb-3 text-sm font-medium">نیاز به بررسی</h2>
            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
              <AnimatePresence initial={false} mode="popLayout">
              {data.face_events.map((f) => (
                <FaceCard
                  key={f.id}
                  event={f}
                  busy={enter.isPending || dismiss.isPending}
                  onPick={(member) => enter.mutate({ member, face: f.id })}
                  onOther={() => setPicking({ face: f })}
                  onDismiss={() => dismiss.mutate(f.id)}
                />
              ))}
              </AnimatePresence>
            </div>
          </section>
        )}

        <Card className="py-2">
          {data && data.visits.length === 0 ? (
            <div className="text-muted-foreground py-16 text-center">
              <DoorClosedIcon className="mx-auto mb-3 size-8 opacity-50" />
              هنوز کسی امروز وارد نشده.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-14" />
                  <TableHead>عضو</TableHead>
                  <TableHead>ورود</TableHead>
                  <TableHead>خروج</TableHead>
                  <TableHead>شهریه</TableHead>
                  <TableHead>باقی‌مانده</TableHead>
                  <TableHead className="w-10" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {/* initial={false}: animate only rows that arrive while the page is open */}
                <AnimatePresence initial={false}>
                  {data?.visits.map((v) => (
                    <VisitRow key={v.id} v={v} onExit={() => exit.mutate(v.member_id)} onCancel={() => cancel.mutate(v.id)} />
                  ))}
                </AnimatePresence>
              </TableBody>
            </Table>
          )}
        </Card>
      </div>

      <Dialog open={picking !== null} onOpenChange={(o) => !o && setPicking(null)}>
        {picking && (
          <MemberPicker
            title={picking.face ? 'این شخص کیست؟' : 'ورود دستی'}
            description={picking.face ? 'عضو را انتخاب کنید تا ورودش ثبت شود.' : 'ورود عضو بدون دوربین ثبت می‌شود.'}
            busy={enter.isPending}
            onPick={(m) => enter.mutate({ member: m.id, face: picking.face?.id })}
          />
        )}
      </Dialog>
    </>
  )
}

type CurrentShift = { uses_shifts: boolean; current: Shift | null; next: Shift | null }

/** Whose shift it is now (only when the gym defines shifts). */
function ShiftNow() {
  const q = useQuery({ queryKey: ['shifts-current'], queryFn: () => get<CurrentShift>('/shifts/current'), refetchInterval: 60_000 })
  const d = q.data
  if (!d?.uses_shifts) return null
  if (d.current) {
    return (
      <span className={cn('flex h-9 items-center rounded-md px-3 text-sm font-medium ring-1', SHIFT_COLOR[d.current.gender])}>
        سانس {SHIFT_LABEL[d.current.gender]} تا {faDigits(d.current.end_time)}
      </span>
    )
  }
  return (
    <span className="bg-warning/15 flex h-9 items-center rounded-md px-3 text-sm">
      خارج از سانس
      {d.next && <span className="text-muted-foreground ms-1">· بعدی: {SHIFT_LABEL[d.next.gender]} {faDigits(d.next.start_time)}</span>}
    </span>
  )
}

function Stat({ label, value }: { label: string; value?: number }) {
  return (
    <div className="bg-card flex h-9 items-center gap-2 rounded-md border px-3 text-sm">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-bold tabular-nums">{value === undefined ? '…' : num(value)}</span>
    </div>
  )
}

function Avatar({ snapshot, name }: { snapshot: string | null; name: string }) {
  if (snapshot) return <img src={snapshotSrc(snapshot)} alt="" className="bg-muted size-10 rounded-lg object-cover" />
  return <span className="bg-muted text-muted-foreground flex size-10 items-center justify-center rounded-lg text-sm font-bold">{name.trim().charAt(0)}</span>
}

const MotionRow = motion.create(TableRow)
const MotionCard = motion.create(Card)

function VisitRow({ v, onExit, onCancel }: { v: Visit; onExit: () => void; onCancel: () => void }) {
  const inside = v.exited_at === null
  const alert = v.status !== 'ok' || v.flags.includes('wrong_shift')
  return (
    <MotionRow
      layout="position"
      initial={{ opacity: 0, y: -12 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, transition: { duration: 0.15 } }}
      transition={{ type: 'spring', stiffness: 420, damping: 34 }}
      className={cn(alert && 'bg-destructive/[0.06] hover:bg-destructive/10')}
    >
      <TableCell>
        <Avatar snapshot={v.snapshot} name={v.full_name} />
      </TableCell>
      <TableCell>
        <Link to={`/members/${v.member_id}`} className="font-medium hover:underline">
          {v.full_name}
        </Link>
        <span className="text-muted-foreground ms-2 text-xs">{GENDER_LABEL[v.gender]}</span>
        {v.flags.length > 0 && (
          <div className="mt-1 flex flex-wrap gap-1">
            {v.flags.map((f) => (
              <Badge key={f} variant={FLAG_LABEL[f].variant}>
                {FLAG_LABEL[f].label}
              </Badge>
            ))}
          </div>
        )}
      </TableCell>
      <TableCell className="tabular-nums">
        <span className="inline-flex items-center gap-1.5">
          {time(v.entered_at)}
          {v.entry_source !== 'camera' && <HandIcon className="text-muted-foreground size-3.5" aria-label="دستی" />}
        </span>
      </TableCell>
      <TableCell className="tabular-nums">
        {inside ? (
          <span className="text-success inline-flex items-center gap-1.5 font-medium">
            <span className="bg-success size-2 rounded-full" />
            داخل
          </span>
        ) : (
          <span className="text-muted-foreground inline-flex items-center gap-1.5">
            {time(v.exited_at!)}
            {v.exit_source === 'manual' && <HandIcon className="size-3.5" aria-label="دستی" />}
            {v.exit_source === 'auto' && <TimerOffIcon className="size-3.5" aria-label="خروج خودکار" />}
          </span>
        )}
      </TableCell>
      <TableCell>
        <StatusBadge status={v.status} />
      </TableCell>
      <TableCell className="text-muted-foreground">{remainingText(v.current)}</TableCell>
      <TableCell>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="ghost" size="icon-sm" aria-label="عملیات">
              <EllipsisVerticalIcon />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            {inside && (
              <DropdownMenuItem onSelect={onExit}>
                <LogOutIcon />
                ثبت خروج
              </DropdownMenuItem>
            )}
            <DropdownMenuItem onSelect={onCancel} className="text-destructive focus:text-destructive">
              <Undo2Icon />
              حذف ورود اشتباه
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </TableCell>
    </MotionRow>
  )
}

function FaceCard({
  event,
  busy,
  onPick,
  onOther,
  onDismiss,
}: {
  event: FaceEvent
  busy: boolean
  onPick: (member: number) => void
  onOther: () => void
  onDismiss: () => void
}) {
  const uncertain = event.type === 'uncertain'
  return (
    <MotionCard
      layout
      initial={{ opacity: 0, scale: 0.94 }}
      animate={{ opacity: 1, scale: 1 }}
      exit={{ opacity: 0, scale: 0.94, transition: { duration: 0.15 } }}
      transition={{ type: 'spring', stiffness: 420, damping: 32 }}
      className="flex-row gap-3 p-3"
    >
      {event.snapshot ? (
        <img src={snapshotSrc(event.snapshot)} alt="" className="bg-muted size-20 shrink-0 rounded-lg object-cover" />
      ) : (
        <span className="bg-muted size-20 shrink-0 rounded-lg" />
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex items-center gap-2">
          <Badge variant={uncertain ? 'warning' : 'secondary'}>{uncertain ? 'مطمئن نیستم' : 'ناشناس'}</Badge>
          <span className="text-muted-foreground text-xs tabular-nums">{time(event.at)}</span>
          <Button variant="ghost" size="icon-sm" className="ms-auto -mt-1 -me-1" aria-label="نادیده گرفتن" disabled={busy} onClick={onDismiss}>
            <XIcon />
          </Button>
        </div>
        <div className="flex flex-wrap gap-1.5">
          {event.candidates.map((c) => (
            <Button key={c.member_id} size="sm" variant="secondary" disabled={busy} onClick={() => onPick(c.member_id)}>
              {c.full_name}
            </Button>
          ))}
          <Button size="sm" variant="ghost" disabled={busy} onClick={onOther}>
            <UserRoundSearchIcon />
            {event.candidates.length ? 'کس دیگر' : 'انتخاب عضو'}
          </Button>
        </div>
      </div>
    </MotionCard>
  )
}
