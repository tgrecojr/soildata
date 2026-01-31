use chrono::Utc;
use sqlx::PgPool;
use uscrn_ingest::db::models::{InsertResult, NewObservation, NewProcessedFile, NewStation};
use uscrn_ingest::db::Repository;

/// Test station upsert - insert new station
#[sqlx::test]
async fn test_upsert_new_station(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    let station = NewStation {
        wbanno: 12345,
        name: Some("Test Station".to_string()),
        state: "CA".to_string(),
        latitude: Some(37.7749),
        longitude: Some(-122.4194),
    };

    // Insert station
    repo.upsert_station(station.clone())
        .await
        .expect("Failed to insert station");

    // Verify insertion
    let result = sqlx::query_as::<_, (i32, Option<String>, String)>(
        "SELECT wbanno, name, state FROM stations WHERE wbanno = $1",
    )
    .bind(12345)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch station");

    assert_eq!(result.0, 12345);
    assert_eq!(result.1, Some("Test Station".to_string()));
    assert_eq!(result.2, "CA");
}

/// Test station upsert - update existing station
#[sqlx::test]
async fn test_upsert_updates_existing_station(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Insert initial station
    let station = NewStation {
        wbanno: 12345,
        name: Some("Original Name".to_string()),
        state: "CA".to_string(),
        latitude: Some(37.0),
        longitude: Some(-122.0),
    };
    repo.upsert_station(station).await.expect("Insert failed");

    // Update with new data
    let updated_station = NewStation {
        wbanno: 12345,
        name: Some("Updated Name".to_string()),
        state: "CA".to_string(),
        latitude: Some(38.0),
        longitude: Some(-123.0),
    };
    repo.upsert_station(updated_station)
        .await
        .expect("Update failed");

    // Verify update
    let result = sqlx::query_as::<_, (Option<String>, Option<f64>, Option<f64>)>(
        "SELECT name, latitude, longitude FROM stations WHERE wbanno = $1",
    )
    .bind(12345)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch");

    assert_eq!(result.0, Some("Updated Name".to_string()));
    assert_eq!(result.1, Some(38.0));
    assert_eq!(result.2, Some(-123.0));
}

/// Test batch station upsert
#[sqlx::test]
async fn test_batch_upsert_stations(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    let stations = vec![
        NewStation {
            wbanno: 1001,
            name: Some("Station A".to_string()),
            state: "CA".to_string(),
            latitude: Some(37.0),
            longitude: Some(-122.0),
        },
        NewStation {
            wbanno: 1002,
            name: Some("Station B".to_string()),
            state: "TX".to_string(),
            latitude: Some(30.0),
            longitude: Some(-97.0),
        },
        NewStation {
            wbanno: 1003,
            name: Some("Station C".to_string()),
            state: "NY".to_string(),
            latitude: Some(40.0),
            longitude: Some(-74.0),
        },
    ];

    repo.batch_upsert_stations(&stations)
        .await
        .expect("Batch upsert failed");

    // Verify all stations were inserted
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations")
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 3);
}

/// Test observation insertion
#[sqlx::test]
async fn test_insert_observations(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // First insert a station
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "CA".to_string(),
        latitude: None,
        longitude: None,
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // Create a processed file to get file_id
    let file = NewProcessedFile {
        file_name: "test_file.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "CA".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 2,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: 0,
        processing_status: "processing".to_string(),
    };
    let file_id = repo
        .mark_file_processed(file)
        .await
        .expect("File insert failed");

    // Create test observations
    let observations = vec![NewObservation {
        wbanno: 53104,
        utc_datetime: Utc::now(),
        lst_datetime: Utc::now(),
        crx_version: Some("3".to_string()),
        t_calc: Some(20.5),
        t_hr_avg: Some(21.0),
        t_max: Some(22.0),
        t_min: Some(19.0),
        p_calc: Some(0.0),
        solarad: Some(450.0),
        solarad_flag: Some(0),
        solarad_max: Some(500.0),
        solarad_max_flag: Some(0),
        solarad_min: Some(400.0),
        solarad_min_flag: Some(0),
        sur_temp_type: Some("C".to_string()),
        sur_temp: Some(18.5),
        sur_temp_flag: Some(0),
        sur_temp_max: Some(20.0),
        sur_temp_max_flag: Some(0),
        sur_temp_min: Some(17.0),
        sur_temp_min_flag: Some(0),
        rh_hr_avg: Some(65.0),
        rh_hr_avg_flag: Some(0),
        soil_moisture_5: Some(0.25),
        soil_moisture_10: Some(0.30),
        soil_moisture_20: Some(0.28),
        soil_moisture_50: Some(0.32),
        soil_moisture_100: Some(0.35),
        soil_temp_5: Some(15.0),
        soil_temp_10: Some(14.5),
        soil_temp_20: Some(14.0),
        soil_temp_50: Some(13.5),
        soil_temp_100: Some(13.0),
        source_file_id: None,
    }];

    let result = repo
        .insert_observations(&observations, file_id)
        .await
        .expect("Observation insert failed");

    assert_eq!(result.total_rows_affected, 1);

    // Verify observation was inserted
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations WHERE wbanno = $1")
        .bind(53104)
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 1);
}

