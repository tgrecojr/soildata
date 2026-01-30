use crate::config::LocationFilter;
use crate::error::{AppError, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use std::time::Duration;
use tracing::{debug, info, warn};

pub struct Fetcher {
    client: Client,
    base_url: String,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub name: String,
    pub url: String,
    pub year: i32,
    pub state: String,
    pub station_name: String,
}

impl Fetcher {
    pub fn new(base_url: &str) -> Result<Self> {
        let client = Client::builder()
            .user_agent("uscrn-ingest/0.1.0")
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn download_file(&self, url: &str) -> Result<String> {
        debug!("Downloading file from {}", url);

        retry_with_backoff(3, || async {
            let response = self.client.get(url).send().await?;

            if !response.status().is_success() {
                return Err(AppError::Http(
                    response.error_for_status().unwrap_err(),
                ));
            }

            let content = response.text().await?;
            Ok(content)
        })
        .await
    }

    pub async fn list_years(&self) -> Result<Vec<i32>> {
        retry_with_backoff(3, || async {
            self.list_years_impl().await
        })
        .await
    }

    pub async fn list_files_for_year(
        &self,
        year: i32,
        filter: &LocationFilter,
    ) -> Result<Vec<FileInfo>> {
        let filter = filter.clone();
        retry_with_backoff(3, || async {
            self.list_files_for_year_impl(year, &filter).await
        })
        .await
    }

    async fn list_years_impl(&self) -> Result<Vec<i32>> {
        let url = format!("{}/", self.base_url);
        debug!("Fetching year listing from {}", url);

        let response = self.client.get(&url).send().await?;
        let html = response.text().await?;

        let document = Html::parse_document(&html);
        let selector =
            Selector::parse("a").map_err(|e| AppError::Parse(format!("Selector error: {:?}", e)))?;

        let mut years = Vec::new();

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                let href = href.trim_end_matches('/');
                if let Ok(year) = href.parse::<i32>() {
                    if (2000..=2100).contains(&year) {
                        years.push(year);
                    }
                }
            }
        }

        years.sort();
        info!("Found {} years available", years.len());
        Ok(years)
    }

    async fn list_files_for_year_impl(
        &self,
        year: i32,
        filter: &LocationFilter,
    ) -> Result<Vec<FileInfo>> {
        let url = format!("{}/{}/", self.base_url, year);
        debug!("Fetching file listing for year {} from {}", year, url);

        let response = self.client.get(&url).send().await?;
        let html = response.text().await?;

        let document = Html::parse_document(&html);
        let selector =
            Selector::parse("a").map_err(|e| AppError::Parse(format!("Selector error: {:?}", e)))?;

        let mut files = Vec::new();

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if href.starts_with("CRNH") && href.ends_with(".txt") {
                    if filter.matches_file(href) {
                        if let Some(file_info) = parse_filename(href, year, &self.base_url) {
                            files.push(file_info);
                        }
                    }
                }
            }
        }

        info!(
            "Found {} files for year {} (after filtering)",
            files.len(),
            year
        );
        Ok(files)
    }
}

/// Retry a future with exponential backoff
async fn retry_with_backoff<F, Fut, T>(max_retries: u32, mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut retries = 0;
    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                retries += 1;

                if retries > max_retries {
                    return Err(e);
                }

                // Check if error is transient (retryable)
                let should_retry = match &e {
                    AppError::Http(reqwest_err) => {
                        // Retry on connection errors, timeouts, server errors (5xx)
                        reqwest_err.is_timeout()
                            || reqwest_err.is_connect()
                            || reqwest_err
                                .status()
                                .map(|s| s.is_server_error())
                                .unwrap_or(false)
                    }
                    AppError::Io(_) => true, // Retry IO errors
                    _ => false,              // Don't retry parse errors, config errors, etc.
                };

                if !should_retry {
                    return Err(e);
                }

                let delay = Duration::from_secs(2u64.pow(retries.saturating_sub(1)));
                warn!(
                    "Request failed (attempt {}/{}): {}. Retrying in {:?}...",
                    retries, max_retries, e, delay
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

fn parse_filename(filename: &str, year: i32, base_url: &str) -> Option<FileInfo> {
    // Format: CRNH0203-{YEAR}-{STATE}_{LOCATION}_{DISTANCE}_{DIRECTION}.txt
    // Example: CRNH0203-2024-CA_Bodega_6_WSW.txt

    let parts: Vec<&str> = filename.split('-').collect();
    if parts.len() < 3 {
        return None;
    }

    let location_part = parts[2];
    let location_parts: Vec<&str> = location_part.split('_').collect();

    if location_parts.is_empty() {
        return None;
    }

    let state = location_parts[0].to_string();

    // Build station name from remaining parts (excluding .txt)
    let station_name = if location_parts.len() > 1 {
        location_parts[1..]
            .join("_")
            .trim_end_matches(".txt")
            .to_string()
    } else {
        "Unknown".to_string()
    };

    Some(FileInfo {
        name: filename.to_string(),
        url: format!("{}/{}/{}", base_url, year, filename),
        year,
        state,
        station_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_filename() {
        let result =
            parse_filename("CRNH0203-2024-CA_Bodega_6_WSW.txt", 2024, "https://example.com");

        assert!(result.is_some());
        let file_info = result.unwrap();
        assert_eq!(file_info.name, "CRNH0203-2024-CA_Bodega_6_WSW.txt");
        assert_eq!(file_info.year, 2024);
        assert_eq!(file_info.state, "CA");
        assert_eq!(file_info.station_name, "Bodega_6_WSW");
    }

    #[test]
    fn test_parse_filename_texas() {
        let result =
            parse_filename("CRNH0203-2024-TX_Austin_33_NW.txt", 2024, "https://example.com");

        assert!(result.is_some());
        let file_info = result.unwrap();
        assert_eq!(file_info.state, "TX");
        assert_eq!(file_info.station_name, "Austin_33_NW");
    }
}
