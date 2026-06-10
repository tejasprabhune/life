import { useEffect, useState } from 'react'
import { listWorkouts } from './api'
import { WorkoutBreakdown, workoutSummary, workoutVolume } from './Row'
import type { Log, WorkoutData } from './types'

export function Gym() {
  const [workouts, setWorkouts] = useState<Log[]>([])
  const [expandedId, setExpandedId] = useState<string | null>(null)

  useEffect(() => {
    listWorkouts()
      .then(setWorkouts)
      .catch(() => {})
  }, [])

  return (
    <div className="app">
      <header>
        <h1 className="brand">gym</h1>
        <a className="guide-link" href="#/">
          back
        </a>
      </header>

      <main className="list">
        {workouts.map((workout) => {
          const data = workout.data as WorkoutData
          const open = expandedId === workout.id
          return (
            <div key={workout.id} className={`row-wrap ${open ? 'open' : ''}`}>
              <div
                className="row"
                onClick={() => setExpandedId(open ? null : workout.id)}
              >
                <span className="row-time">{data.date.slice(5)}</span>
                <span className="row-main">{workoutSummary(data)}</span>
                <span className="row-right">{workoutVolume(data)}</span>
              </div>
              <div className="expand">
                <div className="expand-inner">
                  {open && (
                    <div className="editor">
                      <WorkoutBreakdown data={data} />
                      {data.note && <p className="workout-notes">{data.note}</p>}
                    </div>
                  )}
                </div>
              </div>
            </div>
          )
        })}
        {workouts.length === 0 && <div className="empty">no workouts logged</div>}
      </main>
    </div>
  )
}
