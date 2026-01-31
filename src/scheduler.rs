use crate::config::Config;
use crate::db::models::{NewProcessedFile, NewStation};
use crate::db::Repository;
use crate::error::Result;
use crate::fetcher::Fetcher;
use crate::parser::Parser;
use chrono::Datelike;
use std::sync::Arc;
use tokio::sync::watch;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

pub struct Scheduler {
    config: Config,
    repository: Arc<Repository>,
    shutdown_rx: watch::Receiver<bool>,
}

impl Scheduler {
    pub fn new(
        config: Config,
        repository: Arc<Repository>,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            repository,
            shutdown_rx,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        let initial_delay = Duration::from_secs(self.config.scheduler.initial_delay_seconds);
        let poll_interval = Duration::from_secs(self.config.scheduler.interval_minutes * 60);

        info!(
            "Scheduler starting with {}s initial delay, {}m interval",
            self.config.scheduler.initial_delay_seconds, self.config.scheduler.interval_minutes
        );

        // Initial delay
        tokio::select! {
            _ = tokio::time::sleep(initial_delay) => {},
            _ = self.shutdown_rx.changed() => {
                info!("Shutdown received during initial delay");
                return Ok(());
            }
        }

        // Run immediately, then on interval
        if let Err(e) = self.run_ingestion().await {
            error!("Ingestion error: {}", e);
        }

        let mut ticker = interval(poll_interval);
        ticker.tick().await; // First tick is immediate, skip it

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(e) = self.run_ingestion().await {
                        error!("Ingestion error: {}", e);
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    info!("Shutdown signal received, stopping scheduler");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn run_ingestion(&self) -> Result<()> {
        info!("Starting ingestion run");

        let fetcher = Fetcher::new(&self.config.source.base_url)?;
        let years_to_process = self.config.source.years_to_fetch.get_years();

        info!("Processing years: {:?}", years_to_process);

        for year in years_to_process {
            if let Err(e) = self.process_year(&fetcher, year).await {
                error!("Error processing year {}: {}", year, e);
            }
        }

        info!("Ingestion run completed");
        Ok(())
    }

    async fn process_year(&self, fetcher: &Fetcher, year: i32) -> Result<()> {
        let current_year = chrono::Utc::now().year();
        let is_current_year = year == current_year;

        if is_current_year {
            info!(
                "Processing year {} (current year - will re-process all files for updates)",
                year
            );
        } else {
            info!("Processing year {} (historical - will skip processed files)", year);
        }

        let files = fetcher
            .list_files_for_year(year, &self.config.locations)
            .await?;

        // Fetch all processed files for this year in one query
        // For current year, we'll still track but won't skip (to handle updates)
        let processed_files: std::collections::HashSet<String> = self
            .repository
            .get_processed_files_for_year(year)
            .await?
            .into_iter()
            .collect();

        let mut processed_count = 0;
        let mut skipped_count = 0;
        let mut updated_count = 0;

        for file_info in files {
            let already_processed = processed_files.contains(&file_info.name);

            // Skip already-processed files ONLY for past years
            // Current year files are always re-processed to capture new hourly data
            if !is_current_year && already_processed {
                skipped_count += 1;
                continue;
            }

            if already_processed {
                info!("Re-processing file (current year): {}", file_info.name);
            } else {
                info!("Processing file: {}", file_info.name);
            }

            match self.process_file(fetcher, &file_info).await {
                Ok(rows) => {
                    info!("Processed {} observations from {}", rows, file_info.name);
                    if already_processed {
                        updated_count += 1;
                    } else {
                        processed_count += 1;
                    }
                }
                Err(e) => {
                    error!("Error processing {}: {}", file_info.name, e);
                }
            }

            // Rate limiting: delay between file downloads
            if self.config.source.request_delay_ms > 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    self.config.source.request_delay_ms,
                ))
                .await;
            }
        }

        if is_current_year {
            info!(
                "Year {} complete: {} new files, {} updated files, {} skipped",
                year, processed_count, updated_count, skipped_count
            );
        } else {
            info!(
                "Year {} complete: {} files processed, {} skipped",
                year, processed_count, skipped_count
            );
        }

