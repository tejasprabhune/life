use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::models::{
    Action, AlbumData, ItineraryEntry, LearningRequest, NutritionData, Parsed, PersonData,
    PlaceData, SongData, TripData,
};
use crate::usda;

const CHAT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
const MODELS: &[&str] = &["openai/gpt-oss-120b", "llama-3.3-70b-versatile"];

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize)]
struct Message {
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Deserialize)]
struct ToolCall {
    function: FunctionCall,
}

#[derive(Debug, Deserialize)]
struct FunctionCall {
    name: String,
    arguments: String,
}

fn tools() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "log_nutrition",
                "description": "Log a food or drink the user ate. Estimate portion weight and macros from the description, including Indian dishes (roti, dal, biryani, paneer, dosa, idli, samosa).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "food_name": {
                            "type": "string",
                            "description": "Short display name of the food or dish, e.g. 'Butter chicken with rice'"
                        },
                        "quantity": {
                            "type": "string",
                            "description": "Amount eaten as stated or inferred, e.g. '1 cup', '150g', '2 rotis'"
                        },
                        "quantity_g": {
                            "type": "number",
                            "description": "Best estimate of the total weight eaten in grams"
                        },
                        "usda_query": {
                            "type": "string",
                            "description": "Short generic search term for the USDA FoodData Central database, e.g. 'banana raw', 'chicken curry', 'roti'"
                        },
                        "calories": {
                            "type": "integer",
                            "description": "Estimated total kcal for the stated quantity"
                        },
                        "protein_g": { "type": "number", "description": "Estimated total protein in grams" },
                        "carbs_g": { "type": "number", "description": "Estimated total carbohydrates in grams" },
                        "fat_g": { "type": "number", "description": "Estimated total fat in grams" }
                    },
                    "required": ["food_name", "quantity", "quantity_g", "usda_query", "calories", "protein_g", "carbs_g", "fat_g"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_person",
                "description": "Log one or more people the user met or talked to, one array element per person.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "people": {
                            "type": "array",
                            "description": "Every person mentioned, as separate elements. Never combine two people into one element.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "The person's name" },
                                    "email": { "type": "string", "description": "This person's email address if mentioned" },
                                    "phone": { "type": "string", "description": "This person's phone number if mentioned" },
                                    "context": {
                                        "type": "string",
                                        "description": "Where or how they met plus any notes worth remembering"
                                    }
                                },
                                "required": ["name", "context"]
                            }
                        }
                    },
                    "required": ["people"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_album",
                "description": "Log an album the user listened to in full. Never assign a rating here; ratings come from a separate ranking flow.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "artist": { "type": "string", "description": "Album artist" },
                        "title": { "type": "string", "description": "Album title" },
                        "thoughts": { "type": "string", "description": "The user's thoughts on the album, if any" }
                    },
                    "required": ["artist", "title"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_song",
                "description": "Log a single song: one the user loved, or one heard somewhere they want to catch and revisit later.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Song title, omit when unknown" },
                        "artist": { "type": "string", "description": "Artist, omit when unknown" },
                        "status": {
                            "type": "string",
                            "enum": ["loved", "to_revisit", "revisited"],
                            "description": "loved when they express loving it; to_revisit when they want to catch, remember or find a song later; revisited when they came back to one"
                        },
                        "thoughts": { "type": "string", "description": "The user's thoughts, if any" },
                        "context": {
                            "type": "string",
                            "description": "Where or how it was heard, plus the song description when the title is unknown, e.g. 'dreamy synth at Blue Bottle'"
                        },
                        "source": { "type": "string", "description": "Short source label: radio, cafe, friend, tv, etc." }
                    },
                    "required": ["status"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_place",
                "description": "Log a venue the user visited: a cafe, restaurant, bar or dessert spot. Separate from food macros; this is about the place itself.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Venue name" },
                        "category": {
                            "type": "string",
                            "enum": ["coffee", "restaurant", "bar", "dessert", "other"],
                            "description": "Venue kind, inferred when unstated"
                        },
                        "order_text": { "type": "string", "description": "What the user ordered" },
                        "thoughts": { "type": "string", "description": "The user's impressions" },
                        "city": { "type": "string", "description": "City if mentioned" }
                    },
                    "required": ["name", "category"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_trip",
                "description": "Log a trip or destination the user visited.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "destination": { "type": "string", "description": "City, region or country visited" },
                        "itinerary_items": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Things done or seen on the trip"
                        },
                        "thoughts": { "type": "string", "description": "The user's impressions" },
                        "start_date": { "type": "string", "description": "YYYY-MM-DD if mentioned" },
                        "end_date": { "type": "string", "description": "YYYY-MM-DD if mentioned" }
                    },
                    "required": ["destination"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_itinerary_item",
                "description": "Add one item to an existing trip's itinerary, e.g. 'in Lisbon, add LX Factory'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "destination": { "type": "string", "description": "Trip destination; omit to use the most recent open trip" },
                        "name": { "type": "string", "description": "The itinerary item" },
                        "note": { "type": "string", "description": "Optional note about the item" }
                    },
                    "required": ["name"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_sleep",
                "description": "Open or close a sleep session. 'sleeping now' starts one; 'just woke up' ends it.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["start", "end", "both"],
                            "description": "start when going to sleep, end when waking up, both when one phrase states a bedtime and that they are now awake"
                        },
                        "at": {
                            "type": "string",
                            "description": "ISO timestamp override when the user states a time, e.g. 'went to bed at 11'; compute from the current local time given in the system prompt"
                        }
                    },
                    "required": ["action"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_learning",
                "description": "Log study progress on a field the user is learning: lectures or chapters finished, problems done, or a note. Match field, resource and topic to the user's configured names listed in the system prompt.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "field": { "type": "string", "description": "The learning field this belongs to" },
                        "resource": { "type": "string", "description": "Resource name when mentioned, e.g. a lecture series or book" },
                        "topic": { "type": "string", "description": "Topic name when mentioned" },
                        "kind": {
                            "type": "string",
                            "enum": ["study", "problems", "note"],
                            "description": "study for watching or reading, problems for exercises, note otherwise"
                        },
                        "resource_progress": { "type": "integer", "description": "New unit reached, e.g. 7 for 'lecture 7'" },
                        "problems_count": { "type": "integer", "description": "How many problems were done" },
                        "problems_type": { "type": "string", "enum": ["theory", "implementation"] },
                        "confidence_signal": {
                            "type": "string",
                            "description": "up when the user felt good, down when shaky, or set:N for an explicit 1-5 rating"
                        },
                        "note": { "type": "string", "description": "Any remaining detail" }
                    },
                    "required": ["kind"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "log_workout",
                "description": "Log a gym workout. The session itself lives in wger; this pulls the latest one. Use when the user says they worked out, lifted, hit the gym, or similar.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "note": { "type": "string", "description": "Any free-text remark the user added, e.g. 'felt strong on squats'" },
                        "allow_not_today": {
                            "type": "boolean",
                            "description": "True only when the user explicitly asks to log a past session, e.g. 'log my last workout even though it was yesterday'"
                        }
                    }
                }
            }
        }
    ])
}

