// First run: choose the admin password. Later: log in. Either submit also unlocks audio.

import { useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertCircleIcon, Loader2Icon } from 'lucide-react'
import { useState, type FormEvent, type ReactNode } from 'react'

import { Field, PlateMark } from '@/components/page'
import { Alert, AlertTitle } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { ApiError, get, post } from '@/lib/api'
import { unlockAudio } from '@/lib/audio'

type AuthState = { setup_required: boolean; authenticated: boolean }

export function AuthGate({ children }: { children: ReactNode }) {
  const q = useQuery({ queryKey: ['auth'], queryFn: () => get<AuthState>('/auth/state') })

  if (q.isPending) return null
  if (q.isError) {
    return (
      <Centered>
        <Alert variant="destructive">
          <AlertCircleIcon />
          <AlertTitle>{q.error.message}</AlertTitle>
        </Alert>
      </Centered>
    )
  }
  if (q.data.authenticated) return <>{children}</>
  return <PasswordForm setup={q.data.setup_required} />
}

function PasswordForm({ setup }: { setup: boolean }) {
  const qc = useQueryClient()
  const [password, setPassword] = useState('')
  const [repeat, setRepeat] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(e: FormEvent) {
    e.preventDefault()
    setError('')
    if (setup && password !== repeat) {
      setError('دو رمز یکسان نیستند')
      return
    }
    void unlockAudio() // this submit is a user gesture: lets the page play sounds from now on
    setBusy(true)
    try {
      await post(setup ? '/auth/setup' : '/auth/login', { password })
      await qc.invalidateQueries()
    } catch (err) {
      setError(err instanceof ApiError ? err.message : String(err))
      setBusy(false)
    }
  }

  return (
    <Centered>
      <Card>
        <CardHeader>
          <CardTitle className="text-xl">{setup ? 'رمز مدیر را تعیین کنید' : 'ورود به برنامه'}</CardTitle>
          <CardDescription>{setup ? 'این رمز برای ورود به برنامه لازم است. جایی یادداشتش کنید.' : 'رمز مدیر را وارد کنید.'}</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit} className="grid gap-5">
            <Field label="رمز" htmlFor="pw">
              <Input id="pw" type="password" autoFocus value={password} onChange={(e) => setPassword(e.target.value)} autoComplete={setup ? 'new-password' : 'current-password'} aria-invalid={!!error} />
            </Field>
            {setup && (
              <Field label="تکرار رمز" htmlFor="pw2">
                <Input id="pw2" type="password" value={repeat} onChange={(e) => setRepeat(e.target.value)} autoComplete="new-password" />
              </Field>
            )}
            {error && <p className="text-destructive text-sm">{error}</p>}
            <Button type="submit" size="lg" disabled={busy || password.length === 0} className="w-full">
              {busy && <Loader2Icon className="animate-spin" />}
              {setup ? 'ذخیره و ورود' : 'ورود'}
            </Button>
          </form>
        </CardContent>
      </Card>
    </Centered>
  )
}

function Centered({ children }: { children: ReactNode }) {
  return (
    <main className="bg-muted/40 grid min-h-dvh place-items-center px-4">
      <div className="grid w-full max-w-sm gap-6">
        <div className="flex items-center gap-3">
          <PlateMark className="size-10" />
          <span className="text-lg font-bold">مدیریت باشگاه</span>
        </div>
        {children}
      </div>
    </main>
  )
}
