use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;

use crate::models::Log;
use crate::rank::{self, RankGroups};
use crate::routes::AppError;
use crate::AppState;

pub async fn list_albums(State(state): State<AppState>) -> Result<Json<RankGroups>, AppError> {
    Ok(Json(rank::groups(&state, "album", None).await?))
}

#[derive(Debug, Deserialize)]
pub struct SongQuery {
    pub status: Option<String>,
}

pub async fn list_songs(
    State(state): State<AppState>,
    Query(query): Query<SongQuery>,
) -> Result<Json<Vec<Log>>, AppError> {
    let status = query.status.unwrap_or_else(|| "all".into());
    if !matches!(status.as_str(), "all" | "loved" | "to_revisit" | "revisited") {
        return Err(AppError::BadRequest(
            "status must be loved, to_revisit, revisited or all".into(),
        ));
    }
    let songs: Vec<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = 'song' AND deleted_at IS NULL \
         AND ($1 = 'all' OR data->>'status' = $1) \
         ORDER BY created_at DESC",
    )
    .bind(status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(songs))
}
