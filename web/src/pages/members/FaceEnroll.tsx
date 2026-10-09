// Face enrollment through gym-server -> face-service. Live camera, progress,
// Persian guidance from face-service's hint codes.

import { useMutation, useQuery } from '@tanstack/react-query'
import { CameraOffIcon, CheckCircle2Icon, Loader2Icon, RotateCcwIcon } from 'lucide-react'
import { useEffect, useState } from 'react'
import { toast } from 'sonner'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { ApiError, api, get, post } from '@/lib/api'
import { num } from '@/lib/format'
import type { MemberDetail } from '@/lib/types'
import { previewSrc } from '@/lib/media'
import { cn } from '@/lib/utils'

type EnrollStatus =
  | { state: 'idle' }
  | { state: 'collecting' | 'ready' | 'failed'; member_id: string; collected: number; target: number; hint: string; hint_code: string }

const HINTS: Record<string, string> = {
  look: 'به دوربین نگاه کنید',
  no_face: 'چهره‌ای دیده نمی‌شود؛ روبه‌روی دوربین بایستید',
  multiple_faces: 'فقط یک نفر جلوی دوربین باشد',
  low_score: 'چهره واضح نیست؛ نور را بیشتر کنید',
  too_small: 'کمی نزدیک‌تر بیایید',
  turned: 'صاف به دوربین نگاه کنید',
  tilted: 'سر را صاف نگه دارید',
  cut_off: 'تمام صورت داخل تصویر باشد',
  too_similar: 'آرام سر را کمی به چپ، راست، بالا یا پایین بچرخانید',
  different_person: 'فرد دیگری جلوی دوربین است؛ فقط خود عضو بایستد',
  good: 'خوب است؛ آرام سر را حرکت دهید',
  done: 'نمونه‌ها کامل شد',
  timeout_ready: 'زمان تمام شد، ولی نمونه‌ی کافی جمع شد',
  timeout_failed: 'نمونه‌ی کافی جمع نشد. نور را بهتر کنید و دوباره امتحان کنید',
}

const errorText = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

/** Call when the dialog closes: frees the camera for recognition if the session wasn't saved. */
export const cancelFaceEnroll = () => api('DELETE', '/face/enroll').catch(() => {})

export function FaceEnrollDialog({ member, onDone }: { member: MemberDetail; onDone: (enrolled: boolean) => void }) {
  const [attempt, setAttempt] = useState(0)
  const [startError, setStartError] = useState<string | null>(null)

  // start a session when the dialog opens (and on "try again")
  useEffect(() => {
    let cancelled = false
    setStartError(null)
    post(`/members/${member.id}/face/start`, { samples: 8 }).catch(async (e) => {
      // a stale session (e.g. a closed tab) blocks the camera: cancel it and retry once
      if (e instanceof ApiError && e.status === 409) {
        await api('DELETE', '/face/enroll').catch(() => {})
        return post(`/members/${member.id}/face/start`, { samples: 8 })
      }
      throw e
    }).catch((e) => !cancelled && setStartError(errorText(e)))
    return () => {
      cancelled = true
    }
  }, [member.id, attempt])

  const status = useQuery({
    queryKey: ['face-enroll', member.id, attempt],
    queryFn: () => get<EnrollStatus>('/face/enroll'),
    refetchInterval: (q) => (q.state.data && q.state.data.state !== 'collecting' && q.state.data.state !== 'idle' ? false : 400),
    enabled: !startError,
  })
  const st = status.data && status.data.state !== 'idle' && status.data.member_id === String(member.id) ? status.data : null

  const commit = useMutation({
    mutationFn: () => post(`/members/${member.id}/face/commit`),
    onSuccess: () => {
      toast.success(`چهره‌ی ${member.full_name} ثبت شد`)
      onDone(true)
    },
    onError: (e) => toast.error(errorText(e)),
  })

  const progress = st ? Math.min(100, (100 * st.collected) / st.target) : 0
  return (
    <DialogContent className="sm:max-w-2xl">
      <DialogHeader>
        <DialogTitle>ثبت چهره</DialogTitle>
        <DialogDescription>{member.full_name} روبه‌روی دوربین بایستد و آرام سرش را کمی به اطراف بچرخاند.</DialogDescription>
      </DialogHeader>

      {startError ? (
        <Alert variant="destructive">
          <CameraOffIcon />
          <AlertTitle>دوربین در دسترس نیست</AlertTitle>
          <AlertDescription>{startError}</AlertDescription>
        </Alert>
      ) : (
        <>
          <div className="bg-muted relative aspect-[4/3] overflow-hidden rounded-lg">
            <img src={previewSrc(attempt)} alt="تصویر زنده‌ی دوربین" className="size-full object-contain" />
            {st?.state === 'ready' && (
              <div className="bg-background/70 absolute inset-0 grid place-items-center backdrop-blur-sm">
                <CheckCircle2Icon className="text-success size-16" />
              </div>
            )}
          </div>
          <div className="grid gap-2">
            <div className="bg-muted h-2 overflow-hidden rounded-full">
              <div
                className={cn('h-full rounded-full transition-[width] duration-300', st?.state === 'failed' ? 'bg-destructive' : st?.state === 'ready' ? 'bg-success' : 'bg-primary')}
                style={{ width: `${progress}%` }}
              />
            </div>
            <div className="flex items-center justify-between gap-4 text-sm">
              <p className={cn('font-medium', st?.state === 'failed' && 'text-destructive')}>
                {st ? HINTS[st.hint_code] ?? st.hint : 'در حال آماده‌سازی…'}
              </p>
              {st && (
                <span className="text-muted-foreground shrink-0">
                  {num(st.collected)} از {num(st.target)}
                </span>
              )}
            </div>
          </div>
        </>
      )}

      <DialogFooter>
        {(st?.state === 'failed' || startError) && (
          <Button variant="outline" onClick={() => setAttempt((a) => a + 1)}>
            <RotateCcwIcon />
            تلاش دوباره
          </Button>
        )}
        {st?.state === 'ready' && (
          <Button onClick={() => commit.mutate()} disabled={commit.isPending}>
            {commit.isPending && <Loader2Icon className="animate-spin" />}
            ذخیره‌ی چهره
          </Button>
        )}
      </DialogFooter>
    </DialogContent>
  )
}
