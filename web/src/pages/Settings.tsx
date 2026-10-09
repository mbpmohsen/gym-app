import { useMutation, useQueryClient } from '@tanstack/react-query'
import { CheckIcon, PlayIcon } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { toast } from 'sonner'

import { useSettings, type Settings as S } from '@/components/Layout'
import { Field, PageHeader } from '@/components/page'
import { MODES } from '@/components/ThemeMenu'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Switch } from '@/components/ui/switch'
import { ApiError, post, put } from '@/lib/api'
import { loadVoice, play, type Sound, type Voice } from '@/lib/audio'
import { ACCENTS, setTheme, useTheme } from '@/lib/theme'
import { cn } from '@/lib/utils'

const SAMPLES: { sound: Sound; label: string }[] = [
  { sound: 'welcome', label: 'خوش آمدید' },
  { sound: 'end-of-tuition', label: 'پایان شهریه' },
  { sound: 'wrong-shift', label: 'سانس نامعتبر' },
  { sound: 'goodbye', label: 'خداحافظ' },
]

const errorText = (e: unknown) => (e instanceof ApiError ? e.message : String(e))

export function SettingsPage() {
  const q = useSettings()
  return (
    <>
      <PageHeader title="تنظیمات" />
      <div className="grid gap-6">
        <Appearance />
        {q.data && <GeneralForm initial={q.data} />}
        <PasswordForm />
      </div>
    </>
  )
}

function Appearance() {
  const t = useTheme()
  return (
    <Card>
      <CardHeader>
        <CardTitle>ظاهر</CardTitle>
        <CardDescription>فقط روی همین کامپیوتر ذخیره می‌شود.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-6">
        <div className="grid gap-3">
          <Label>حالت</Label>
          <div className="grid max-w-md grid-cols-3 gap-3">
            {MODES.map((m) => (
              <button
                key={m.id}
                type="button"
                onClick={() => setTheme({ mode: m.id })}
                aria-pressed={t.mode === m.id}
                className={cn(
                  'hover:bg-accent flex flex-col items-center gap-2 rounded-lg border p-4 text-sm transition-colors',
                  t.mode === m.id && 'border-primary ring-primary/30 ring-2',
                )}
              >
                <m.icon className="size-5" />
                {m.label}
              </button>
            ))}
          </div>
        </div>
        <div className="grid gap-3">
          <Label>رنگ اصلی</Label>
          <div className="flex flex-wrap gap-3">
            {ACCENTS.map((a) => (
              <button
                key={a.id}
                type="button"
                onClick={() => setTheme({ accent: a.id })}
                aria-pressed={t.accent === a.id}
                aria-label={a.label}
                title={a.label}
                className={cn(
                  'grid size-10 place-items-center rounded-full border border-foreground/15 ring-offset-2 ring-offset-background transition-shadow',
                  t.accent === a.id && 'ring-2 ring-foreground/60',
                )}
                style={{ background: a.swatch }}
              >
                {t.accent === a.id && <CheckIcon className="size-5 text-white" />}
              </button>
            ))}
          </div>
        </div>
      </CardContent>
    </Card>
  )
}

