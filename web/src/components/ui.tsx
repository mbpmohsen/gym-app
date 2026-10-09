// Small shared building blocks. Kept deliberately few.

import type { ButtonHTMLAttributes, InputHTMLAttributes, ReactNode } from 'react'

type Tone = 'primary' | 'quiet' | 'danger'

const tones: Record<Tone, string> = {
  primary: 'bg-plate-blue text-white hover:brightness-110 disabled:opacity-50',
  quiet: 'bg-paper text-rubber border border-line hover:bg-chalk disabled:opacity-50',
  danger: 'bg-plate-red text-white hover:brightness-110 disabled:opacity-50',
}

export function Button({ tone = 'primary', className = '', ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { tone?: Tone }) {
  return <button {...p} className={`h-10 rounded-md px-4 text-sm font-medium transition ${tones[tone]} ${className}`} />
}

export function Field({ label, hint, error, children }: { label: string; hint?: string; error?: string; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-sm font-medium">{label}</span>
      {children}
      {error ? (
        <span className="mt-1 block text-sm text-plate-red">{error}</span>
      ) : hint ? (
        <span className="mt-1 block text-sm text-rubber-soft">{hint}</span>
      ) : null}
    </label>
  )
}

export function Input(p: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...p}
      className={`h-10 w-full rounded-md border border-line bg-paper px-3 text-base outline-none focus:border-plate-blue ${p.className ?? ''}`}
    />
  )
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className="mb-6 flex items-center justify-between gap-4">
      <h1 className="text-2xl font-bold">{title}</h1>
      {children}
    </header>
  )
}

/** Message bar; its leading (right, in RTL) edge carries the plate color of its meaning. */
export function Notice({ tone, children }: { tone: 'ok' | 'warn' | 'error'; children: ReactNode }) {
  const edge = { ok: 'border-plate-green', warn: 'border-plate-yellow', error: 'border-plate-red' }[tone]
  return <div className={`rounded-md border-s-4 ${edge} bg-paper px-4 py-3 text-sm`}>{children}</div>
}
