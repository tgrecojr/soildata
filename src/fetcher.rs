use crate::bronze::{slugify, Bronze, CaptureMeta};
use crate::config::LocationFilter;
use crate::error::{AppError, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Bronze `source` segment for all USCRN captures.
const BRONZE_SOURCE: &str = "uscrn";

/// Allowed hostnames for NOAA data fetching (prevents SSRF attacks)
const ALLOWED_HOSTS: &[&str] = &[
    "www.ncei.noaa.gov",
    "ncei.noaa.gov",
    "www1.ncdc.noaa.gov",
    "ncdc.noaa.gov",
];

/// Validate that a URL is from an allowed NOAA host
fn validate_url(url: &str) -> Result<()> {
    let parsed =
        url::Url::parse(url).map_err(|e| AppError::InvalidData(format!("Invalid URL: {}", e)))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::InvalidData("URL missing host".to_string()))?;

    if !ALLOWED_HOSTS.contains(&host) {
        return Err(AppError::InvalidData(format!(
            "URL host '{}' not in allowed list. Expected one of: {}",
            host,
            ALLOWED_HOSTS.join(", ")
        )));
    }

    // Ensure HTTPS
    if parsed.scheme() != "https" {
        return Err(AppError::InvalidData(format!(
            "URL must use HTTPS, got: {}",
            parsed.scheme()
        )));
    }

    Ok(())
}

pub struct Fetcher {
    client: Client,
    base_url: String,
    bronze: Arc<Bronze>,
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
    pub fn new(base_url: &str, bronze: Arc<Bronze>) -> Result<Self> {
        let client = Client::builder()
            .user_agent("uscrn-ingest/0.1.0")
            .timeout(std::time::Duration::from_secs(60))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            bronze,
        })
    }

    /// Download a USCRN data file, capturing the raw bytes to the bronze layer
    /// before decoding for the parser.
    ///
    /// # Arguments
    /// * `file_info` - The file to download (its `url` must be an allowed NOAA host)
    ///
    /// # Returns
    /// The file content as a decoded string, for the existing parser.
    ///
    /// # Errors
    /// Returns error if URL validation fails or download fails. A bronze capture
    /// failure is non-fatal and never surfaces here.
    pub async fn download_file(&self, file_info: &FileInfo) -> Result<String> {
        let url = &file_info.url;
        debug!("Downloading file from {}", url);

        // Validate URL before making request
        validate_url(url)?;

        // collection = the per-station dataset, e.g. "pa_avondale_2_n".
        let collection = slugify(&format!("{}_{}", file_info.state, file_info.station_name));

        retry_with_backoff(3, || async {
            let response = self.client.get(url).send().await?;

            if !response.status().is_success() {
                return Err(AppError::Http(response.error_for_status().unwrap_err()));
            }

            // Read response provenance from headers before consuming the body.
            let http_status = response.status().as_u16();
            let content_type = header_value(&response, reqwest::header::CONTENT_TYPE);
            let charset = content_type.as_deref().and_then(charset_from_content_type);
            // No `gzip` reqwest feature is enabled, so the server never gzips for
            // transport; the body is delivered as-is (`identity`).
            let content_encoding = header_value(&response, reqwest::header::CONTENT_ENCODING)
                .unwrap_or_else(|| "identity".to_string());

            // Capture the EXACT unparsed bytes, never a decoded string.
            let bytes = response.bytes().await?;

            // Bronze capture happens before processing and is best-effort:
            // a failure here is logged inside `capture` and never propagates.
            let meta = CaptureMeta {
                request_url: url.clone(),
                request_params: serde_json::json!({}),
                http_status,
                content_type,
                charset,
                content_encoding,
                // Native format is plain text; reqwest delivered it undecompressed.
                stored_encoding: "identity".to_string(),
                ext: "txt".to_string(),
                redacted_fields: vec![],
            };
            self.bronze
                .capture(BRONZE_SOURCE, &collection, &bytes, &meta)
                .await;

            // Decode for the existing parser. NOAA data is ASCII/UTF-8 and the
            // parser sanitizes per line, so lossy decoding is behavior-equivalent
            // to the previous `response.text()`.
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        })
        .await
    }

    pub async fn list_years(&self) -> Result<Vec<i32>> {
        retry_with_backoff(3, || async { self.list_years_impl().await }).await
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
        let selector = Selector::parse("a")
            .map_err(|e| AppError::Parse(format!("Selector error: {:?}", e)))?;

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
        let selector = Selector::parse("a")
            .map_err(|e| AppError::Parse(format!("Selector error: {:?}", e)))?;

        let mut files = Vec::new();

        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                if href.starts_with("CRNH") && href.ends_with(".txt") && filter.matches_file(href) {
                    if let Some(file_info) = parse_filename(href, year, &self.base_url) {
                        files.push(file_info);
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

/// Read a response header as an owned `String`, if present and valid UTF-8.
fn header_value(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Extract the `charset` parameter from a `Content-Type` value, lowercased.
/// e.g. `text/plain; charset=UTF-8` -> `utf-8`.
fn charset_from_content_type(content_type: &str) -> Option<String> {
    content_type.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("charset=")
            .or_else(|| part.strip_prefix("charset ="))
            .map(|c| c.trim().trim_matches('"').to_ascii_lowercase())
    })
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
        let result = parse_filename(
            "CRNH0203-2024-CA_Bodega_6_WSW.txt",
            2024,
            "https://example.com",
        );

        assert!(result.is_some());
        let file_info = result.unwrap();
        assert_eq!(file_info.name, "CRNH0203-2024-CA_Bodega_6_WSW.txt");
        assert_eq!(file_info.year, 2024);
        assert_eq!(file_info.state, "CA");
        assert_eq!(file_info.station_name, "Bodega_6_WSW");
    }

    #[test]
    fn test_parse_filename_texas() {
        let result = parse_filename(
            "CRNH0203-2024-TX_Austin_33_NW.txt",
            2024,
            "https://example.com",
        );

        assert!(result.is_some());
        let file_info = result.unwrap();
        assert_eq!(file_info.state, "TX");
        assert_eq!(file_info.station_name, "Austin_33_NW");
    }
}
