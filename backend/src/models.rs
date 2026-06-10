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

pub enum Parsed {
    Nutrition(NutritionData),
    Person(PersonData),
}

impl Parsed {
    pub fn type_name(&self) -> &'static str {
        match self {
            Parsed::Nutrition(_) => "nutrition",
            Parsed::Person(_) => "person",
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Parsed::Nutrition(n) => serde_json::to_value(n).unwrap(),
            Parsed::Person(p) => serde_json::to_value(p).unwrap(),
        }
    }
}
