// Domain inputs: Jalali date and toman amount. Both accept Persian digits.

import { useEffect, useState } from 'react'

import { Input } from '@/components/ui/input'
import { jalaliDate, num, weekday } from '@/lib/format'
import { asciiDigits, isoToJalali, jalaliToIso } from '@/lib/jalali'

/**
 * Typed Jalali date (۱۴۰۵/۰۷/۱۷). `value`/`onChange` speak ISO ("2026-10-09");
 * onChange(null) while the text isn't a valid date.
 */
export function JalaliDateInput({
  id,
  value,
  onChange,
  invalid,
}: {
  id?: string
  value: string | null
  onChange: (iso: string | null) => void
  invalid?: boolean
}) {
  const [text, setText] = useState(value ? isoToJalali(value) : '')
  // follow outside changes (e.g. a default arriving from the server)
  useEffect(() => {
    if (value && jalaliToIso(text) !== value) setText(isoToJalali(value))
  }, [value])

  const iso = jalaliToIso(text)
  return (
    <div className="grid gap-1">
      <Input
        id={id}
        dir="ltr"
        inputMode="numeric"
        placeholder="۱۴۰۵/۰۷/۱۷"
        className="text-end tabular-nums"
        value={text}
        aria-invalid={invalid || (text !== '' && !iso)}
        onChange={(e) => {
          setText(e.target.value)
          onChange(jalaliToIso(e.target.value))
        }}
      />
      <span className="text-muted-foreground min-h-5 text-xs">
        {iso ? `${weekday(iso)} ${jalaliDate(iso)}` : text ? 'تاریخ را به شکل سال/ماه/روز وارد کنید' : ''}
      </span>
    </div>
  )
}

/** Toman amount with thousands separators while typing. */
export function MoneyInput({ id, value, onChange, invalid }: { id?: string; value: number; onChange: (n: number) => void; invalid?: boolean }) {
  return (
    <div className="relative">
      <Input
        id={id}
        dir="ltr"
        inputMode="numeric"
        className="pe-14 text-end tabular-nums"
        value={value ? num(value) : ''}
        placeholder="۰"
        aria-invalid={invalid}
        onChange={(e) => {
          const n = Number(asciiDigits(e.target.value).replace(/[^\d]/g, '') || 0)
          if (Number.isSafeInteger(n)) onChange(n)
        }}
      />
      <span className="text-muted-foreground pointer-events-none absolute inset-y-0 end-3 flex items-center text-sm">تومان</span>
    </div>
  )
}
