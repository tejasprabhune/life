use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Serialize, FromRow)]
pub struct Log {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub raw_input: String,
    pub parsed_type: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateLog {
    pub raw_text: String,
    pub tz_offset_min: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLog {
    pub raw_input: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub date: Option<String>,
    pub category: Option<String>,
    pub tz_offset_min: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NutritionData {
    pub food_name: String,
    pub quantity: String,
    pub calories: i64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub usda_fdc_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersonData {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub context: String,
    pub last_contacted: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AlbumData {
    pub artist: String,
    pub title: String,
    pub thoughts: Option<String>,
    pub rating: Option<f64>,
    pub rating_tier: Option<String>,
    pub rank_position: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SongData {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub status: String,
    pub thoughts: Option<String>,
    pub context: Option<String>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkoutSet {
    pub weight: Option<f64>,
    pub reps: Option<i64>,
    pub rir: Option<f64>,
    pub rest_s: Option<i64>,
    pub unit: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkoutExercise {
    pub exercise_id: i64,
    pub name: String,
    pub sets: Vec<WorkoutSet>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkoutData {
    pub wger_session_id: i64,
    pub date: String,
    pub notes: Option<String>,
    pub note: Option<String>,
    pub impression: Option<String>,
    pub duration_min: Option<i64>,
    pub exercises: Vec<WorkoutExercise>,
    pub total_sets: i64,
    pub total_volume: Option<f64>,
}

/// What one tool call asks the backend to do. Most calls carry a parsed
/// entry; a workout call asks for a wger sync instead.
pub enum Action {
    Entry(Parsed),
    Workout { note: Option<String>, allow_not_today: bool },
}

pub enum Parsed {
    Nutrition(NutritionData),
    Person(PersonData),
    Album(AlbumData),
    Song(SongData),
}

impl Parsed {
    pub fn type_name(&self) -> &'static str {
        match self {
            Parsed::Nutrition(_) => "nutrition",
            Parsed::Person(_) => "person",
            Parsed::Album(_) => "album",
            Parsed::Song(_) => "song",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Parsed::Nutrition(n) => serde_json::to_value(n).unwrap(),
            Parsed::Person(p) => serde_json::to_value(p).unwrap(),
            Parsed::Album(a) => serde_json::to_value(a).unwrap(),
            Parsed::Song(s) => serde_json::to_value(s).unwrap(),
        }
    }
}
