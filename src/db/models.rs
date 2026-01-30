use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct ProcessedFile {
    pub id: i32,
    pub file_name: String,
    pub file_url: String,
    pub year: i32,
    pub state: String,
    pub station_name: String,
    pub last_modified: Option<DateTime<Utc>>,
    pub rows_processed: i32,
    pub processed_at: DateTime<Utc>,
    pub file_hash: Option<String>,
    pub observations_inserted: Option<i32>,
    pub observations_updated: Option<i32>,
    pub parse_failures: Option<i32>,
    pub processing_status: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewProcessedFile {
    pub file_name: String,
    pub file_url: String,
    pub year: i32,
    pub state: String,
    pub station_name: String,
    pub last_modified: Option<DateTime<Utc>>,
    pub rows_processed: i32,
    pub file_hash: Option<String>,
    pub observations_inserted: i32,
    pub observations_updated: i32,
    pub parse_failures: i32,
    pub processing_status: String,
}

#[derive(Debug, Clone)]
pub struct InsertResult {
    pub inserted: usize,
    pub updated: usize,
    pub total_rows_affected: usize,
}

#[derive(Debug, Clone, FromRow)]
pub struct Station {
    pub wbanno: i32,
    pub name: Option<String>,
    pub state: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub first_seen: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewStation {
    pub wbanno: i32,
    pub name: Option<String>,
    pub state: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Observation {
    pub id: i64,
    pub wbanno: i32,
    pub utc_datetime: DateTime<Utc>,
    pub lst_datetime: DateTime<Utc>,
    pub crx_version: Option<String>,

    pub t_calc: Option<f32>,
    pub t_hr_avg: Option<f32>,
    pub t_max: Option<f32>,
    pub t_min: Option<f32>,

    pub p_calc: Option<f32>,

    pub solarad: Option<f32>,
    pub solarad_flag: Option<i32>,
    pub solarad_max: Option<f32>,
    pub solarad_max_flag: Option<i32>,
    pub solarad_min: Option<f32>,
    pub solarad_min_flag: Option<i32>,

    pub sur_temp_type: Option<String>,
    pub sur_temp: Option<f32>,
    pub sur_temp_flag: Option<i32>,
    pub sur_temp_max: Option<f32>,
    pub sur_temp_max_flag: Option<i32>,
    pub sur_temp_min: Option<f32>,
    pub sur_temp_min_flag: Option<i32>,

    pub rh_hr_avg: Option<f32>,
    pub rh_hr_avg_flag: Option<i32>,

    pub soil_moisture_5: Option<f32>,
    pub soil_moisture_10: Option<f32>,
    pub soil_moisture_20: Option<f32>,
    pub soil_moisture_50: Option<f32>,
    pub soil_moisture_100: Option<f32>,

    pub soil_temp_5: Option<f32>,
    pub soil_temp_10: Option<f32>,
    pub soil_temp_20: Option<f32>,
    pub soil_temp_50: Option<f32>,
    pub soil_temp_100: Option<f32>,

    pub source_file_id: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewObservation {
    pub wbanno: i32,
    pub utc_datetime: DateTime<Utc>,
    pub lst_datetime: DateTime<Utc>,
    pub crx_version: Option<String>,

    pub t_calc: Option<f32>,
    pub t_hr_avg: Option<f32>,
    pub t_max: Option<f32>,
    pub t_min: Option<f32>,

    pub p_calc: Option<f32>,

    pub solarad: Option<f32>,
    pub solarad_flag: Option<i32>,
    pub solarad_max: Option<f32>,
    pub solarad_max_flag: Option<i32>,
    pub solarad_min: Option<f32>,
    pub solarad_min_flag: Option<i32>,

    pub sur_temp_type: Option<String>,
    pub sur_temp: Option<f32>,
    pub sur_temp_flag: Option<i32>,
    pub sur_temp_max: Option<f32>,
    pub sur_temp_max_flag: Option<i32>,
    pub sur_temp_min: Option<f32>,
    pub sur_temp_min_flag: Option<i32>,

    pub rh_hr_avg: Option<f32>,
    pub rh_hr_avg_flag: Option<i32>,

    pub soil_moisture_5: Option<f32>,
    pub soil_moisture_10: Option<f32>,
    pub soil_moisture_20: Option<f32>,
    pub soil_moisture_50: Option<f32>,
    pub soil_moisture_100: Option<f32>,

    pub soil_temp_5: Option<f32>,
    pub soil_temp_10: Option<f32>,
    pub soil_temp_20: Option<f32>,
    pub soil_temp_50: Option<f32>,
    pub soil_temp_100: Option<f32>,

    pub source_file_id: Option<i32>,
}
