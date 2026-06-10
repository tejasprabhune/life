use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::models::{Action, CreateLog, ListQuery, Log, UpdateLog};
use crate::{groq, wger, AppState};

pub enum AppError {
    NotFound,
    BadRequest(String),
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError::Internal(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Internal(e) => {
                tracing::error!("internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

const LOG_COLUMNS: &str = "id, created_at, raw_input, parsed_type, data";

#[derive(serde::Serialize)]
pub struct CreateResponse {
    pub logs: Vec<Log>,
    pub notice: Option<String>,
}

async fn insert_log(state: &AppState, raw: &str, parsed_type: &str, data: serde_json::Value) -> Result<Log, AppError> {
    let log: Log = sqlx::query_as(
        "INSERT INTO logs (raw_input, parsed_type, data) VALUES ($1, $2, $3) \
         RETURNING id, created_at, raw_input, parsed_type, data",
    )
    .bind(raw)
    .bind(parsed_type)
    .bind(data)
    .fetch_one(&state.pool)
    .await?;
    Ok(log)
}

/// Insert the workout, or update the existing entry for the same wger
/// session so saying "worked out" twice does not duplicate it.
async fn upsert_workout(
    state: &AppState,
    raw: &str,
    data: &crate::models::WorkoutData,
) -> Result<Log, AppError> {
    let existing: Option<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = 'workout' AND deleted_at IS NULL \
         AND (data->>'wger_session_id')::bigint = $1",
    )
    .bind(data.wger_session_id)
    .fetch_optional(&state.pool)
    .await?;

    let payload = serde_json::to_value(data).unwrap();
    match existing {
        Some(log) => {
            // Keep an earlier user note when the re-sync does not bring one.
            let mut payload = payload;
            if payload.get("note").map(|n| n.is_null()).unwrap_or(true) {
                if let Some(old) = log.data.get("note").filter(|n| !n.is_null()) {
                    payload["note"] = old.clone();
                }
            }
            let updated: Log = sqlx::query_as(
                "UPDATE logs SET data = $2 WHERE id = $1 \
                 RETURNING id, created_at, raw_input, parsed_type, data",
            )
            .bind(log.id)
            .bind(payload)
            .fetch_one(&state.pool)
            .await?;
            Ok(updated)
        }
        None => insert_log(state, raw, "workout", payload).await,
    }
}

pub async fn create_log(
    State(state): State<AppState>,
    Json(body): Json<CreateLog>,
) -> Result<Json<CreateResponse>, AppError> {
    let raw = body.raw_text.trim();
    if raw.is_empty() {
        return Err(AppError::BadRequest("raw_text is empty".into()));
    }
    let tz_offset = body.tz_offset_min.unwrap_or(0);

    let actions = groq::parse(&state.http, &state.groq_key, &state.usda_key, raw).await?;

    let mut logs = Vec::with_capacity(actions.len());
    let mut notices = Vec::new();
    for action in actions {
        match action {
            Action::Entry(entry) => {
                logs.push(insert_log(&state, raw, entry.type_name(), entry.to_json()).await?);
            }
            Action::Workout { note, allow_not_today } => {
                let Some(wger_key) = state.wger_key.as_deref() else {
                    notices.push("wger is not configured.".to_string());
                    continue;
                };
                match wger::sync(&state.http, wger_key, tz_offset, note, allow_not_today).await {
                    Ok(wger::Outcome::Synced(data)) => {
                        logs.push(upsert_workout(&state, raw, &data).await?);
                    }
                    Ok(wger::Outcome::Notice(msg)) => notices.push(msg),
                    Err(e) => {
                        tracing::warn!("wger sync failed: {e:#}");
                        notices.push("Could not reach wger, try again.".to_string());
                    }
                }
            }
        }
    }

    let notice = (!notices.is_empty()).then(|| notices.join(" "));
    Ok(Json(CreateResponse { logs, notice }))
}

pub async fn list_workouts(State(state): State<AppState>) -> Result<Json<Vec<Log>>, AppError> {
    let workouts: Vec<Log> = sqlx::query_as(&format!(
        "SELECT {LOG_COLUMNS} FROM logs \
         WHERE parsed_type = 'workout' AND deleted_at IS NULL \
         ORDER BY data->>'date' DESC, created_at DESC"
    ))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(workouts))
}

pub async fn list_logs(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Log>>, AppError> {
    let offset_min = query.tz_offset_min.unwrap_or(0);
    if !(-900..=900).contains(&offset_min) {
        return Err(AppError::BadRequest("tz_offset_min out of range".into()));
    }

    // tz_offset_min uses the JS getTimezoneOffset convention: minutes to add
    // to local time to reach UTC (PDT is 420).
    let local_date = match &query.date {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest("date must be YYYY-MM-DD".into()))?,
        None => (Utc::now() - Duration::minutes(offset_min as i64)).date_naive(),
    };
    let start = Utc.from_utc_datetime(&local_date.and_hms_opt(0, 0, 0).unwrap())
        + Duration::minutes(offset_min as i64);
    let end = start + Duration::days(1);

    let category = query.category.as_deref().unwrap_or("all");
    if !matches!(category, "all" | "nutrition" | "person" | "album" | "song") {
        return Err(AppError::BadRequest(
            "category must be nutrition, person, album, song or all".into(),
        ));
    }

    let logs: Vec<Log> = sqlx::query_as(&format!(
        "SELECT {LOG_COLUMNS} FROM logs \
         WHERE deleted_at IS NULL AND created_at >= $1 AND created_at < $2 \
         AND ($3 = 'all' OR parsed_type = $3) \
         ORDER BY created_at DESC"
    ))
    .bind(start)
    .bind(end)
    .bind(category)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(logs))
}

pub async fn get_log(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Log>, AppError> {
    let log: Option<Log> = sqlx::query_as(&format!(
        "SELECT {LOG_COLUMNS} FROM logs WHERE id = $1 AND deleted_at IS NULL"
    ))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    log.map(Json).ok_or(AppError::NotFound)
}

pub async fn update_log(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateLog>,
) -> Result<Json<Log>, AppError> {
    if let Some(data) = &body.data {
        if !data.is_object() {
            return Err(AppError::BadRequest("data must be an object".into()));
        }
    }

    let log: Option<Log> = sqlx::query_as(&format!(
        "UPDATE logs SET \
            data = data || COALESCE($2, '{{}}'::jsonb), \
            raw_input = COALESCE($3, raw_input) \
         WHERE id = $1 AND deleted_at IS NULL \
         RETURNING {LOG_COLUMNS}"
    ))
    .bind(id)
    .bind(body.data)
    .bind(body.raw_input)
    .fetch_optional(&state.pool)
    .await?;

    log.map(Json).ok_or(AppError::NotFound)
}

pub async fn delete_log(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("UPDATE logs SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .execute(&state.pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn health() -> &'static str {
    "ok"
}
