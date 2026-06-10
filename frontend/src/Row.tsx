import { useState } from 'react'
import { deleteLog, updateLog } from './api'
import type { Log, NutritionData, PersonData } from './types'

interface RowProps {
  log: Log
  justParsed: boolean
  expanded: boolean
  onToggle: () => void
  onChange: (log: Log) => void
  onDelete: (id: string) => void
}

function timeOf(iso: string): string {
  return new Date(iso)
    .toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
    .toLowerCase()
    .replace(' ', '')
}

export function Row({ log, justParsed, expanded, onToggle, onChange, onDelete }: RowProps) {
  const isFood = log.parsed_type === 'nutrition'
  const food = log.data as NutritionData
  const person = log.data as PersonData

  return (
    <div className={`row-wrap ${expanded ? 'open' : ''}`}>
      <div className={`row ${justParsed ? 'morph' : ''}`} onClick={onToggle}>
        <span className="row-time">{timeOf(log.created_at)}</span>
        <span className="row-main">{isFood ? food.food_name : `met ${person.name}`}</span>
        <span className="row-right">{isFood ? Math.round(food.calories) : ''}</span>
      </div>
      <div className="expand">
        <div className="expand-inner">
          {expanded &&
            (isFood ? (
              <FoodEditor log={log} onChange={onChange} onDelete={onDelete} />
            ) : (
              <PersonEditor log={log} onChange={onChange} onDelete={onDelete} />
            ))}
        </div>
      </div>
    </div>
  )
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
