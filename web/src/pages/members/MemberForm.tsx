import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'

import { JalaliDateInput } from '@/components/inputs'
import { Field } from '@/components/page'
import { Button } from '@/components/ui/button'
import { DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group'
import { Textarea } from '@/components/ui/textarea'
import { ApiError, post, put } from '@/lib/api'
import { GENDER_LABEL, type Gender, type MemberDetail } from '@/lib/types'

/** Create (member = null) or edit a member's details. */
export function MemberForm({ member, onDone }: { member: MemberDetail | null; onDone: (m: MemberDetail) => void }) {
  const qc = useQueryClient()
  const [fullName, setFullName] = useState(member?.full_name ?? '')
  const [phone, setPhone] = useState(member?.phone ?? '')
  const [birth, setBirth] = useState<string | null>(member?.birth_date ?? null)
  const [gender, setGender] = useState<Gender | ''>(member?.gender ?? '')
  const [notes, setNotes] = useState(member?.notes ?? '')

  const save = useMutation({
    mutationFn: () => {
      const body = { full_name: fullName, phone, birth_date: birth ?? '', gender, notes }
      return member ? put<MemberDetail>(`/members/${member.id}`, body) : post<MemberDetail>('/members', body)
    },
    onSuccess: (m) => {
      void qc.invalidateQueries({ queryKey: ['members'] })
      qc.setQueryData(['member', m.id], m)
      toast.success(member ? 'اطلاعات ذخیره شد' : `${m.full_name} ثبت شد`)
      onDone(m)
    },
    onError: (e) => toast.error(e instanceof ApiError ? e.message : String(e)),
  })

  return (
    <DialogContent>
      <DialogHeader>
        <DialogTitle>{member ? 'ویرایش اطلاعات عضو' : 'عضو جدید'}</DialogTitle>
        {!member && <DialogDescription>ثبت چهره بعد از ذخیره، از صفحه‌ی عضو انجام می‌شود.</DialogDescription>}
      </DialogHeader>
      <form
        className="grid gap-5"
        onSubmit={(e: FormEvent) => {
          e.preventDefault()
          save.mutate()
        }}
      >
        <Field label="نام کامل" htmlFor="m-name">
          <Input id="m-name" value={fullName} onChange={(e) => setFullName(e.target.value)} autoFocus />
        </Field>
        <div className="grid gap-5 sm:grid-cols-2">
          <Field label="موبایل" htmlFor="m-phone">
            <Input id="m-phone" dir="ltr" inputMode="tel" className="text-end" placeholder="۰۹۱۲۱۲۳۴۵۶۷" value={phone} onChange={(e) => setPhone(e.target.value)} />
          </Field>
          <Field label="تاریخ تولد" htmlFor="m-birth">
            <JalaliDateInput id="m-birth" value={birth} onChange={setBirth} />
          </Field>
        </div>
        <div className="grid gap-3">
          <Label>جنسیت</Label>
          <RadioGroup value={gender} onValueChange={(v) => setGender(v as Gender)} className="flex gap-6">
            {(['male', 'female'] as const).map((g) => (
              <div key={g} className="flex items-center gap-2">
                <RadioGroupItem value={g} id={`g-${g}`} />
                <Label htmlFor={`g-${g}`} className="font-normal">
                  {GENDER_LABEL[g]}
                </Label>
              </div>
            ))}
          </RadioGroup>
        </div>
        <Field label="یادداشت" htmlFor="m-notes">
          <Textarea id="m-notes" value={notes} onChange={(e) => setNotes(e.target.value)} placeholder="اختیاری" />
        </Field>
        <DialogFooter>
          <Button type="submit" disabled={save.isPending || !fullName || !phone || !birth || !gender}>
            {member ? 'ذخیره' : 'ثبت عضو'}
          </Button>
        </DialogFooter>
      </form>
    </DialogContent>
  )
}