        Ok(())
    }

    async fn process_file(
        &self,
        fetcher: &Fetcher,
        file_info: &crate::fetcher::FileInfo,
    ) -> Result<usize> {
        // Download file
        let content = fetcher.download_file(&file_info.url).await?;

        // Parse observations
        let (mut observations, parse_stats) = Parser::parse_file(&content)?;

        info!(
            "Parsed {} from {}: {} successful, {} failures ({:.1}% success rate)",
            file_info.name,
            parse_stats.total_lines,
            parse_stats.parsed_successfully,
            parse_stats.parse_failures,
            (parse_stats.parsed_successfully as f64
                / (parse_stats.total_lines - parse_stats.empty_lines) as f64)
                * 100.0
        );

        // Filter observations by station (WBANNO) if configured
        let observations_before_filter = observations.len();
        observations.retain(|obs| self.config.locations.matches_station(obs.wbanno));

        if observations_before_filter > observations.len() {
            info!(
                "Station filter: kept {}/{} observations matching configured stations",
                observations.len(),
                observations_before_filter
            );
        }

        if observations.is_empty() {
            warn!("No observations remaining after filtering for {}", file_info.name);

            // Mark file as processed with failure status
            let failed_file = NewProcessedFile {
                file_name: file_info.name.clone(),
                file_url: file_info.url.clone(),
                year: file_info.year,
                state: file_info.state.clone(),
                station_name: file_info.station_name.clone(),
                last_modified: None,
                rows_processed: 0,
                file_hash: None,
                observations_inserted: 0,
                observations_updated: 0,
                parse_failures: parse_stats.parse_failures as i32,
                processing_status: "failed".to_string(),
            };
            self.repository.mark_file_processed(failed_file).await?;

            return Ok(0);
        }

        // Extract unique stations and batch upsert them
        let mut seen_stations = std::collections::HashMap::new();
        for obs in &observations {
            seen_stations.entry(obs.wbanno).or_insert_with(|| NewStation {
                wbanno: obs.wbanno,
                name: Some(file_info.station_name.clone()),
                state: file_info.state.clone(),
                latitude: None,
                longitude: None,
            });
        }

        // Batch upsert all unique stations in one query
        let stations: Vec<NewStation> = seen_stations.into_values().collect();
        if !stations.is_empty() {
            self.repository.batch_upsert_stations(&stations).await?;
        }

        // Create preliminary processed_file record to get file_id
        // Status is "processing" initially in case insertion fails
        let preliminary_file = NewProcessedFile {
            file_name: file_info.name.clone(),
            file_url: file_info.url.clone(),
            year: file_info.year,
            state: file_info.state.clone(),
            station_name: file_info.station_name.clone(),
            last_modified: None,
            rows_processed: observations.len() as i32,
            file_hash: None,
            observations_inserted: 0,
            observations_updated: 0,
            parse_failures: parse_stats.parse_failures as i32,
            processing_status: "processing".to_string(),
        };

        let file_id = self
            .repository
            .mark_file_processed(preliminary_file)
            .await?;

        // Insert observations - this is the critical step
        let insert_result = self
            .repository
            .insert_observations(&observations, file_id)
            .await?;

        info!(
            "Inserted observations for {}: {} inserted, {} updated, {} total affected",
            file_info.name,
            insert_result.inserted,
            insert_result.updated,
            insert_result.total_rows_affected
        );

        // Update processed_file record with final statistics
        let final_file = NewProcessedFile {
            file_name: file_info.name.clone(),
            file_url: file_info.url.clone(),
            year: file_info.year,
            state: file_info.state.clone(),
            station_name: file_info.station_name.clone(),
            last_modified: None,
            rows_processed: observations.len() as i32,
            file_hash: None,
            observations_inserted: insert_result.inserted as i32,
            observations_updated: insert_result.updated as i32,
            parse_failures: parse_stats.parse_failures as i32,
            processing_status: "completed".to_string(),
        };

        self.repository.mark_file_processed(final_file).await?;

        Ok(insert_result.total_rows_affected)
    }
}