const SYSTEM_PROMPT: &str = "You parse one short personal log entry into tool calls. \
Entries are either food eaten (log_nutrition) or people met (log_person). \
For food, estimate realistic portion weights: a roti is about 40g, a naan about 90g, \
a dosa about 120g, an idli about 40g, a samosa about 100g, a typical restaurant curry \
serving about 250g, a cup of cooked rice about 160g. Scale macros to the full stated \
quantity. If the entry names multiple foods, combine them into one dish entry with \
summed macros and pick the dominant component for usda_query. \
For people, extract each name and keep all remaining detail in context. \
If the entry mentions meeting more than one person, put each person in their own \
people element: match emails and phone numbers to the right person and repeat the \
shared context for each. Never combine two people into one element. \
For music, log_album is for full album listens and log_song for single songs. \
Song status: phrases like loved, obsessed with, or this song is great mean loved; \
catch, remember, come back to, find this later, heard at, or playing at mean \
to_revisit; came back to or finally listened again means revisited. \
When the user gives only a vague description and no song title, leave title out \
and put the description with the location in context. \
For gym sessions, phrases like worked out, lifted, or hit the gym mean log_workout; \
any extra remark goes in note. Set allow_not_today only when the user explicitly \
asks to log a session that was not today. \
Visiting a cafe, restaurant, bar or dessert spot is log_place; what they ate there \
can additionally be log_nutrition only when they describe the food as eaten. \
Trips to cities or countries are log_trip with itinerary_items for things done. \
Adding one thing to an existing trip is log_itinerary_item. \
Sleeping now or going to bed is log_sleep action start; waking up is action end; \
when a clock time is stated, compute the ISO timestamp from the current local time. \
A phrase stating both a bedtime and waking up means one log_sleep call with action \
both and at set to the stated bedtime. \
Study progress, watched lectures, finished chapters, or practice problems are \
log_learning; match names against the configured fields listed below when present. \
Always call at least one tool.";

