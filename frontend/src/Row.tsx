import { useState } from 'react'
import { deleteLog, updateLog } from './api'
import type { AlbumData, Log, NutritionData, PersonData, SongData, SongStatus } from './types'

interface RowProps {
  log: Log
  justParsed: boolean
  expanded: boolean
  onToggle: () => void
  onChange: (log: Log) => void
  onDelete: (id: string) => void
  onRate: (log: Log) => void
}

function timeOf(iso: string): string {
  return new Date(iso)
    .toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    .toLowerCase()
    .replace(' ', '')
}

const SONG_STATUS_LABEL: Record<SongStatus, string> = {
  loved: 'loved',
  to_revisit: 'revisit',
  revisited: 'revisited',
}

function summary(log: Log): string {
  switch (log.parsed_type) {
    case 'nutrition':
      return (log.data as NutritionData).food_name
    case 'person':
      return `met ${(log.data as PersonData).name}`
    case 'album': {
      const a = log.data as AlbumData
      return `${a.title}, ${a.artist}`
    }
    case 'song': {
      const s = log.data as SongData
      if (s.title) return s.artist ? `${s.title}, ${s.artist}` : s.title
      return s.context ?? 'a song'
    }
  }
}

function badge(log: Log): { label: string; kind: string } {
  switch (log.parsed_type) {
    case 'nutrition':
      return { label: 'food', kind: 'food' }
    case 'person':
      return { label: 'people', kind: 'people' }
    case 'album':
    case 'song':
      return { label: 'music', kind: 'music' }
  }
}

function rightSide(log: Log, onRate: (log: Log) => void): React.ReactNode {
  switch (log.parsed_type) {
    case 'nutrition':
      return Math.round((log.data as NutritionData).calories)
    case 'album': {
      const a = log.data as AlbumData
      if (a.rating !== null) return a.rating.toFixed(1)
      return (
        <button
          className="rate-link"
          onClick={(e) => {
            e.stopPropagation()
            onRate(log)
          }}
        >
          rate
        </button>
      )
    }
    case 'song':
      return <span className="status-label">{SONG_STATUS_LABEL[(log.data as SongData).status]}</span>
    default:
      return ''
  }
}

export function Row({ log, justParsed, expanded, onToggle, onChange, onDelete, onRate }: RowProps) {
  return (
    <div className={`row-wrap ${expanded ? 'open' : ''}`}>
      <div className={`row ${justParsed ? 'morph' : ''}`} onClick={onToggle}>
        <span className="row-time">{timeOf(log.created_at)}</span>
        <span className={`badge ${badge(log).kind}`}>{badge(log).label}</span>
        <span className="row-main">{summary(log)}</span>
        <span className="row-right">{rightSide(log, onRate)}</span>
      </div>
      <div className="expand">
        <div className="expand-inner">
          {expanded && <Editor log={log} onChange={onChange} onDelete={onDelete} onRate={onRate} />}
        </div>
      </div>
    </div>
  )
}

function Editor({
  log,
  onChange,
  onDelete,
  onRate,
}: EditorProps & { onRate: (log: Log) => void }) {
  switch (log.parsed_type) {
    case 'nutrition':
      return <FoodEditor log={log} onChange={onChange} onDelete={onDelete} />
    case 'person':
      return <PersonEditor log={log} onChange={onChange} onDelete={onDelete} />
    case 'album':
      return <AlbumEditor log={log} onChange={onChange} onDelete={onDelete} onRate={onRate} />
    case 'song':
      return <SongEditor log={log} onChange={onChange} onDelete={onDelete} />
  }
}

interface EditorProps {
  log: Log
  onChange: (log: Log) => void
  onDelete: (id: string) => void
}

function useEditor(log: Log, onChange: (log: Log) => void, onDelete: (id: string) => void) {
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  const save = async (data: Record<string, unknown>) => {
    setSaving(true)
    setError('')
    try {
      onChange(await updateLog(log.id, { data }))
    } catch (e) {
      setError(e instanceof Error ? e.message : 'save failed')
    } finally {
      setSaving(false)
    }
  }

  const remove = async () => {
    setError('')
    try {
      await deleteLog(log.id)
      onDelete(log.id)
    } catch (e) {
      setError(e instanceof Error ? e.message : 'delete failed')
    }
  }

  return { saving, error, save, remove }
}

