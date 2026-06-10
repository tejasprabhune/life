use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::models::Log;
use crate::routes::AppError;
use crate::AppState;

const TIERS: &[(&str, f64, f64)] = &[
    ("loved", 6.8, 10.0),
    ("fine", 3.4, 6.7),
    ("disliked", 0.0, 3.3),
];

fn band(tier: &str) -> Option<(f64, f64)> {
    TIERS.iter().find(|(t, _, _)| *t == tier).map(|(_, lo, hi)| (*lo, *hi))
}

/// Spread positions evenly across the tier's band. Position 0 is the best
/// album in the tier and gets the top of the band.
fn score(position: usize, count: usize, band: (f64, f64)) -> f64 {
    let (lo, hi) = band;
    let raw = if count <= 1 {
        (lo + hi) / 2.0
    } else {
        hi - (hi - lo) * position as f64 / (count - 1) as f64
    };
    (raw * 10.0).round() / 10.0
}

#[derive(Debug, Deserialize)]
pub struct RankRequest {
    pub tier: String,
    #[serde(default)]
    pub comparisons: Vec<Comparison>,
}

#[derive(Debug, Deserialize)]
pub struct Comparison {
    pub opponent_id: Uuid,
    pub preferred: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RankResponse {
    NeedMore { done: bool, next_opponent: Opponent },
    Done { done: bool, rating: f64, rank_position: i64 },
}

#[derive(Debug, Serialize)]
pub struct Opponent {
    pub id: Uuid,
    pub artist: String,
    pub title: String,
}

fn album_field(log: &Log, key: &str) -> String {
    log.data.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

async fn tier_albums(state: &AppState, tier: &str, exclude: Uuid) -> Result<Vec<Log>, AppError> {
    let albums: Vec<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = 'album' AND deleted_at IS NULL \
         AND data->>'rating_tier' = $1 AND id != $2 \
         ORDER BY (data->>'rank_position')::int",
    )
    .bind(tier)
    .bind(exclude)
    .fetch_all(&state.pool)
    .await?;
    Ok(albums)
}

async fn apply_tier(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    ordered: &[Uuid],
    tier: &str,
    tier_band: (f64, f64),
) -> Result<(), AppError> {
    for (position, id) in ordered.iter().enumerate() {
        let patch = json!({
            "rating": score(position, ordered.len(), tier_band),
            "rating_tier": tier,
            "rank_position": position as i64,
        });
        sqlx::query("UPDATE logs SET data = data || $2 WHERE id = $1")
            .bind(id)
            .bind(patch)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Stateless Beli-style placement. The client resends the full comparison
/// history each call; the server replays it to reconstruct binary-search
/// bounds over the tier's ordered list and either asks for the next
/// comparison or finalizes the position and respreads tier scores.
pub async fn rank_album(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RankRequest>,
) -> Result<Json<RankResponse>, AppError> {
    let tier_band = band(&body.tier)
        .ok_or_else(|| AppError::BadRequest("tier must be loved, fine or disliked".into()))?;

    let target: Option<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE id = $1 AND parsed_type = 'album' AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let target = target.ok_or(AppError::NotFound)?;

    let candidates = tier_albums(&state, &body.tier, id).await?;

    let mut lo = 0usize;
    let mut hi = candidates.len();
    for c in &body.comparisons {
        if lo >= hi {
            return Err(AppError::BadRequest("more comparisons than needed".into()));
        }
        let mid = (lo + hi) / 2;
        if candidates[mid].id != c.opponent_id {
            return Err(AppError::BadRequest(
                "ranking state changed, restart the flow".into(),
            ));
        }
        match c.preferred.as_str() {
            "this" => hi = mid,
            "that" => lo = mid + 1,
            _ => return Err(AppError::BadRequest("preferred must be this or that".into())),
        }
    }

    if lo < hi {
        let opp = &candidates[(lo + hi) / 2];
        return Ok(Json(RankResponse::NeedMore {
            done: false,
            next_opponent: Opponent {
                id: opp.id,
                artist: album_field(opp, "artist"),
                title: album_field(opp, "title"),
            },
        }));
    }

    let old_tier = target
        .data
        .get("rating_tier")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut ordered: Vec<Uuid> = candidates.iter().map(|c| c.id).collect();
    ordered.insert(lo, id);

    let mut tx = state.pool.begin().await?;
    apply_tier(&mut tx, &ordered, &body.tier, tier_band).await?;

    if let Some(old) = old_tier.filter(|t| *t != body.tier) {
        if let Some(old_band) = band(&old) {
            let remaining: Vec<Uuid> =
                tier_albums(&state, &old, id).await?.iter().map(|l| l.id).collect();
            apply_tier(&mut tx, &remaining, &old, old_band).await?;
        }
    }
    tx.commit().await?;

    Ok(Json(RankResponse::Done {
        done: true,
        rating: score(lo, ordered.len(), tier_band),
        rank_position: lo as i64,
    }))
}

#[derive(Debug, Serialize)]
pub struct AlbumGroups {
    pub loved: Vec<Log>,
    pub fine: Vec<Log>,
    pub disliked: Vec<Log>,
    pub unrated: Vec<Log>,
}

pub async fn list_albums(State(state): State<AppState>) -> Result<Json<AlbumGroups>, AppError> {
    let albums: Vec<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = 'album' AND deleted_at IS NULL \
         ORDER BY COALESCE((data->>'rank_position')::int, 0), created_at DESC",
    )
    .fetch_all(&state.pool)
    .await?;

    let mut groups = AlbumGroups {
        loved: vec![],
        fine: vec![],
        disliked: vec![],
        unrated: vec![],
    };
    for album in albums {
        match album.data.get("rating_tier").and_then(|v| v.as_str()) {
            Some("loved") => groups.loved.push(album),
            Some("fine") => groups.fine.push(album),
            Some("disliked") => groups.disliked.push(album),
            _ => groups.unrated.push(album),
        }
    }
    Ok(Json(groups))
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