/// Test observation upsert - should update existing observation
#[sqlx::test]
async fn test_upsert_observation_updates_existing(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Insert station
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "CA".to_string(),
        latitude: None,
        longitude: None,
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // Create processed file
    let file = NewProcessedFile {
        file_name: "test_file.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "CA".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 1,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: 0,
        processing_status: "processing".to_string(),
    };
    let file_id = repo
        .mark_file_processed(file)
        .await
        .expect("File insert failed");

    let timestamp = Utc::now();

    // Insert initial observation
    let observation = vec![NewObservation {
        wbanno: 53104,
        utc_datetime: timestamp,
        lst_datetime: timestamp,
        crx_version: Some("3".to_string()),
        t_hr_avg: Some(20.0),
        t_calc: None,
        t_max: None,
        t_min: None,
        p_calc: None,
        solarad: None,
        solarad_flag: None,
        solarad_max: None,
        solarad_max_flag: None,
        solarad_min: None,
        solarad_min_flag: None,
        sur_temp_type: None,
        sur_temp: None,
        sur_temp_flag: None,
        sur_temp_max: None,
        sur_temp_max_flag: None,
        sur_temp_min: None,
        sur_temp_min_flag: None,
        rh_hr_avg: None,
        rh_hr_avg_flag: None,
        soil_moisture_5: None,
        soil_moisture_10: None,
        soil_moisture_20: None,
        soil_moisture_50: None,
        soil_moisture_100: None,
        soil_temp_5: None,
        soil_temp_10: None,
        soil_temp_20: None,
        soil_temp_50: None,
        soil_temp_100: None,
        source_file_id: None,
    }];

    repo.insert_observations(&observation, file_id)
        .await
        .expect("Initial insert failed");

    // Update with new temperature
    let updated_observation = vec![NewObservation {
        wbanno: 53104,
        utc_datetime: timestamp,
        lst_datetime: timestamp,
        crx_version: Some("3".to_string()),
        t_hr_avg: Some(25.0), // Updated value
        t_calc: None,
        t_max: None,
        t_min: None,
        p_calc: None,
        solarad: None,
        solarad_flag: None,
        solarad_max: None,
        solarad_max_flag: None,
        solarad_min: None,
        solarad_min_flag: None,
        sur_temp_type: None,
        sur_temp: None,
        sur_temp_flag: None,
        sur_temp_max: None,
        sur_temp_max_flag: None,
        sur_temp_min: None,
        sur_temp_min_flag: None,
        rh_hr_avg: None,
        rh_hr_avg_flag: None,
        soil_moisture_5: None,
        soil_moisture_10: None,
        soil_moisture_20: None,
        soil_moisture_50: None,
        soil_moisture_100: None,
        soil_temp_5: None,
        soil_temp_10: None,
        soil_temp_20: None,
        soil_temp_50: None,
        soil_temp_100: None,
        source_file_id: None,
    }];

    repo.insert_observations(&updated_observation, file_id)
        .await
        .expect("Update failed");

    // Verify update - should still be only 1 row, but with updated value
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations WHERE wbanno = $1")
        .bind(53104)
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 1);

    let temp = sqlx::query_scalar::<_, Option<f32>>(
        "SELECT t_hr_avg FROM observations WHERE wbanno = $1 AND utc_datetime = $2",
    )
    .bind(53104)
    .bind(timestamp)
    .fetch_one(&pool)
    .await
    .expect("Temp query failed");

    assert_eq!(temp, Some(25.0));
}

