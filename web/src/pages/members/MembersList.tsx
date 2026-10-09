import { useQuery } from '@tanstack/react-query'
import { ScanFaceIcon, SearchIcon, UserPlusIcon } from 'lucide-react'
import { useState } from 'react'
import { useNavigate } from 'react-router'

import { PageHeader } from '@/components/page'
import { StatusBadge } from '@/components/StatusBadge'
import { Button } from '@/components/ui/button'
import { Card } from '@/components/ui/card'
import { Dialog } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { get } from '@/lib/api'
import { faDigits, num, toman } from '@/lib/format'
import { GENDER_LABEL, type CurrentSub, type MemberRow } from '@/lib/types'
import { MemberForm } from './MemberForm'

const ALL = 'all'

export function remainingText(c: CurrentSub | null): string {
  if (!c) return '—'
  const parts = []
  if (c.sessions_remaining !== null) parts.push(`${num(c.sessions_remaining)} جلسه`)
  if (c.days_remaining !== null) parts.push(`${num(c.days_remaining)} روز`)
  return parts.join(' / ')
}

export function MembersList() {
  const navigate = useNavigate()
  const [q, setQ] = useState('')
  const [status, setStatus] = useState(ALL)
  const [gender, setGender] = useState(ALL)
  const [archived, setArchived] = useState(false)
  const [creating, setCreating] = useState(false)

  const params = new URLSearchParams()
  if (q.trim()) params.set('q', q.trim())
  if (status !== ALL) params.set('status', status)
  if (gender !== ALL) params.set('gender', gender)
  if (archived) params.set('archived', 'true')
  const list = useQuery({ queryKey: ['members', params.toString()], queryFn: () => get<MemberRow[]>(`/members?${params}`), placeholderData: (p) => p })

  return (
    <>
      <PageHeader title="اعضا" description={list.data ? `${num(list.data.length)} نفر` : undefined}>
        <Button onClick={() => setCreating(true)}>
          <UserPlusIcon />
          عضو جدید
        </Button>
      </PageHeader>

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <div className="relative w-72">
          <SearchIcon className="text-muted-foreground absolute inset-y-0 start-3 my-auto size-4" />
          <Input className="ps-9" placeholder="جستجوی نام یا موبایل" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <Select value={status} onValueChange={setStatus}>
          <SelectTrigger className="w-40">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>همه‌ی وضعیت‌ها</SelectItem>
            <SelectItem value="ok">شهریه دارد</SelectItem>
            <SelectItem value="alert">پایان شهریه یا بدهی</SelectItem>
            <SelectItem value="debt">بدهکار</SelectItem>
            <SelectItem value="expired">پایان شهریه</SelectItem>
            <SelectItem value="none">بدون اشتراک</SelectItem>
          </SelectContent>
        </Select>
        <Select value={gender} onValueChange={setGender}>
          <SelectTrigger className="w-32">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={ALL}>آقا و خانم</SelectItem>
            <SelectItem value="male">آقایان</SelectItem>
            <SelectItem value="female">خانم‌ها</SelectItem>
          </SelectContent>
        </Select>
        <Button variant={archived ? 'secondary' : 'ghost'} size="sm" onClick={() => setArchived((a) => !a)}>
          {archived ? 'نمایش اعضای فعال' : 'بایگانی‌شده‌ها'}
        </Button>
      </div>

      <Card className="py-2">
        {list.data && list.data.length === 0 ? (
          <p className="text-muted-foreground py-14 text-center">
            {q || status !== ALL || gender !== ALL ? 'عضوی با این فیلترها پیدا نشد.' : archived ? 'عضو بایگانی‌شده‌ای وجود ندارد.' : 'هنوز عضوی ثبت نشده.'}
          </p>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>نام</TableHead>
                <TableHead>موبایل</TableHead>
                <TableHead>وضعیت</TableHead>
                <TableHead>اشتراک فعلی</TableHead>
                <TableHead>باقی‌مانده</TableHead>
                <TableHead>بدهی</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.data?.map((m) => (
                <TableRow key={m.id} className="cursor-pointer" onClick={() => navigate(`/members/${m.id}`)}>
                  <TableCell className="font-medium">
                    <span className="flex items-center gap-2">
                      {m.full_name}
                      <span className="text-muted-foreground text-xs font-normal">{GENDER_LABEL[m.gender]}</span>
                      {!m.face_enrolled && <ScanFaceIcon className="text-warning size-4" aria-label="چهره ثبت نشده" />}
                    </span>
                  </TableCell>
                  <TableCell dir="ltr" className="text-end tabular-nums">
                    {faDigits(m.phone)}
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={m.status} />
                  </TableCell>
                  <TableCell>{m.current?.plan_name ?? '—'}</TableCell>
                  <TableCell>{remainingText(m.current)}</TableCell>
                  <TableCell className={m.debt ? 'text-destructive' : 'text-muted-foreground'}>{m.debt ? toman(m.debt) : '—'}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </Card>

      <Dialog open={creating} onOpenChange={setCreating}>
        {creating && (
          <MemberForm
            member={null}
            onDone={(m) => {
              setCreating(false)
              navigate(`/members/${m.id}?step=face`)
            }}
          />
        )}
      </Dialog>
    </>
  )
}
