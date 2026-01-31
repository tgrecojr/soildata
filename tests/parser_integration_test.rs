use sqlx::PgPool;
use uscrn_ingest::db::models::{NewProcessedFile, NewStation};
use uscrn_ingest::db::Repository;
use uscrn_ingest::parser::Parser;

/// Test parsing a complete USCRN data file and inserting into database
#[sqlx::test]
async fn test_parse_and_insert_complete_flow(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Sample USCRN data (2 observations)
    let file_content = "\
53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0
53104 20240115 1500 20240115 0700 3   -81.74    36.53  -9999.0     4.5     5.2     4.0     0.0    52.3 0    65.4 0    42.1 0 C     1.8 0     2.5 0    -0.2 0    78.5 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

    // Parse the file
    let (observations, stats) = Parser::parse_file(file_content).expect("Parse failed");

    assert_eq!(observations.len(), 2);
    assert_eq!(stats.parsed_successfully, 2);
    assert_eq!(stats.parse_failures, 0);

    // Insert station first
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "NC".to_string(),
        latitude: Some(36.53),
        longitude: Some(-81.74),
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // Create processed file record
    let file = NewProcessedFile {
        file_name: "test_file.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "NC".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: observations.len() as i32,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: stats.parse_failures as i32,
        processing_status: "processing".to_string(),
    };
    let file_id = repo
        .mark_file_processed(file)
        .await
        .expect("File insert failed");

    // Insert observations
    let result = repo
        .insert_observations(&observations, file_id)
        .await
        .expect("Insert failed");

    assert_eq!(result.total_rows_affected, 2);

    // Verify the observations in database
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations WHERE wbanno = $1")
        .bind(53104)
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 2);

    // Verify specific values from first observation
    let (t_hr_avg, t_max, t_min) = sqlx::query_as::<_, (Option<f32>, Option<f32>, Option<f32>)>(
        "SELECT t_hr_avg, t_max, t_min FROM observations WHERE wbanno = $1 ORDER BY utc_datetime LIMIT 1",
    )
    .bind(53104)
    .fetch_one(&pool)
    .await
    .expect("Value query failed");

    assert_eq!(t_hr_avg, Some(4.1));
    assert_eq!(t_max, Some(4.9));
    assert_eq!(t_min, Some(3.4));
}

/// Test parsing file with missing values (-9999)
#[sqlx::test]
async fn test_parse_missing_values_stored_as_null(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    // Data with missing soil measurements
    let file_content = "\
53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

    let (observations, _) = Parser::parse_file(file_content).expect("Parse failed");

    // Insert station
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "NC".to_string(),
        latitude: None,
        longitude: None,
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // Create processed file
    let file = NewProcessedFile {
        file_name: "test_missing.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "NC".to_string(),
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

    repo.insert_observations(&observations, file_id)
        .await
        .expect("Insert failed");

    // Verify missing values are NULL in database
    let (t_calc, soil_moisture_5) = sqlx::query_as::<_, (Option<f32>, Option<f32>)>(
        "SELECT t_calc, soil_moisture_5 FROM observations WHERE wbanno = $1",
    )
    .bind(53104)
    .fetch_one(&pool)
    .await
    .expect("Query failed");

    assert_eq!(t_calc, None); // Was -9999.0, should be NULL
    assert_eq!(soil_moisture_5, None); // Was -9999.0, should be NULL
}

/// Test parsing file with high failure rate rejects the file
#[tokio::test]
async fn test_parse_high_failure_rate_rejects_file() {
    // Mix of valid and invalid lines - 75% failure rate
    let file_content = "\
invalid line without enough fields
53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0
another invalid line
yet another invalid line";

    let result = Parser::parse_file(file_content);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("exceeds threshold"));
}

/// Test parsing empty file
#[tokio::test]
async fn test_parse_empty_file() {
    let file_content = "";
    let result = Parser::parse_file(file_content);

    // Empty file returns Ok with empty observations
    assert!(result.is_ok());
    let (observations, stats) = result.unwrap();
    assert_eq!(observations.len(), 0);
    assert_eq!(stats.total_lines, 0);
}

/// Test parsing file with only whitespace
#[tokio::test]
async fn test_parse_whitespace_only_file() {
    let file_content = "   \n\n  \n\n";
    let result = Parser::parse_file(file_content);

    // Whitespace-only file returns Ok with empty observations
    assert!(result.is_ok());
    let (observations, stats) = result.unwrap();
    assert_eq!(observations.len(), 0);
    assert_eq!(stats.empty_lines, 4);
}

/// Test custom failure threshold
#[tokio::test]
async fn test_parse_custom_failure_threshold() {
    // 50% failure rate
    let file_content = "\
invalid line
53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

    // Default threshold (10%) should reject
    let result = Parser::parse_file(file_content);
    assert!(result.is_err());

    // Higher threshold (60%) should accept
    let result = Parser::parse_file_with_threshold(file_content, 0.60);
    assert!(result.is_ok());
    let (observations, stats) = result.unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(stats.parse_failures, 1);
}

/// Test that observations are properly deduplicated on re-import
#[sqlx::test]
async fn test_reimport_deduplicates_observations(pool: PgPool) {
    let repo = Repository::new(pool.clone());

    let file_content = "\
53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

    // Insert station
    let station = NewStation {
        wbanno: 53104,
        name: Some("Test Station".to_string()),
        state: "NC".to_string(),
        latitude: None,
        longitude: None,
    };
    repo.upsert_station(station)
        .await
        .expect("Station insert failed");

    // First import
    let (observations, _) = Parser::parse_file(file_content).expect("Parse failed");

    let file1 = NewProcessedFile {
        file_name: "test_dedup.txt".to_string(),
        file_url: "https://example.com/test.txt".to_string(),
        year: 2024,
        state: "NC".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 1,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: 0,
        processing_status: "processing".to_string(),
    };
    let file_id1 = repo
        .mark_file_processed(file1)
        .await
        .expect("File insert failed");

    repo.insert_observations(&observations, file_id1)
        .await
        .expect("First insert failed");

    // Second import (simulating re-processing same file)
    let file2 = NewProcessedFile {
        file_name: "test_dedup_v2.txt".to_string(),
        file_url: "https://example.com/test_v2.txt".to_string(),
        year: 2024,
        state: "NC".to_string(),
        station_name: "Test Station".to_string(),
        last_modified: None,
        rows_processed: 1,
        file_hash: None,
        observations_inserted: 0,
        observations_updated: 0,
        parse_failures: 0,
        processing_status: "processing".to_string(),
    };
    let file_id2 = repo
        .mark_file_processed(file2)
        .await
        .expect("File insert failed");

    repo.insert_observations(&observations, file_id2)
        .await
        .expect("Second insert failed");

    // Verify only 1 observation exists (deduplicated on wbanno + utc_datetime)
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM observations WHERE wbanno = $1")
        .bind(53104)
        .fetch_one(&pool)
        .await
        .expect("Count query failed");

    assert_eq!(count, 1, "Should have deduplicated the observation");
}
