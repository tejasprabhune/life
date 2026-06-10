import { useCallback, useEffect, useRef, useState } from 'react'
import { createLog, listLogs } from './api'
import type { Category, Log, PendingLog } from './types'
import { Row } from './Row'
import { Guide } from './Guide'

function localDateStr(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

function shiftDate(date: string, days: number): string {
  const d = new Date(date + 'T12:00:00')
  d.setDate(d.getDate() + days)
  return localDateStr(d)
}

function dateLabel(date: string): string {
  const today = localDateStr(new Date())
  if (date === today) return 'today'
  if (date === shiftDate(today, -1)) return 'yesterday'
  return new Date(date + 'T12:00:00')
    .toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })
    .toLowerCase()
}

function useHashRoute(): string {
  const [hash, setHash] = useState(window.location.hash)
  useEffect(() => {
    const onChange = () => setHash(window.location.hash)
    window.addEventListener('hashchange', onChange)
    return () => window.removeEventListener('hashchange', onChange)
  }, [])
  return hash
}

const FILTERS: { value: Category; label: string }[] = [
  { value: 'all', label: 'all' },
  { value: 'nutrition', label: 'food' },
  { value: 'person', label: 'people' },
]

export default function App() {
  const route = useHashRoute()
  if (route.startsWith('#/guide')) return <Guide />
  return <Home />
}

function Home() {
  const [date, setDate] = useState(() => localDateStr(new Date()))
  const [category, setCategory] = useState<Category>('all')
  const [logs, setLogs] = useState<Log[]>([])
  const [pendings, setPendings] = useState<PendingLog[]>([])
  const [justParsed, setJustParsed] = useState<Set<string>>(new Set())
  const [expandedId, setExpandedId] = useState<string | null>(null)
  const [text, setText] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const today = localDateStr(new Date())
  const isToday = date === today

  const refresh = useCallback(async (d: string) => {
    try {
      setLogs(await listLogs(d, 'all'))
    } catch {
      // a failed fetch leaves the previous list; logging still works
    }
  }, [])

  useEffect(() => {
    void refresh(date)
  }, [date, refresh])

  const submit = async (rawText: string, tempId?: string) => {
    const id = tempId ?? `tmp-${Math.random().toString(36).slice(2)}`
    setPendings((p) => [
      { tempId: id, raw_input: rawText, failed: false },
      ...p.filter((x) => x.tempId !== id),
    ])
    if (!isToday) setDate(today)
    try {
      const created = await createLog(rawText)
      const createdIds = new Set(created.map((x) => x.id))
      setPendings((p) => p.filter((x) => x.tempId !== id))
      setLogs((l) => [...created, ...l.filter((x) => !createdIds.has(x.id))])
      setJustParsed((s) => {
        const next = new Set(s)
        created.forEach((x) => next.add(x.id))
        return next
      })
      setTimeout(() => {
        setJustParsed((s) => {
          const next = new Set(s)
          createdIds.forEach((x) => next.delete(x))
          return next
        })
      }, 500)
    } catch {
      setPendings((p) =>
        p.map((x) => (x.tempId === id ? { ...x, failed: true } : x)),
      )
    }
  }

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== 'Enter') return
    const value = text.trim()
    if (!value) return
    setText('')
    void submit(value)
  }

  const visible = category === 'all' ? logs : logs.filter((l) => l.parsed_type === category)
  const totalCals = logs
    .filter((l) => l.parsed_type === 'nutrition')
    .reduce((sum, l) => sum + (Number((l.data as { calories?: number }).calories) || 0), 0)

  return (
    <div className="app">
      <header>
        <h1 className="brand">life</h1>
        <a className="guide-link" href="#/guide">
          guide
        </a>
      </header>

      <input
        ref={inputRef}
        className="entry-input"
        type="text"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder="write anything..."
        autoFocus
        enterKeyHint="send"
      />

      <div className="dateline">
        <div className="datenav">
          <button className="chev" onClick={() => setDate(shiftDate(date, -1))} aria-label="previous day">
            &lsaquo;
          </button>
          <span className="datelabel">{dateLabel(date)}</span>
          <button
            className="chev"
            onClick={() => setDate(shiftDate(date, 1))}
            disabled={isToday}
            aria-label="next day"
          >
            &rsaquo;
          </button>
        </div>
        <div className="filters">
          {FILTERS.map((f) => (
            <button
              key={f.value}
              className={`filter ${category === f.value ? 'active' : ''}`}
              onClick={() => setCategory(f.value)}
            >
              {f.label}
            </button>
          ))}
        </div>
        <span className="total">
          cals <span className="total-num">{Math.round(totalCals)}</span>
        </span>
      </div>

      <main className="list">
        {isToday &&
          pendings.map((p) => (
            <div
              key={p.tempId}
              className={`row pending ${p.failed ? 'failed' : ''}`}
              onClick={() => p.failed && void submit(p.raw_input, p.tempId)}
            >
              <span className="row-main">{p.raw_input}</span>
              <span className="row-right">
                {p.failed ? (
                  <>
                    retry
                    <button
                      className="dismiss"
                      onClick={(e) => {
                        e.stopPropagation()
                        setPendings((x) => x.filter((y) => y.tempId !== p.tempId))
                      }}
                      aria-label="dismiss"
                    >
                      &times;
                    </button>
                  </>
                ) : (
                  '...'
                )}
              </span>
            </div>
          ))}
        {visible.map((log) => (
          <Row
            key={log.id}
            log={log}
            justParsed={justParsed.has(log.id)}
            expanded={expandedId === log.id}
            onToggle={() => setExpandedId(expandedId === log.id ? null : log.id)}
            onChange={(updated) =>
              setLogs((l) => l.map((x) => (x.id === updated.id ? updated : x)))
            }
            onDelete={(id) => setLogs((l) => l.filter((x) => x.id !== id))}
          />
        ))}
        {visible.length === 0 && pendings.length === 0 && (
          <div className="empty">nothing logged</div>
        )}
      </main>
    </div>
  )
}