async fn chat(
    http: &reqwest::Client,
    api_key: &str,
    raw_text: &str,
    context: &str,
) -> Result<Vec<(String, Value)>> {
    let mut last_err = anyhow!("no groq models attempted");
    let system = format!("{SYSTEM_PROMPT}\n\n{context}");

    for model in MODELS {
        let body = json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": raw_text }
            ],
            "tools": tools(),
            "tool_choice": "required",
            "temperature": 0.2
        });

        let resp = match http
            .post(CHAT_URL)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = e.into();
                continue;
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            tracing::warn!(%status, model, "groq request failed: {text}");
            last_err = anyhow!("groq {model} returned {status}");
            continue;
        }

        let parsed: ChatResponse = resp.json().await?;
        let calls = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.tool_calls)
            .unwrap_or_default();

        if calls.is_empty() {
            last_err = anyhow!("groq {model} returned no tool call");
            continue;
        }

        return calls
            .into_iter()
            .take(8)
            .map(|call| {
                let args: Value = serde_json::from_str(&call.function.arguments)
                    .context("tool call arguments were not valid JSON")?;
                Ok((call.function.name, args))
            })
            .collect();
    }

    Err(last_err)
}

fn as_f64(args: &Value, key: &str) -> Result<f64> {
    args.get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| anyhow!("missing numeric field {key}"))
}

