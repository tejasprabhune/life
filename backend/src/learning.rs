use anyhow::{anyhow, Result};
use axum::extract::{Multipart, Path, Query, State};
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::FromRow;
use uuid::Uuid;

use crate::models::{LearningData, LearningRequest, Log};
use crate::routes::AppError;
use crate::AppState;

#[derive(Debug, Serialize, FromRow)]
pub struct Field {
    pub id: Uuid,
    pub name: String,
    pub goal_text: Option<String>,
    pub timeline_text: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Resource {
    pub id: Uuid,
    pub field_id: Uuid,
    pub kind: String,
    pub title: String,
    pub uri: Option<String>,
    pub total_units: Option<i32>,
    pub unit_label: Option<String>,
    pub current_unit: i32,
    pub structure: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct Topic {
    pub id: Uuid,
    pub field_id: Uuid,
    pub name: String,
    pub ord: i32,
    pub status: String,
    pub confidence: Option<i32>,
    pub source_resource_id: Option<Uuid>,
}

async fn active_fields(state: &AppState) -> Result<Vec<Field>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, name, goal_text, timeline_text, created_at FROM fields \
         WHERE archived_at IS NULL ORDER BY created_at",
    )
    .fetch_all(&state.pool)
    .await?)
}

async fn field_resources(state: &AppState, field_id: Uuid) -> Result<Vec<Resource>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, field_id, kind, title, uri, total_units, unit_label, current_unit, structure \
         FROM resources WHERE field_id = $1 ORDER BY created_at",
    )
    .bind(field_id)
    .fetch_all(&state.pool)
    .await?)
}

async fn field_topics(state: &AppState, field_id: Uuid) -> Result<Vec<Topic>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, field_id, name, ord, status, confidence, source_resource_id \
         FROM topics WHERE field_id = $1 ORDER BY ord",
    )
    .bind(field_id)
    .fetch_all(&state.pool)
    .await?)
}

/// Lines injected into the parser's system prompt so names like
/// "CS 285 lecture 7" resolve to the right field and resource.
pub async fn context_block(state: &AppState) -> String {
    let Ok(fields) = active_fields(state).await else {
        return String::new();
    };
    if fields.is_empty() {
        return String::new();
    }
    let mut out = String::from("Configured learning fields:\n");
    for field in &fields {
        out.push_str(&format!("- field: {}\n", field.name));
        if let Ok(resources) = field_resources(state, field.id).await {
            for r in resources {
                out.push_str(&format!("  resource: {}\n", r.title));
            }
        }
        if let Ok(topics) = field_topics(state, field.id).await {
            for t in topics {
                out.push_str(&format!("  topic: {}\n", t.name));
            }
        }
    }
    out
}

fn best_match<'a, T>(items: &'a [T], query: &str, name: impl Fn(&T) -> &str) -> Option<&'a T> {
    let q = query.to_lowercase();
    let mut best: Option<(&T, i32)> = None;
    for item in items {
        let n = name(item).to_lowercase();
        let score = if n == q {
            3
        } else if n.contains(&q) || q.contains(&n) {
            2
        } else if n.split_whitespace().any(|w| q.contains(w) && w.len() > 2) {
            1
        } else {
            continue;
        };
        if best.map(|(_, s)| score > s).unwrap_or(true) {
            best = Some((item, score));
        }
    }
    best.map(|(item, _)| item)
}

