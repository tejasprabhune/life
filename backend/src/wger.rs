use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveTime, Utc};
use serde::Deserialize;

use crate::models::{WorkoutData, WorkoutExercise, WorkoutSet};

const BASE: &str = "https://wger.de/api/v2";

// Verified against /api/v2/setting-weightunit/.
fn unit_name(id: i64) -> Option<String> {
    let name = match id {
        1 => "kg",
        2 => "lb",
        3 => "bodyweight",
        4 => "plates",
        _ => return None,
    };
    Some(name.to_string())
}

fn impression_label(value: &str) -> Option<String> {
    let label = match value {
        "1" => "bad",
        "2" => "neutral",
        "3" => "good",
        _ => return None,
    };
    Some(label.to_string())
}

fn exercise_cache() -> &'static Mutex<HashMap<i64, String>> {
    static CACHE: OnceLock<Mutex<HashMap<i64, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Deserialize)]
struct Page<T> {
    results: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct Session {
    id: i64,
    date: String,
    notes: Option<String>,
    impression: Option<String>,
    time_start: Option<String>,
    time_end: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SetLog {
    exercise: i64,
    weight: Option<String>,
    repetitions: Option<String>,
    rir: Option<String>,
    rest: Option<i64>,
    weight_unit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ExerciseInfo {
    translations: Vec<Translation>,
}

#[derive(Debug, Deserialize)]
struct Translation {
    language: i64,
    name: String,
}

pub enum Outcome {
    Synced(WorkoutData),
    Notice(String),
}

async fn get_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    api_key: &str,
    path: &str,
) -> Result<T> {
    let resp = http
        .get(format!("{BASE}{path}"))
        .header("Authorization", format!("Token {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("wger returned {status} for {path}"));
    }
    Ok(resp.json().await?)
}

fn parse_num(value: &Option<String>) -> Option<f64> {
    value.as_deref().and_then(|s| s.parse().ok())
}

async fn exercise_name(http: &reqwest::Client, api_key: &str, id: i64) -> String {
    if let Some(name) = exercise_cache().lock().unwrap().get(&id) {
        return name.clone();
    }
    let fetched: Result<ExerciseInfo> = get_json(http, api_key, &format!("/exerciseinfo/{id}/")).await;
    let name = match fetched {
        Ok(info) => info
            .translations
            .iter()
            .find(|t| t.language == 2)
            .or_else(|| info.translations.first())
            .map(|t| t.name.clone())
            .unwrap_or_else(|| format!("exercise {id}")),
        Err(e) => {
            tracing::warn!("exerciseinfo {id} fetch failed: {e}");
            return format!("exercise {id}");
        }
    };
    exercise_cache().lock().unwrap().insert(id, name.clone());
    name
}

fn duration_min(start: &Option<String>, end: &Option<String>) -> Option<i64> {
    let start = NaiveTime::parse_from_str(start.as_deref()?, "%H:%M:%S").ok()?;
    let end = NaiveTime::parse_from_str(end.as_deref()?, "%H:%M:%S").ok()?;
    let minutes = (end - start).num_minutes();
    (minutes > 0).then_some(minutes)
}

/// Pull the latest wger session and build a workout payload. Returns a
/// notice instead when there is nothing suitable to log.
pub async fn sync(
    http: &reqwest::Client,
    api_key: &str,
    tz_offset_min: i32,
    note: Option<String>,
    allow_not_today: bool,
) -> Result<Outcome> {
    let sessions: Page<Session> =
        get_json(http, api_key, "/workoutsession/?ordering=-date&limit=1").await?;
    let Some(session) = sessions.results.into_iter().next() else {
        return Ok(Outcome::Notice("No wger sessions found.".into()));
    };

    let today = (Utc::now() - Duration::minutes(tz_offset_min as i64))
        .date_naive()
        .to_string();
    if session.date != today && !allow_not_today {
        return Ok(Outcome::Notice(format!(
            "Latest wger session is {}, not today. Nothing logged.",
            session.date
        )));
    }

    let logs: Page<SetLog> = get_json(
        http,
        api_key,
        &format!("/workoutlog/?session={}&limit=500", session.id),
    )
    .await?;

    let mut exercises: Vec<WorkoutExercise> = Vec::new();
    for set in &logs.results {
        let parsed = WorkoutSet {
            weight: parse_num(&set.weight),
            reps: parse_num(&set.repetitions).map(|r| r.round() as i64),
            rir: parse_num(&set.rir),
            rest_s: set.rest,
            unit: set.weight_unit.and_then(unit_name),
        };
        match exercises.iter_mut().find(|e| e.exercise_id == set.exercise) {
            Some(entry) => entry.sets.push(parsed),
            None => exercises.push(WorkoutExercise {
                exercise_id: set.exercise,
                name: exercise_name(http, api_key, set.exercise).await,
                sets: vec![parsed],
            }),
        }
    }

    let total_sets = exercises.iter().map(|e| e.sets.len() as i64).sum();
    let volumes: Vec<f64> = exercises
        .iter()
        .flat_map(|e| &e.sets)
        .filter_map(|s| Some(s.weight? * s.reps? as f64))
        .collect();
    let total_volume = (!volumes.is_empty()).then(|| volumes.iter().sum::<f64>().round());

    let note = match (note, exercises.is_empty()) {
        (Some(n), _) => Some(n),
        (None, true) => Some("no sets logged yet".into()),
        (None, false) => None,
    };

    Ok(Outcome::Synced(WorkoutData {
        wger_session_id: session.id,
        date: session.date,
        notes: session.notes.filter(|n| !n.trim().is_empty()),
        note,
        impression: session.impression.as_deref().and_then(impression_label),
        duration_min: duration_min(&session.time_start, &session.time_end),
        exercises,
        total_sets,
        total_volume,
    }))
}