fn as_str(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("missing field {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Parse raw text into structured log entries. One entry usually yields one
/// tool call, but mentioning several people yields one log_person call each.
/// Nutrition entries are grounded against USDA FoodData Central; if no usable
/// match exists, the model's own estimates are kept and usda_fdc_id stays null.
pub async fn parse(
    http: &reqwest::Client,
    groq_key: &str,
    usda_key: &str,
    raw_text: &str,
    context: &str,
) -> Result<Vec<Action>> {
    let calls = chat(http, groq_key, raw_text, context).await?;
    let mut results = Vec::with_capacity(calls.len());
    for (name, args) in calls {
        match name.as_str() {
            "log_workout" => results.push(Action::Workout {
                note: opt_str(&args, "note"),
                allow_not_today: args
                    .get("allow_not_today")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            "log_itinerary_item" => results.push(Action::ItineraryItem {
                destination: opt_str(&args, "destination"),
                name: as_str(&args, "name")?,
                note: opt_str(&args, "note"),
            }),
            "log_sleep" => {
                let action = as_str(&args, "action")?;
                if !matches!(action.as_str(), "start" | "end" | "both") {
                    bail!("sleep action must be start, end or both");
                }
                results.push(Action::Sleep { action, at: opt_str(&args, "at") });
            }
            "log_learning" => {
                let kind = as_str(&args, "kind")?;
                if !matches!(kind.as_str(), "study" | "problems" | "note") {
                    bail!("learning kind must be study, problems or note");
                }
                results.push(Action::Learning(LearningRequest {
                    field: opt_str(&args, "field"),
                    resource: opt_str(&args, "resource"),
                    topic: opt_str(&args, "topic"),
                    kind,
                    resource_progress: args.get("resource_progress").and_then(Value::as_i64),
                    problems_count: args.get("problems_count").and_then(Value::as_i64),
                    problems_type: opt_str(&args, "problems_type")
                        .filter(|t| matches!(t.as_str(), "theory" | "implementation")),
                    confidence_signal: opt_str(&args, "confidence_signal"),
                    note: opt_str(&args, "note"),
                }));
            }
            _ => results.extend(
                parse_call(http, usda_key, &name, args)
                    .await?
                    .into_iter()
                    .map(Action::Entry),
            ),
        }
    }
    if results.is_empty() {
        bail!("no entries parsed");
    }
    Ok(results)
}

async fn parse_call(
    http: &reqwest::Client,
    usda_key: &str,
    name: &str,
    args: Value,
) -> Result<Vec<Parsed>> {
    match name {
        "log_nutrition" => {
            let food_name = as_str(&args, "food_name")?;
            let quantity = as_str(&args, "quantity")?;
            let quantity_g = as_f64(&args, "quantity_g")?;
            let usda_query = as_str(&args, "usda_query")?;
            let est_calories = as_f64(&args, "calories")?;
            let est_protein = as_f64(&args, "protein_g")?;
            let est_carbs = as_f64(&args, "carbs_g")?;
            let est_fat = as_f64(&args, "fat_g")?;

            let grounded = if quantity_g > 0.0 {
                usda::ground(http, usda_key, &usda_query, quantity_g)
                    .await
                    .unwrap_or_else(|e| {
                        tracing::warn!("usda lookup error: {e}");
                        None
                    })
            } else {
                None
            };

            // A grounded result far from the model's estimate usually means
            // the search matched the wrong food (e.g. plain butter for butter
            // chicken). Keep the estimate in that case.
            let usable = grounded.filter(|g| {
                g.calories > 0.0
                    && (est_calories <= 30.0
                        || (g.calories >= est_calories / 4.0 && g.calories <= est_calories * 4.0))
            });

            let data = match usable {
                Some(g) => {
                    tracing::info!(fdc_id = %g.fdc_id, desc = %g.description, "grounded in usda");
                    NutritionData {
                        food_name,
                        quantity,
                        calories: g.calories.round() as i64,
                        protein_g: (g.protein_g * 10.0).round() / 10.0,
                        carbs_g: (g.carbs_g * 10.0).round() / 10.0,
                        fat_g: (g.fat_g * 10.0).round() / 10.0,
                        usda_fdc_id: Some(g.fdc_id),
                    }
                }
                None => NutritionData {
                    food_name,
                    quantity,
                    calories: est_calories.round() as i64,
                    protein_g: est_protein,
                    carbs_g: est_carbs,
                    fat_g: est_fat,
                    usda_fdc_id: None,
                },
            };
            Ok(vec![Parsed::Nutrition(data)])
        }
        "log_person" => {
            let people = args
                .get("people")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("missing people array"))?;
            people
                .iter()
                .take(8)
                .map(|p| {
                    Ok(Parsed::Person(PersonData {
                        name: as_str(p, "name")?,
                        email: opt_str(p, "email"),
                        phone: opt_str(p, "phone"),
                        context: as_str(p, "context")?,
                        last_contacted: None,
                    }))
                })
                .collect()
        }
        "log_place" => {
            let category = as_str(&args, "category")?;
            if !crate::rank::PLACE_CATEGORIES.contains(&category.as_str()) {
                bail!("unexpected place category {category}");
            }
            Ok(vec![Parsed::Place(PlaceData {
                name: as_str(&args, "name")?,
                category,
                order_text: opt_str(&args, "order_text"),
                thoughts: opt_str(&args, "thoughts"),
                city: opt_str(&args, "city"),
                address: None,
                rating: None,
                rating_tier: None,
                rank_position: None,
            })])
        }
        "log_trip" => {
            let itinerary = args
                .get("itinerary_items")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|name| ItineraryEntry { name: name.to_string(), note: None })
                        .collect()
                })
                .unwrap_or_default();
            Ok(vec![Parsed::Trip(TripData {
                destination: as_str(&args, "destination")?,
                start_date: opt_str(&args, "start_date"),
                end_date: opt_str(&args, "end_date"),
                itinerary,
                thoughts: opt_str(&args, "thoughts"),
                rating: None,
                rating_tier: None,
                rank_position: None,
            })])
        }
        "log_album" => Ok(vec![Parsed::Album(AlbumData {
            artist: as_str(&args, "artist")?,
            title: as_str(&args, "title")?,
            thoughts: opt_str(&args, "thoughts"),
            rating: None,
            rating_tier: None,
            rank_position: None,
        })]),
        "log_song" => {
            let status = as_str(&args, "status")?;
            if !matches!(status.as_str(), "loved" | "to_revisit" | "revisited") {
                bail!("unexpected song status {status}");
            }
            Ok(vec![Parsed::Song(SongData {
                title: opt_str(&args, "title"),
                artist: opt_str(&args, "artist"),
                status,
                thoughts: opt_str(&args, "thoughts"),
                context: opt_str(&args, "context"),
                source: opt_str(&args, "source"),
            })])
        }
        other => bail!("unexpected tool call {other}"),
    }
}