async fn most_recent_field(state: &AppState, fields: &[Field]) -> Option<Uuid> {
    let last: Option<(String,)> = sqlx::query_as(
        "SELECT data->>'field_id' FROM logs \
         WHERE parsed_type = 'learning' AND deleted_at IS NULL \
         AND data->>'field_id' IS NOT NULL ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await
    .ok()
    .flatten();
    last.and_then(|(id,)| Uuid::parse_str(&id).ok())
        .filter(|id| fields.iter().any(|f| f.id == *id))
        .or_else(|| fields.last().map(|f| f.id))
}

/// Resolve names, apply side effects, store the learning event.
pub async fn apply(state: &AppState, raw: &str, req: LearningRequest) -> Result<Log, AppError> {
    let fields = active_fields(state).await?;

    let mut field = req
        .field
        .as_deref()
        .and_then(|q| best_match(&fields, q, |f: &Field| &f.name));

    let mut resource: Option<Resource> = None;
    if let Some(rq) = req.resource.as_deref() {
        if let Some(f) = field {
            let rs = field_resources(state, f.id).await?;
            resource = best_match(&rs, rq, |r: &Resource| &r.title).map(|r| Resource { ..clone_resource(r) });
        }
        if resource.is_none() {
            for f in &fields {
                let rs = field_resources(state, f.id).await?;
                if let Some(r) = best_match(&rs, rq, |r: &Resource| &r.title) {
                    resource = Some(clone_resource(r));
                    field = Some(f);
                    break;
                }
            }
        }
    }

    if field.is_none() {
        let recent = most_recent_field(state, &fields).await;
        field = recent.and_then(|id| fields.iter().find(|f| f.id == id));
    }

    let mut topic: Option<Topic> = None;
    if let (Some(tq), Some(f)) = (req.topic.as_deref(), field) {
        let ts = field_topics(state, f.id).await?;
        topic = best_match(&ts, tq, |t: &Topic| &t.name).map(clone_topic);
    }

    if let (Some(progress), Some(r)) = (req.resource_progress, &resource) {
        sqlx::query("UPDATE resources SET current_unit = $2 WHERE id = $1")
            .bind(r.id)
            .bind(progress as i32)
            .execute(&state.pool)
            .await?;
    }

    if let Some(t) = &topic {
        let new_confidence = match req.confidence_signal.as_deref() {
            Some("up") => Some((t.confidence.unwrap_or(3) + 1).min(5)),
            Some("down") => Some((t.confidence.unwrap_or(3) - 1).max(1)),
            Some(s) if s.starts_with("set:") => s[4..].parse::<i32>().ok().map(|n| n.clamp(1, 5)),
            _ => t.confidence,
        };
        let new_status = if t.status == "todo" { "in_progress" } else { t.status.as_str() };
        sqlx::query("UPDATE topics SET status = $2, confidence = $3 WHERE id = $1")
            .bind(t.id)
            .bind(new_status)
            .bind(new_confidence)
            .execute(&state.pool)
            .await?;
    }

    let data = LearningData {
        field_id: field.map(|f| f.id),
        field_name: field.map(|f| f.name.clone()),
        resource_id: resource.as_ref().map(|r| r.id),
        resource_title: resource.as_ref().map(|r| r.title.clone()),
        topic_id: topic.as_ref().map(|t| t.id),
        topic_name: topic.as_ref().map(|t| t.name.clone()),
        kind: req.kind,
        resource_progress: req.resource_progress,
        problems_count: req.problems_count,
        problems_type: req.problems_type,
        note: req.note,
    };

    let log: Log = sqlx::query_as(
        "INSERT INTO logs (raw_input, parsed_type, data) VALUES ($1, 'learning', $2) \
         RETURNING id, created_at, raw_input, parsed_type, data",
    )
    .bind(raw)
    .bind(serde_json::to_value(&data).unwrap())
    .fetch_one(&state.pool)
    .await?;
    Ok(log)
}

fn clone_resource(r: &Resource) -> Resource {
    Resource {
        id: r.id,
        field_id: r.field_id,
        kind: r.kind.clone(),
        title: r.title.clone(),
        uri: r.uri.clone(),
        total_units: r.total_units,
        unit_label: r.unit_label.clone(),
        current_unit: r.current_unit,
        structure: r.structure.clone(),
    }
}

fn clone_topic(t: &Topic) -> Topic {
    Topic {
        id: t.id,
        field_id: t.field_id,
        name: t.name.clone(),
        ord: t.ord,
        status: t.status.clone(),
        confidence: t.confidence,
        source_resource_id: t.source_resource_id,
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateField {
    pub name: String,
    pub goal_text: Option<String>,
    pub timeline_text: Option<String>,
}

pub async fn create_field(
    State(state): State<AppState>,
    Json(body): Json<CreateField>,
) -> Result<Json<Field>, AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is empty".into()));
    }
    let field: Field = sqlx::query_as(
        "INSERT INTO fields (name, goal_text, timeline_text) VALUES ($1, $2, $3) \
         RETURNING id, name, goal_text, timeline_text, created_at",
    )
    .bind(body.name.trim())
    .bind(body.goal_text)
    .bind(body.timeline_text)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(field))
}

