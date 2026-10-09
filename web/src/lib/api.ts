// Thin fetch wrapper. The server answers errors as { error, message } with a
// Persian message meant for the UI.

export class ApiError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.status = status
    this.code = code
  }
}

export async function api<T>(method: string, path: string, body?: unknown): Promise<T> {
  if (import.meta.env.VITE_DEMO) {
    // GitHub Pages demo: an in-browser fake server (removed from the real build)
    const { handle } = await import('@/demo/server')
    return (await handle(method, path, body)) as T
  }
  let res: Response
  try {
    res = await fetch(`/api${path}`, {
      method,
      headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    })
  } catch {
    throw new ApiError(0, 'network', 'ارتباط با سرور برقرار نشد. آیا سرویس اجراست؟')
  }
  const text = await res.text()
  const data = text ? JSON.parse(text) : null
  if (!res.ok) {
    throw new ApiError(res.status, data?.error ?? 'unknown', data?.message ?? `خطای ${res.status}`)
  }
  return data as T
}

export const get = <T>(path: string) => api<T>('GET', path)
export const post = <T>(path: string, body?: unknown) => api<T>('POST', path, body ?? {})
export const put = <T>(path: string, body: unknown) => api<T>('PUT', path, body)