function FoodEditor({ log, onChange, onDelete }: EditorProps) {
  const data = log.data as NutritionData
  const [fields, setFields] = useState({
    food_name: data.food_name,
    quantity: data.quantity,
    calories: String(data.calories),
    protein_g: String(data.protein_g),
    carbs_g: String(data.carbs_g),
    fat_g: String(data.fat_g),
  })
  const { saving, error, save, remove } = useEditor(log, onChange, onDelete)

  const set = (key: keyof typeof fields) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setFields({ ...fields, [key]: e.target.value })

  const submit = () =>
    save({
      food_name: fields.food_name,
      quantity: fields.quantity,
      calories: Math.round(Number(fields.calories) || 0),
      protein_g: Number(fields.protein_g) || 0,
      carbs_g: Number(fields.carbs_g) || 0,
      fat_g: Number(fields.fat_g) || 0,
    })

  return (
    <div className="editor">
      <label>
        <span>food</span>
        <input value={fields.food_name} onChange={set('food_name')} />
      </label>
      <label>
        <span>quantity</span>
        <input value={fields.quantity} onChange={set('quantity')} />
      </label>
      <div className="editor-grid">
        <label>
          <span>cals</span>
          <input inputMode="numeric" value={fields.calories} onChange={set('calories')} />
        </label>
        <label>
          <span>protein</span>
          <input inputMode="decimal" value={fields.protein_g} onChange={set('protein_g')} />
        </label>
        <label>
          <span>carbs</span>
          <input inputMode="decimal" value={fields.carbs_g} onChange={set('carbs_g')} />
        </label>
        <label>
          <span>fat</span>
          <input inputMode="decimal" value={fields.fat_g} onChange={set('fat_g')} />
        </label>
      </div>
      <EditorFooter
        meta={data.usda_fdc_id ? `usda ${data.usda_fdc_id}` : 'estimated'}
        saving={saving}
        error={error}
        onSave={submit}
        onDelete={remove}
      />
    </div>
  )
}

function PersonEditor({ log, onChange, onDelete }: EditorProps) {
  const data = log.data as PersonData
  const [fields, setFields] = useState({
    name: data.name,
    email: data.email ?? '',
    phone: data.phone ?? '',
    context: data.context,
  })
  const { saving, error, save, remove } = useEditor(log, onChange, onDelete)

  const submit = () =>
    save({
      name: fields.name,
      email: fields.email.trim() || null,
      phone: fields.phone.trim() || null,
      context: fields.context,
    })

  return (
    <div className="editor">
      <label>
        <span>name</span>
        <input value={fields.name} onChange={(e) => setFields({ ...fields, name: e.target.value })} />
      </label>
      <div className="editor-grid two">
        <label>
          <span>email</span>
          <input value={fields.email} onChange={(e) => setFields({ ...fields, email: e.target.value })} />
        </label>
        <label>
          <span>phone</span>
          <input value={fields.phone} onChange={(e) => setFields({ ...fields, phone: e.target.value })} />
        </label>
      </div>
      <label>
        <span>context</span>
        <textarea
          rows={2}
          value={fields.context}
          onChange={(e) => setFields({ ...fields, context: e.target.value })}
        />
      </label>
      <EditorFooter saving={saving} error={error} onSave={submit} onDelete={remove} />
    </div>
  )
}