#[derive(Debug, Serialize)]
pub struct FieldSummary {
    #[serde(flatten)]
    pub field: Field,
    pub units_done: i64,
    pub units_total: i64,
    pub topics_done: i64,
    pub topics_total: i64,
    pub problems_theory: i64,
    pub problems_implementation: i64,
    pub streak: i64,
}

#[derive(Debug, Deserialize)]
pub struct TzQuery {
    pub tz_offset_min: Option<i32>,
}

async fn field_stats(
    state: &AppState,
    field_id: Uuid,
    tz_offset_min: i32,
) -> Result<(i64, i64, i64, i64, i64), AppError> {
    let problems: Vec<(Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT data->>'problems_type', SUM((data->>'problems_count')::bigint)::bigint FROM logs \
         WHERE parsed_type = 'learning' AND deleted_at IS NULL \
         AND data->>'field_id' = $1 AND data->>'problems_count' IS NOT NULL \
         GROUP BY 1",
    )
    .bind(field_id.to_string())
    .fetch_all(&state.pool)
    .await?;
    let mut theory = 0;
    let mut implementation = 0;
    for (kind, count) in problems {
        match kind.as_deref() {
            Some("theory") => theory += count.unwrap_or(0),
            Some("implementation") => implementation += count.unwrap_or(0),
            _ => theory += count.unwrap_or(0),
        }
    }

    let topics: Option<(i64, i64)> = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'done') FROM topics WHERE field_id = $1",
    )
    .bind(field_id)
    .fetch_optional(&state.pool)
    .await?;
    let (topics_total, topics_done) = topics.unwrap_or((0, 0));

    let days: Vec<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT created_at FROM logs WHERE parsed_type = 'learning' AND deleted_at IS NULL \
         AND data->>'field_id' = $1 ORDER BY created_at DESC LIMIT 200",
    )
    .bind(field_id.to_string())
    .fetch_all(&state.pool)
    .await?;
    let offset = Duration::minutes(tz_offset_min as i64);
    let mut dates: Vec<NaiveDate> = days.iter().map(|(d,)| (*d - offset).date_naive()).collect();
    dates.dedup();
    let today = (Utc::now() - offset).date_naive();
    let mut streak = 0i64;
    let mut cursor = today;
    for date in &dates {
        if *date == cursor {
            streak += 1;
            cursor = cursor.pred_opt().unwrap();
        } else if *date == cursor.pred_opt().unwrap() && streak == 0 {
            // a streak that ended yesterday is still alive today
            streak += 1;
            cursor = date.pred_opt().unwrap();
        } else {
            break;
        }
    }

    Ok((topics_done, topics_total, theory, implementation, streak))
}

