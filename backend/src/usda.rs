use anyhow::Result;
use serde::Deserialize;

const SEARCH_URL: &str = "https://api.nal.usda.gov/fdc/v1/foods/search";

#[derive(Debug, Deserialize)]
struct SearchResponse {
    foods: Option<Vec<Food>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Food {
    fdc_id: i64,
    description: String,
    food_nutrients: Option<Vec<Nutrient>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Nutrient {
    nutrient_id: Option<i64>,
    nutrient_name: Option<String>,
    unit_name: Option<String>,
    value: Option<f64>,
}

#[derive(Debug)]
pub struct GroundedFood {
    pub fdc_id: String,
    pub description: String,
    pub calories: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
}

fn nutrient_value(nutrients: &[Nutrient], ids: &[i64]) -> Option<f64> {
    for id in ids {
        if let Some(n) = nutrients
            .iter()
            .find(|n| n.nutrient_id == Some(*id) && n.value.is_some())
        {
            return n.value;
        }
    }
    None
}

fn kcal(nutrients: &[Nutrient]) -> Option<f64> {
    // 1008 is Energy in kcal, 2047/2048 are Atwater energy values.
    if let Some(v) = nutrient_value(nutrients, &[1008, 2047, 2048]) {
        return Some(v);
    }
    nutrients
        .iter()
        .find(|n| {
            n.nutrient_name.as_deref() == Some("Energy")
                && n.unit_name.as_deref() == Some("KCAL")
                && n.value.is_some()
        })
        .and_then(|n| n.value)
}

/// Search FoodData Central and scale per-100g macros to the eaten grams.
/// Returns None when no usable match exists, so the caller can fall back
/// to model estimates.
pub async fn ground(
    http: &reqwest::Client,
    api_key: &str,
    query: &str,
    quantity_g: f64,
) -> Result<Option<GroundedFood>> {
    let resp = http
        .get(SEARCH_URL)
        .query(&[
            ("query", query),
            ("dataType", "Foundation,SR Legacy,Survey (FNDDS)"),
            ("pageSize", "5"),
            ("api_key", api_key),
        ])
        .send()
        .await?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), query, "usda search failed");
        return Ok(None);
    }

    let body: SearchResponse = resp.json().await?;
    let foods = body.foods.unwrap_or_default();
    let scale = quantity_g / 100.0;

    for food in foods {
        let Some(nutrients) = food.food_nutrients.as_deref() else {
            continue;
        };
        let Some(calories) = kcal(nutrients) else {
            continue;
        };
        let protein = nutrient_value(nutrients, &[1003]).unwrap_or(0.0);
        let carbs = nutrient_value(nutrients, &[1005]).unwrap_or(0.0);
        let fat = nutrient_value(nutrients, &[1004]).unwrap_or(0.0);

        return Ok(Some(GroundedFood {
            fdc_id: food.fdc_id.to_string(),
            description: food.description,
            calories: calories * scale,
            protein_g: protein * scale,
            carbs_g: carbs * scale,
            fat_g: fat * scale,
        }));
    }

    Ok(None)
}
