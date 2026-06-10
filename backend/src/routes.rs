use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use crate::models::{CreateLog, ListQuery, Log, UpdateLog};
use crate::{groq, AppState};

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

pub async fn create_log(
    State(state): State<AppState>,
    Json(body): Json<CreateLog>,
) -> Result<Json<Vec<Log>>, AppError> {
    let raw = body.raw_text.trim();
    if raw.is_empty() {
        return Err(AppError::BadRequest("raw_text is empty".into()));
    }

    let parsed = groq::parse(&state.http, &state.groq_key, &state.usda_key, raw).await?;

    let mut tx = state.pool.begin().await?;
    let mut logs = Vec::with_capacity(parsed.len());
    for entry in &parsed {
        let log: Log = sqlx::query_as(
            "INSERT INTO logs (raw_input, parsed_type, data) VALUES ($1, $2, $3) \
             RETURNING id, created_at, raw_input, parsed_type, data",
        )
        .bind(raw)
        .bind(entry.type_name())
        .bind(entry.to_json())
        .fetch_one(&mut *tx)
        .await?;
        logs.push(log);
    }
    tx.commit().await?;

    Ok(Json(logs))
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