pub async fn list_fields(
    State(state): State<AppState>,
    Query(query): Query<TzQuery>,
) -> Result<Json<Vec<FieldSummary>>, AppError> {
    let tz = query.tz_offset_min.unwrap_or(0);
    let fields = active_fields(&state).await?;
    let mut out = Vec::with_capacity(fields.len());
    for field in fields {
        let resources = field_resources(&state, field.id).await?;
        let units_done = resources.iter().map(|r| r.current_unit as i64).sum();
        let units_total = resources.iter().filter_map(|r| r.total_units.map(|t| t as i64)).sum();
        let (topics_done, topics_total, theory, implementation, streak) =
            field_stats(&state, field.id, tz).await?;
        out.push(FieldSummary {
            field,
            units_done,
            units_total,
            topics_done,
            topics_total,
            problems_theory: theory,
            problems_implementation: implementation,
            streak,
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct FieldDetail {
    #[serde(flatten)]
    pub field: Field,
    pub resources: Vec<Resource>,
    pub topics: Vec<Topic>,
    pub problems_theory: i64,
    pub problems_implementation: i64,
    pub streak: i64,
}

pub async fn get_field(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<TzQuery>,
) -> Result<Json<FieldDetail>, AppError> {
    let field: Option<Field> = sqlx::query_as(
        "SELECT id, name, goal_text, timeline_text, created_at FROM fields \
         WHERE id = $1 AND archived_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let field = field.ok_or(AppError::NotFound)?;
    let resources = field_resources(&state, id).await?;
    let topics = field_topics(&state, id).await?;
    let (_, _, theory, implementation, streak) =
        field_stats(&state, id, query.tz_offset_min.unwrap_or(0)).await?;
    Ok(Json(FieldDetail {
        field,
        resources,
        topics,
        problems_theory: theory,
        problems_implementation: implementation,
        streak,
    }))
}

async fn llm_json(state: &AppState, system: &str, user: &str) -> Result<Value> {
    let body = json!({
        "model": "openai/gpt-oss-120b",
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ],
        "response_format": { "type": "json_object" },
        "temperature": 0.2
    });
    let resp = state
        .http
        .post("https://api.groq.com/openai/v1/chat/completions")
        .bearer_auth(&state.groq_key)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let parsed: Value = resp.json().await?;
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow!("no content in llm response"))?;
    Ok(serde_json::from_str(content)?)
}

fn pdf_outline(bytes: &[u8]) -> Option<Vec<String>> {
    let doc = lopdf::Document::load_mem(bytes).ok()?;
    let catalog = doc.catalog().ok()?;
    let outlines_ref = catalog.get(b"Outlines").ok()?;
    let outlines = outlines_ref
        .as_reference()
        .ok()
        .and_then(|r| doc.get_dictionary(r).ok())?;
    let mut titles = Vec::new();
    let mut next = outlines.get(b"First").ok().and_then(|o| o.as_reference().ok());
    while let Some(item_ref) = next {
        let Ok(item) = doc.get_dictionary(item_ref) else { break };
        if let Ok(title) = item.get(b"Title") {
            if let Ok(s) = title.as_str() {
                titles.push(String::from_utf8_lossy(s).to_string());
            }
        }
        next = item.get(b"Next").ok().and_then(|o| o.as_reference().ok());
        if titles.len() > 200 {
            break;
        }
    }
    (!titles.is_empty()).then_some(titles)
}

struct Ingested {
    structure: Option<String>,
    total_units: Option<i32>,
    unit_label: Option<String>,
    notice: Option<String>,
}

async fn ingest_pdf(state: &AppState, bytes: &[u8]) -> Ingested {
    if let Some(titles) = pdf_outline(bytes) {
        let chapters = titles
            .iter()
            .filter(|t| t.to_lowercase().contains("chapter"))
            .count() as i32;
        let total = if chapters > 0 { chapters } else { titles.len() as i32 };
        return Ingested {
            structure: Some(titles.join("\n")),
            total_units: Some(total),
            unit_label: Some("chapter".into()),
            notice: None,
        };
    }

    let text = match pdf_extract::extract_text_from_mem(bytes) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("pdf text extraction failed: {e}");
            return Ingested {
                structure: None,
                total_units: None,
                unit_label: None,
                notice: Some("Could not read the PDF structure.".into()),
            };
        }
    };
    let head: String = text.chars().take(30_000).collect();
    match llm_json(
        state,
        "You are given the opening pages of a textbook or course PDF. Locate the table of \
         contents and answer as JSON: {\"unit_label\": \"chapter\"|\"lecture\"|\"page\", \
         \"total_units\": int or null, \"toc\": [list of section title strings]}.",
        &head,
    )
    .await
    {
        Ok(v) => Ingested {
            structure: v["toc"].as_array().map(|a| {
                a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")
            }),
            total_units: v["total_units"].as_i64().map(|n| n as i32),
            unit_label: v["unit_label"].as_str().map(str::to_string),
            notice: None,
        },
        Err(e) => {
            tracing::warn!("toc extraction failed: {e}");
            Ingested {
                structure: Some(head.chars().take(4000).collect()),
                total_units: None,
                unit_label: None,
                notice: Some("Could not find a table of contents in the PDF.".into()),
            }
        }
    }
}

async fn ingest_url(state: &AppState, url: &str) -> Ingested {
    let fetched = state
        .http
        .get(url)
        .header("User-Agent", "life-app/0.1")
        .send()
        .await
        .and_then(|r| r.error_for_status());
    let html = match fetched {
        Ok(resp) => resp.text().await.unwrap_or_default(),
        Err(e) => {
            tracing::warn!("url fetch failed: {e}");
            return Ingested {
                structure: None,
                total_units: None,
                unit_label: None,
                notice: Some("Could not fetch that page.".into()),
            };
        }
    };
    let text = html2text::from_read(html.as_bytes(), 100)
        .chars()
        .take(20_000)
        .collect::<String>();
    if text.trim().len() < 80 {
        return Ingested {
            structure: None,
            total_units: None,
            unit_label: None,
            notice: Some("Page content could not be extracted; it may need JavaScript.".into()),
        };
    }
    match llm_json(
        state,
        "You are given the readable text of a course or syllabus page. Answer as JSON: \
         {\"unit_label\": \"lecture\"|\"chapter\"|\"page\" or null, \"total_units\": int or null, \
         \"toc\": [list of lecture or section titles found]}.",
        &text,
    )
    .await
    {
        Ok(v) => Ingested {
            structure: v["toc"].as_array().map(|a| {
                a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")
            }),
            total_units: v["total_units"].as_i64().map(|n| n as i32),
            unit_label: v["unit_label"].as_str().map(str::to_string),
            notice: None,
        },
        Err(e) => {
            tracing::warn!("url structure extraction failed: {e}");
            Ingested {
                structure: Some(text.chars().take(4000).collect()),
                total_units: None,
                unit_label: None,
                notice: None,
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ResourceResponse {
    #[serde(flatten)]
    pub resource: Resource,
    pub notice: Option<String>,
}

pub async fn add_resource(
    State(state): State<AppState>,
    Path(field_id): Path<Uuid>,
    mut multipart: Multipart,
) -> Result<Json<ResourceResponse>, AppError> {
    let mut kind = String::new();
    let mut title = String::new();
    let mut url = String::new();
    let mut total_units: Option<i32> = None;
    let mut unit_label: Option<String> = None;
    let mut file: Option<(Vec<u8>, String)> = None;

    while let Some(part) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("bad multipart: {e}")))?
    {
        let name = part.name().unwrap_or("").to_string();
        match name.as_str() {
            "kind" => kind = part.text().await.unwrap_or_default(),
            "title" => title = part.text().await.unwrap_or_default(),
            "url" => url = part.text().await.unwrap_or_default(),
            "total_units" => {
                total_units = part.text().await.ok().and_then(|t| t.parse().ok());
            }
            "unit_label" => {
                unit_label = part.text().await.ok().filter(|t| !t.is_empty());
            }
            "file" => {
                let content_type = part
                    .content_type()
                    .unwrap_or("application/pdf")
                    .to_string();
                let filename = part.file_name().unwrap_or("").to_string();
                if title.is_empty() && !filename.is_empty() {
                    title = filename.trim_end_matches(".pdf").to_string();
                }
                let bytes = part
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("upload failed: {e}")))?;
                file = Some((bytes.to_vec(), content_type));
            }
            _ => {}
        }
    }

    if !matches!(kind.as_str(), "pdf" | "url" | "manual") {
        return Err(AppError::BadRequest("kind must be pdf, url or manual".into()));
    }

    let mut notice = None;
    let mut structure = None;
    let mut uri: Option<String> = None;

    match kind.as_str() {
        "pdf" => {
            let (bytes, _) = file
                .as_ref()
                .ok_or_else(|| AppError::BadRequest("pdf upload needs a file".into()))?;
            let ingested = ingest_pdf(&state, bytes).await;
            structure = ingested.structure;
            if total_units.is_none() {
                total_units = ingested.total_units;
            }
            if unit_label.is_none() {
                unit_label = ingested.unit_label;
            }
            notice = ingested.notice;
        }
        "url" => {
            if url.trim().is_empty() {
                return Err(AppError::BadRequest("url resource needs a url".into()));
            }
            uri = Some(url.trim().to_string());
            if title.is_empty() {
                title = url.trim().to_string();
            }
            let ingested = ingest_url(&state, url.trim()).await;
            structure = ingested.structure;
            if total_units.is_none() {
                total_units = ingested.total_units;
            }
            if unit_label.is_none() {
                unit_label = ingested.unit_label;
            }
            notice = ingested.notice;
        }
        _ => {}
    }

    if title.trim().is_empty() {
        return Err(AppError::BadRequest("resource needs a title".into()));
    }

    let resource: Resource = sqlx::query_as(
        "INSERT INTO resources (field_id, kind, title, uri, total_units, unit_label, structure) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         RETURNING id, field_id, kind, title, uri, total_units, unit_label, current_unit, structure",
    )
    .bind(field_id)
    .bind(&kind)
    .bind(title.trim())
    .bind(&uri)
    .bind(total_units)
    .bind(&unit_label)
    .bind(&structure)
    .fetch_one(&state.pool)
    .await?;

    let mut resource = resource;
    if let Some((bytes, content_type)) = file {
        if kind == "pdf" {
            sqlx::query("INSERT INTO resource_files (resource_id, content_type, bytes) VALUES ($1, $2, $3)")
                .bind(resource.id)
                .bind(content_type)
                .bind(bytes)
                .execute(&state.pool)
                .await?;
            let uri = format!("db://{}", resource.id);
            sqlx::query("UPDATE resources SET uri = $2 WHERE id = $1")
                .bind(resource.id)
                .bind(&uri)
                .execute(&state.pool)
                .await?;
            resource.uri = Some(uri);
        }
    }

    Ok(Json(ResourceResponse { resource, notice }))
}

#[derive(Debug, Serialize)]
pub struct ProposedTopic {
    pub name: String,
    pub source_resource_id: Option<Uuid>,
}

pub async fn generate_plan(
    State(state): State<AppState>,
    Path(field_id): Path<Uuid>,
) -> Result<Json<Vec<ProposedTopic>>, AppError> {
    let field: Option<Field> = sqlx::query_as(
        "SELECT id, name, goal_text, timeline_text, created_at FROM fields WHERE id = $1",
    )
    .bind(field_id)
    .fetch_optional(&state.pool)
    .await?;
    let field = field.ok_or(AppError::NotFound)?;
    let resources = field_resources(&state, field_id).await?;

    let mut prompt = format!(
        "Field: {}\nGoal: {}\nTimeline: {}\n\nResources:\n",
        field.name,
        field.goal_text.as_deref().unwrap_or("none stated"),
        field.timeline_text.as_deref().unwrap_or("none stated"),
    );
    for (i, r) in resources.iter().enumerate() {
        prompt.push_str(&format!(
            "[{i}] {} ({})\n{}\n\n",
            r.title,
            r.kind,
            r.structure.as_deref().unwrap_or("no structure extracted")
        ));
    }

    let v = llm_json(
        &state,
        "Propose an ordered topic plan for learning this field, grounded in the resources' \
         structure and the stated goal. 8 to 20 topics, short names, logical order. Answer as \
         JSON: {\"topics\": [{\"name\": string, \"resource_index\": int or null}]}.",
        &prompt,
    )
    .await
    .map_err(AppError::Internal)?;

    let topics = v["topics"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t["name"].as_str()?.to_string();
                    let source = t["resource_index"]
                        .as_u64()
                        .and_then(|i| resources.get(i as usize))
                        .map(|r| r.id);
                    Some(ProposedTopic { name, source_resource_id: source })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Json(topics))
}

#[derive(Debug, Deserialize)]
pub struct PlanTopic {
    pub name: String,
    pub source_resource_id: Option<Uuid>,
}

pub async fn save_plan(
    State(state): State<AppState>,
    Path(field_id): Path<Uuid>,
    Json(body): Json<Vec<PlanTopic>>,
) -> Result<Json<Vec<Topic>>, AppError> {
    let existing = field_topics(&state, field_id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM topics WHERE field_id = $1")
        .bind(field_id)
        .execute(&mut *tx)
        .await?;
    for (i, topic) in body.iter().enumerate() {
        let name = topic.name.trim();
        if name.is_empty() {
            continue;
        }
        // Carry over status and confidence for topics that keep their name.
        let prior = existing.iter().find(|t| t.name.eq_ignore_ascii_case(name));
        sqlx::query(
            "INSERT INTO topics (field_id, name, ord, status, confidence, source_resource_id) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(field_id)
        .bind(name)
        .bind(i as i32)
        .bind(prior.map(|t| t.status.clone()).unwrap_or_else(|| "todo".into()))
        .bind(prior.and_then(|t| t.confidence))
        .bind(topic.source_resource_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(field_topics(&state, field_id).await?))
}

#[derive(Debug, Deserialize)]
pub struct PatchResource {
    pub title: Option<String>,
    pub current_unit: Option<i32>,
    pub total_units: Option<i32>,
    pub unit_label: Option<String>,
}

pub async fn patch_resource(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchResource>,
) -> Result<Json<Resource>, AppError> {
    let resource: Option<Resource> = sqlx::query_as(
        "UPDATE resources SET \
            title = COALESCE($2, title), \
            current_unit = COALESCE($3, current_unit), \
            total_units = COALESCE($4, total_units), \
            unit_label = COALESCE($5, unit_label) \
         WHERE id = $1 \
         RETURNING id, field_id, kind, title, uri, total_units, unit_label, current_unit, structure",
    )
    .bind(id)
    .bind(body.title)
    .bind(body.current_unit)
    .bind(body.total_units)
    .bind(body.unit_label)
    .fetch_optional(&state.pool)
    .await?;
    resource.map(Json).ok_or(AppError::NotFound)
}

#[derive(Debug, Deserialize)]
pub struct PatchTopic {
    pub name: Option<String>,
    pub status: Option<String>,
    pub confidence: Option<i32>,
}

pub async fn patch_topic(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchTopic>,
) -> Result<Json<Topic>, AppError> {
    if let Some(status) = &body.status {
        if !matches!(status.as_str(), "todo" | "in_progress" | "done") {
            return Err(AppError::BadRequest("bad status".into()));
        }
    }
    if let Some(c) = body.confidence {
        if !(1..=5).contains(&c) {
            return Err(AppError::BadRequest("confidence must be 1 to 5".into()));
        }
    }
    let topic: Option<Topic> = sqlx::query_as(
        "UPDATE topics SET \
            name = COALESCE($2, name), \
            status = COALESCE($3, status), \
            confidence = COALESCE($4, confidence) \
         WHERE id = $1 \
         RETURNING id, field_id, name, ord, status, confidence, source_resource_id",
    )
    .bind(id)
    .bind(body.name)
    .bind(body.status)
    .bind(body.confidence)
    .fetch_optional(&state.pool)
    .await?;
    topic.map(Json).ok_or(AppError::NotFound)
}