function GeneralForm({ initial }: { initial: S }) {
  const qc = useQueryClient()
  const [s, setS] = useState(initial)
  useEffect(() => setS(initial), [initial])
  const save = useMutation({
    mutationFn: (v: S) => put<S>('/settings', v),
    onSuccess: (v) => {
      qc.setQueryData(['settings'], v)
      toast.success('تنظیمات ذخیره شد')
    },
    onError: (e) => toast.error(errorText(e)),
  })
  const set = <K extends keyof S>(k: K, v: S[K]) => setS((p) => ({ ...p, [k]: v }))
  const num = (k: 'exit_min_minutes' | 'second_visit_hours' | 'auto_exit_hours') => (
    <Input id={k} type="number" inputMode="numeric" min={1} value={s[k]} onChange={(e) => set(k, Number(e.target.value))} className="w-28" />
  )

  async function preview(voice: Voice, sound: Sound) {
    try {
      await loadVoice(voice)
      await play(sound)
    } catch (e) {
      toast.error(`پخش نشد: ${errorText(e)}`)
    }
  }

  return (
    <form
      onSubmit={(e: FormEvent) => {
        e.preventDefault()
        save.mutate(s)
      }}
      className="grid gap-6"
    >
      <Card>
        <CardHeader>
          <CardTitle>باشگاه و صدای اعلام</CardTitle>
        </CardHeader>
        <CardContent className="grid gap-6">
          <Field label="نام باشگاه" htmlFor="gym_name" hint="بالای منو نمایش داده می‌شود" className="max-w-md">
            <Input id="gym_name" value={s.gym_name} onChange={(e) => set('gym_name', e.target.value)} />
          </Field>
          <div className="grid gap-3">
            <Label>صدای اعلام</Label>
            <RadioGroup dir="rtl" value={s.voice} onValueChange={(v) => set('voice', v as Voice)} className="flex gap-6">
              {(['male', 'female'] as const).map((v) => (
                <div key={v} className="flex items-center gap-2">
                  <RadioGroupItem value={v} id={`voice-${v}`} />
                  <Label htmlFor={`voice-${v}`} className="font-normal">
                    {v === 'male' ? 'مرد' : 'زن'}
                  </Label>
                </div>
              ))}
            </RadioGroup>
            <div className="flex flex-wrap gap-2">
              {SAMPLES.map((x) => (
                <Button key={x.sound} type="button" variant="outline" size="sm" onClick={() => void preview(s.voice, x.sound)}>
                  <PlayIcon />
                  {x.label}
                </Button>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>ورود و خروج</CardTitle>
          <CardDescription>قواعد ثبت خودکار با دوربین</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-6">
          <Field label="حداقل فاصله‌ی ورود تا خروج (دقیقه)" htmlFor="exit_min_minutes" hint="اگر عضو زودتر از این دوباره جلوی دوربین بیاید، چیزی ثبت نمی‌شود؛ مثلاً موقع درآوردن کفش.">
            {num('exit_min_minutes')}
          </Field>
          <Field label="ورود دوم در یک روز (ساعت)" htmlFor="second_visit_hours" hint="ورودی که این مقدار بعد از ورود قبلی باشد، برچسب «بار دوم امروز» می‌گیرد. جلسه‌ی دوم کسر نمی‌شود.">
            {num('second_visit_hours')}
          </Field>
          <Field label="خروج خودکار (ساعت)" htmlFor="auto_exit_hours" hint="عضوی که این مدت داخل بماند و خروجش دیده نشود، خودکار خارج‌شده ثبت می‌شود.">
            {num('auto_exit_hours')}
          </Field>
          <div className="flex items-center gap-3">
            <Switch id="wsa" dir="rtl" checked={s.wrong_shift_alarm} onCheckedChange={(v) => set('wrong_shift_alarm', v)} />
            <Label htmlFor="wsa" className="font-normal">
              هشدار صوتی ورود در سانس نامعتبر
            </Label>
          </div>
        </CardContent>
        <CardFooter className="border-t">
          <Button type="submit" disabled={save.isPending}>
            ذخیره‌ی تنظیمات
          </Button>
        </CardFooter>
      </Card>
    </form>
  )
}

function PasswordForm() {
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const change = useMutation({
    mutationFn: () => post('/auth/password', { current, new: next }),
    onSuccess: () => {
      setCurrent('')
      setNext('')
      toast.success('رمز تغییر کرد')
    },
    onError: (e) => toast.error(errorText(e)),
  })
  return (
    <Card>
      <CardHeader>
        <CardTitle>تغییر رمز</CardTitle>
      </CardHeader>
      <form
        onSubmit={(e) => {
          e.preventDefault()
          change.mutate()
        }}
      >
        <CardContent className="grid max-w-md gap-5">
          <Field label="رمز فعلی" htmlFor="cur">
            <Input id="cur" type="password" value={current} onChange={(e) => setCurrent(e.target.value)} autoComplete="current-password" />
          </Field>
          <Field label="رمز جدید" htmlFor="new">
            <Input id="new" type="password" value={next} onChange={(e) => setNext(e.target.value)} autoComplete="new-password" />
          </Field>
        </CardContent>
        <CardFooter className="mt-6 border-t">
          <Button type="submit" variant="secondary" disabled={!current || !next || change.isPending}>
            تغییر رمز
          </Button>
        </CardFooter>
      </form>
    </Card>
  )
}
