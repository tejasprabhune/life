import type {
  AlbumGroups,
  Category,
  CreateResponse,
  Log,
  RankComparison,
  RankResponse,
  Tier,
} from './types'

const API = import.meta.env.VITE_API_URL ?? 'https://tejas-life-api.fly.dev'
const TOKEN_KEY = 'life_token'

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY)
}

export function setToken(token: string) {
  localStorage.setItem(TOKEN_KEY, token)
}

export function clearToken() {
  localStorage.removeItem(TOKEN_KEY)
}

function authHeaders(): Record<string, string> {
  const token = getToken()
  return token ? { authorization: `Bearer ${token}` } : {}
}

async function check(res: Response): Promise<Response> {
  if (res.status === 401) {
    clearToken()
    window.dispatchEvent(new Event('life-unauthorized'))
    throw new Error('unauthorized')
  }
  if (!res.ok) {
    let message = `request failed (${res.status})`
    try {
      const body = await res.json()
      if (body.error) message = body.error
    } catch {
      // keep the status message
    }
    throw new Error(message)
  }
  return res
}

export async function createLog(rawText: string): Promise<CreateResponse> {
  const res = await fetch(`${API}/api/logs`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...authHeaders() },
    body: JSON.stringify({
      raw_text: rawText,
      tz_offset_min: new Date().getTimezoneOffset(),
    }),
  })
  return (await check(res)).json()
}

export async function listWorkouts(): Promise<Log[]> {
  const res = await fetch(`${API}/api/workouts`, { headers: authHeaders() })
  return (await check(res)).json()
}

export async function listLogs(date: string, category: Category): Promise<Log[]> {
  const params = new URLSearchParams({
    date,
    category,
    tz_offset_min: String(new Date().getTimezoneOffset()),
  })
  const res = await fetch(`${API}/api/logs?${params}`, { headers: authHeaders() })
  return (await check(res)).json()
}

export async function listAlbums(): Promise<AlbumGroups> {
  const res = await fetch(`${API}/api/albums`, { headers: authHeaders() })
  return (await check(res)).json()
}

export async function listSongs(status: string): Promise<Log[]> {
  const res = await fetch(`${API}/api/songs?status=${status}`, { headers: authHeaders() })
  return (await check(res)).json()
}

export async function rankAlbum(
  id: string,
  tier: Tier,
  comparisons: RankComparison[],
): Promise<RankResponse> {
  const res = await fetch(`${API}/api/albums/${id}/rank`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', ...authHeaders() },
    body: JSON.stringify({ tier, comparisons }),
  })
  return (await check(res)).json()
}

export async function updateLog(
  id: string,
  patch: { raw_input?: string; data?: Record<string, unknown> },
): Promise<Log> {
  const res = await fetch(`${API}/api/logs/${id}`, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json', ...authHeaders() },
    body: JSON.stringify(patch),
  })
  return (await check(res)).json()
}

export async function deleteLog(id: string): Promise<void> {
  const res = await fetch(`${API}/api/logs/${id}`, { method: 'DELETE', headers: authHeaders() })
  await check(res)
}