function AlbumEditor({
  log,
  onChange,
  onDelete,
  onRate,
}: EditorProps & { onRate: (log: Log) => void }) {
  const data = log.data as AlbumData
  const [fields, setFields] = useState({
    artist: data.artist,
    title: data.title,
    thoughts: data.thoughts ?? '',
  })
  const { saving, error, save, remove } = useEditor(log, onChange, onDelete)

  const submit = () =>
    save({
      artist: fields.artist,
      title: fields.title,
      thoughts: fields.thoughts.trim() || null,
    })

  const meta =
    data.rating !== null && data.rating_tier
      ? `${data.rating_tier} ${data.rating.toFixed(1)}`
      : 'unrated'

  return (
    <div className="editor">
      <div className="editor-grid two">
        <label>
          <span>title</span>
          <input value={fields.title} onChange={(e) => setFields({ ...fields, title: e.target.value })} />
        </label>
        <label>
          <span>artist</span>
          <input value={fields.artist} onChange={(e) => setFields({ ...fields, artist: e.target.value })} />
        </label>
      </div>
      <label>
        <span>thoughts</span>
        <textarea
          rows={2}
          value={fields.thoughts}
          onChange={(e) => setFields({ ...fields, thoughts: e.target.value })}
        />
      </label>
      <div className="editor-footer">
        <span className="editor-meta">{error || meta}</span>
        <div className="editor-actions">
          <button className="action" onClick={() => onRate(log)}>
            {data.rating !== null ? 're-rank' : 'rate'}
          </button>
          <button className="action delete" onClick={remove}>
            delete
          </button>
          <button className="action save" onClick={submit} disabled={saving}>
            {saving ? 'saving' : 'save'}
          </button>
        </div>
      </div>
    </div>
  )
}

const SONG_STATUSES: SongStatus[] = ['loved', 'to_revisit', 'revisited']

function SongEditor({ log, onChange, onDelete }: EditorProps) {
  const data = log.data as SongData
  const [fields, setFields] = useState({
    title: data.title ?? '',
    artist: data.artist ?? '',
    context: data.context ?? '',
    source: data.source ?? '',
    thoughts: data.thoughts ?? '',
  })
  const [status, setStatus] = useState<SongStatus>(data.status)
  const { saving, error, save, remove } = useEditor(log, onChange, onDelete)

  const submit = () =>
    save({
      title: fields.title.trim() || null,
      artist: fields.artist.trim() || null,
      status,
      context: fields.context.trim() || null,
      source: fields.source.trim() || null,
      thoughts: fields.thoughts.trim() || null,
    })

  return (
    <div className="editor">
      <div className="editor-grid two">
        <label>
          <span>title</span>
          <input value={fields.title} onChange={(e) => setFields({ ...fields, title: e.target.value })} />
        </label>
        <label>
          <span>artist</span>
          <input value={fields.artist} onChange={(e) => setFields({ ...fields, artist: e.target.value })} />
        </label>
      </div>
      <div className="editor-grid two">
        <label>
          <span>context</span>
          <input value={fields.context} onChange={(e) => setFields({ ...fields, context: e.target.value })} />
        </label>
        <label>
          <span>source</span>
          <input value={fields.source} onChange={(e) => setFields({ ...fields, source: e.target.value })} />
        </label>
      </div>
      <label>
        <span>thoughts</span>
        <textarea
          rows={2}
          value={fields.thoughts}
          onChange={(e) => setFields({ ...fields, thoughts: e.target.value })}
        />
      </label>
      <label>
        <span>status</span>
        <div className="status-buttons">
          {SONG_STATUSES.map((s) => (
            <button
              key={s}
              className={`filter ${status === s ? 'active' : ''}`}
              onClick={() => setStatus(s)}
            >
              {SONG_STATUS_LABEL[s]}
            </button>
          ))}
        </div>
      </label>
      <EditorFooter saving={saving} error={error} onSave={submit} onDelete={remove} />
    </div>
  )
}

function EditorFooter({
  meta,
  saving,
  error,
  onSave,
  onDelete,
}: {
  meta?: string
  saving: boolean
  error: string
  onSave: () => void
  onDelete: () => void
}) {
  return (
    <div className="editor-footer">
      <span className="editor-meta">{error || meta || ''}</span>
      <div className="editor-actions">
        <button className="action delete" onClick={onDelete}>
          delete
        </button>
        <button className="action save" onClick={onSave} disabled={saving}>
          {saving ? 'saving' : 'save'}
        </button>
      </div>
    </div>
  )
}