/// Test batch insert with large number of observations
#[sqlx::test]
async fn test_large_batch_insert(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Insert station
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "CA".to_string(),
        latitude: None,
        longitude: None,
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // Create processed file
    let file = NewProcessedFile {
        file_name: "large_test.txt".to_string(),
        file_url: "https://example.com/large_test.txt".to_string(),
        year: 2024,
        state: "CA".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 2000,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: 0,
        processing_status: "processing".to_string(),
    };
    let file_id = repo
        .mark_file_processed(file)
        .await
        .expect("File insert failed");

    // Create 2000 observations (tests batching logic)
    let mut observations = Vec::new();
    let base_time = Utc::now();

    for i in 0..2000 {
        observations.push(NewObservation {
            wbanno: 53104,
            utc_datetime: base_time + chrono::Duration::hours(i),
            lst_datetime: base_time + chrono::Duration::hours(i),
            crx_version: Some("3".to_string()),
            t_hr_avg: Some(20.0 + (i as f32) * 0.1),
            t_calc: None,
            t_max: None,
            t_min: None,
            p_calc: None,
            solarad: None,
            solarad_flag: None,
            solarad_max: None,
            solarad_max_flag: None,
            solarad_min: None,
            solarad_min_flag: None,
            sur_temp_type: None,
            sur_temp: None,
            sur_temp_flag: None,
            sur_temp_max: None,
            sur_temp_max_flag: None,
            sur_temp_min: None,
            sur_temp_min_flag: None,
            rh_hr_avg: None,
            rh_hr_avg_flag: None,
            soil_moisture_5: None,
            soil_moisture_10: None,
            soil_moisture_20: None,
            soil_moisture_50: None,
            soil_moisture_100: None,
            soil_temp_5: None,
            soil_temp_10: None,
            soil_temp_20: None,
            soil_temp_50: None,
            soil_temp_100: None,
            source_file_id: None,
        });
    }

    let result = repo
        .insert_observations(&observations, file_id)
        .await
        .expect("Batch insert failed");

    assert_eq!(result.total_rows_affected, 2000);

    // Verify count
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations WHERE wbanno = $1")
        .bind(53104)
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 2000);
}

/// Test processed file tracking
#[sqlx::test]
async fn test_mark_file_processed(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    let file = NewProcessedFile {
        file_name: "CRNH0203-2024-CA_Test.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "CA".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 100,
        file_hash: Some("abc123".to_string()),
        observations_inserted: 95,
        observations_updated: 5,
        parse_failures: 2,
        processing_status: "completed".to_string(),
    };

    let file_id = repo
        .mark_file_processed(file)
        .await
        .expect("File insert failed");

    // Verify file was inserted
    let is_processed = repo
        .is_file_processed("CRNH0203-2024-CA_Test.txt")
        .await
        .expect("Check failed");

    assert!(is_processed);

    // Retrieve the file
    let retrieved = repo
        .get_processed_file("CRNH0203-2024-CA_Test.txt")
        .await
        .expect("Get failed")
        .expect("File not found");

    assert_eq!(retrieved.year, 2024);
    assert_eq!(retrieved.state, "CA");
    assert_eq!(retrieved.rows_processed, 100);
    assert_eq!(retrieved.file_hash, Some("abc123".to_string()));
    assert_eq!(retrieved.observations_inserted, Some(95));
    assert_eq!(retrieved.observations_updated, Some(5));
    assert_eq!(retrieved.parse_failures, Some(2));
    assert_eq!(retrieved.processing_status, Some("completed".to_string()));
}

/// Test get processed files for year
#[sqlx::test]
async fn test_get_processed_files_for_year(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Insert files for different years
    for year in [2022, 2023, 2024] {
        for i in 1..=3 {
            let file = NewProcessedFile {
                file_name: format!("file_{}_y{}.txt", i, year),
                file_url: format!("https://example.com/file_{}.txt", i),
                year,
                state: "CA".to_string(),
                station_name: "Test".to_string(),
                last_modified: None,
                rows_processed: 10,
                file_hash: None,
                observations_inserted: 10,
                observations_updated: 0,
                parse_failures: 0,
                processing_status: "completed".to_string(),
            };
            repo.mark_file_processed(file)
                .await
                .expect("File insert failed");
        }
    }

    // Get files for 2023
    let files_2023 = repo
        .get_processed_files_for_year(2023)
        .await
        .expect("Query failed");

    assert_eq!(files_2023.len(), 3);
    assert!(files_2023.contains(&"file_1_y2023.txt".to_string()));
    assert!(files_2023.contains(&"file_2_y2023.txt".to_string()));
    assert!(files_2023.contains(&"file_3_y2023.txt".to_string()));

    // Get files for 2024
    let files_2024 = repo
        .get_processed_files_for_year(2024)
        .await
        .expect("Query failed");

    assert_eq!(files_2024.len(), 3);
}
