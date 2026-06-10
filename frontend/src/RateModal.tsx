import { useState } from 'react'
import { rankAlbum } from './api'
import type { AlbumData, Log, Opponent, RankComparison, Tier } from './types'

interface RateModalProps {
  album: Log
  onClose: (rated: boolean) => void
}

const TIERS: { value: Tier; label: string }[] = [
  { value: 'loved', label: 'loved it' },
  { value: 'fine', label: 'it was fine' },
  { value: 'disliked', label: 'disliked it' },
]

export function RateModal({ album, onClose }: RateModalProps) {
  const data = album.data as AlbumData
  const [tier, setTier] = useState<Tier | null>(null)
  const [comparisons, setComparisons] = useState<RankComparison[]>([])
  const [opponent, setOpponent] = useState<Opponent | null>(null)
  const [rating, setRating] = useState<number | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  const step = async (t: Tier, comps: RankComparison[]) => {
    setBusy(true)
    setError('')
    try {
      const res = await rankAlbum(album.id, t, comps)
      if (res.done) {
        setRating(res.rating)
        setTimeout(() => onClose(true), 1100)
      } else {
        setOpponent(res.next_opponent)
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'ranking failed')
      setTier(null)
      setComparisons([])
      setOpponent(null)
    } finally {
      setBusy(false)
    }
  }

  const pickTier = (t: Tier) => {
    setTier(t)
    void step(t, [])
  }

  const choose = (preferred: 'this' | 'that') => {
    if (!tier || !opponent || busy) return
    const next = [...comparisons, { opponent_id: opponent.id, preferred }]
    setComparisons(next)
    void step(tier, next)
  }

  return (
    <div className="modal-overlay" onClick={() => rating === null && onClose(false)}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        {rating !== null ? (
          <div className="modal-card result" key="result">
            <span className="modal-album">
              {data.title}, {data.artist}
            </span>
            <span className="modal-rating">{rating.toFixed(1)}</span>
          </div>
        ) : tier === null ? (
          <div className="modal-card" key="tier">
            <span className="modal-album">
              {data.title}, {data.artist}
            </span>
            <div className="tier-buttons">
              {TIERS.map((t) => (
                <button key={t.value} className="tier-btn" onClick={() => pickTier(t.value)}>
                  {t.label}
                </button>
              ))}
            </div>
            {error && <span className="modal-error">{error}</span>}
          </div>
        ) : (
          <div className="modal-card" key={comparisons.length}>
            <span className="modal-question">which did you prefer?</span>
            <div className="versus">
              <button className="versus-btn" disabled={busy} onClick={() => choose('this')}>
                <span className="versus-title">{data.title}</span>
                <span className="versus-artist">{data.artist}</span>
              </button>
              {opponent && (
                <button className="versus-btn" disabled={busy} onClick={() => choose('that')}>
                  <span className="versus-title">{opponent.title}</span>
                  <span className="versus-artist">{opponent.artist}</span>
                </button>
              )}
            </div>
            {error && <span className="modal-error">{error}</span>}
          </div>
        )}
      </div>
    </div>
  )
}
