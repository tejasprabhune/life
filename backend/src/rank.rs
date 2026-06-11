use axum::extract::{Query, State};
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

pub const PLACE_CATEGORIES: &[&str] = &["coffee", "restaurant", "bar", "dessert", "other"];

fn band(tier: &str) -> Option<(f64, f64)> {
    TIERS.iter().find(|(t, _, _)| *t == tier).map(|(_, lo, hi)| (*lo, *hi))
}

fn score(position: usize, count: usize, band: (f64, f64)) -> f64 {
    let (lo, hi) = band;
    let raw = if count <= 1 {
        (lo + hi) / 2.0
    } else {
        hi - (hi - lo) * position as f64 / (count - 1) as f64
    };
    (raw * 10.0).round() / 10.0
}

/// A rank group is (domain, category). Albums and trips have one global
/// group; places rank within their category.
struct Group {
    parsed_type: &'static str,
    category: Option<String>,
}

fn resolve_group(domain: &str, category: Option<&str>) -> Result<Group, AppError> {
    match domain {
        "album" => Ok(Group { parsed_type: "album", category: None }),
        "trip" => Ok(Group { parsed_type: "trip", category: None }),
        "place" => {
            let cat = category
                .filter(|c| PLACE_CATEGORIES.contains(c))
                .ok_or_else(|| {
                    AppError::BadRequest("place ranking needs a valid category".into())
                })?;
            Ok(Group { parsed_type: "place", category: Some(cat.to_string()) })
        }
        _ => Err(AppError::BadRequest("domain must be album, place or trip".into())),
    }
}

pub fn label(log: &Log) -> String {
    let get = |k: &str| log.data.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match log.parsed_type.as_str() {
        "album" => format!("{}, {}", get("title"), get("artist")),
        "place" => get("name"),
        "trip" => get("destination"),
        _ => String::new(),
    }
}

async fn tier_members(
    state: &AppState,
    group: &Group,
    tier: &str,
    exclude: Uuid,
) -> Result<Vec<Log>, AppError> {
    let rows: Vec<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = $1 AND deleted_at IS NULL \
         AND data->>'rating_tier' = $2 AND id != $3 \
         AND ($4::text IS NULL OR data->>'category' = $4) \
         ORDER BY (data->>'rank_position')::int",
    )
    .bind(group.parsed_type)
    .bind(tier)
    .bind(exclude)
    .bind(group.category.as_deref())
    .fetch_all(&state.pool)
    .await?;
    Ok(rows)
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

#[derive(Debug, Deserialize)]
pub struct RankRequest {
    pub domain: String,
    pub category: Option<String>,
    pub item_id: Uuid,
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
    pub label: String,
}

pub async fn rank(
    State(state): State<AppState>,
    Json(body): Json<RankRequest>,
) -> Result<Json<RankResponse>, AppError> {
    let tier_band = band(&body.tier)
        .ok_or_else(|| AppError::BadRequest("tier must be loved, fine or disliked".into()))?;
    let group = resolve_group(&body.domain, body.category.as_deref())?;

    let target: Option<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE id = $1 AND parsed_type = $2 AND deleted_at IS NULL",
    )
    .bind(body.item_id)
    .bind(group.parsed_type)
    .fetch_optional(&state.pool)
    .await?;
    let target = target.ok_or(AppError::NotFound)?;

    if let Some(cat) = &group.category {
        if target.data.get("category").and_then(|v| v.as_str()) != Some(cat) {
            return Err(AppError::BadRequest("item is not in that category".into()));
        }
    }

    let candidates = tier_members(&state, &group, &body.tier, body.item_id).await?;

    let mut lo = 0usize;
    let mut hi = candidates.len();
    for c in &body.comparisons {
        if lo >= hi {
            return Err(AppError::BadRequest("more comparisons than needed".into()));
        }
        let mid = (lo + hi) / 2;
        if candidates[mid].id != c.opponent_id {
            return Err(AppError::BadRequest("ranking state changed, restart the flow".into()));
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
            next_opponent: Opponent { id: opp.id, label: label(opp) },
        }));
    }

    let old_tier = target
        .data
        .get("rating_tier")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let mut ordered: Vec<Uuid> = candidates.iter().map(|c| c.id).collect();
    ordered.insert(lo, body.item_id);

    let mut tx = state.pool.begin().await?;
    apply_tier(&mut tx, &ordered, &body.tier, tier_band).await?;

    if let Some(old) = old_tier.filter(|t| *t != body.tier) {
        if let Some(old_band) = band(&old) {
            let remaining: Vec<Uuid> = tier_members(&state, &group, &old, body.item_id)
                .await?
                .iter()
                .map(|l| l.id)
                .collect();
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
pub struct RankGroups {
    pub loved: Vec<Log>,
    pub fine: Vec<Log>,
    pub disliked: Vec<Log>,
    pub unrated: Vec<Log>,
}

pub async fn groups(state: &AppState, domain: &str, category: Option<&str>) -> Result<RankGroups, AppError> {
    let group = resolve_group(domain, category)?;
    let rows: Vec<Log> = sqlx::query_as(
        "SELECT id, created_at, raw_input, parsed_type, data FROM logs \
         WHERE parsed_type = $1 AND deleted_at IS NULL \
         AND ($2::text IS NULL OR data->>'category' = $2) \
         ORDER BY COALESCE((data->>'rank_position')::int, 0), created_at DESC",
    )
    .bind(group.parsed_type)
    .bind(group.category.as_deref())
    .fetch_all(&state.pool)
    .await?;

    let mut out = RankGroups { loved: vec![], fine: vec![], disliked: vec![], unrated: vec![] };
    for row in rows {
        match row.data.get("rating_tier").and_then(|v| v.as_str()) {
            Some("loved") => out.loved.push(row),
            Some("fine") => out.fine.push(row),
            Some("disliked") => out.disliked.push(row),
            _ => out.unrated.push(row),
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub domain: String,
    pub category: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<RankGroups>, AppError> {
    Ok(Json(groups(&state, &query.domain, query.category.as_deref()).await?))
}
