use crate::error::{AppError, Result};
use serde::{Deserialize, Deserializer};
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database: DatabaseConfig,
    pub scheduler: SchedulerConfig,
    pub source: SourceConfig,
    #[serde(default)]
    pub locations: LocationFilter,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    #[serde(default = "default_db_port", deserialize_with = "deserialize_port")]
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_db_port() -> u16 {
    5432
}

fn default_max_connections() -> u32 {
    5
}

/// Custom deserializer that handles port as both number and string
///
/// Accepts:
/// - `port: 5432` (number)
/// - `port: "5432"` (string that parses to number)
/// - `port: ${DB_PORT}` (env var substituted to either)
fn deserialize_port<'de, D>(deserializer: D) -> std::result::Result<u16, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum PortValue {
        Number(u16),
        String(String),
    }

    match PortValue::deserialize(deserializer)? {
        PortValue::Number(n) => Ok(n),
        PortValue::String(s) => s
            .parse::<u16>()
            .map_err(|_| serde::de::Error::custom(format!("Invalid port number: '{}'", s))),
    }
}

impl DatabaseConfig {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.user, self.password, self.host, self.port, self.name
        )
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub interval_minutes: u64,
    #[serde(default = "default_initial_delay")]
    pub initial_delay_seconds: u64,
}

fn default_initial_delay() -> u64 {
    10
}

#[derive(Debug, Deserialize, Clone)]
pub struct SourceConfig {
    pub base_url: String,
    pub years_to_fetch: YearsConfig,
    #[serde(default = "default_request_delay_ms")]
    pub request_delay_ms: u64,
}

fn default_request_delay_ms() -> u64 {
    500 // 500ms delay between requests
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum YearsConfig {
    Keyword(String),
    Specific(Vec<i32>),
}

impl YearsConfig {
    pub fn get_years(&self) -> Vec<i32> {
        match self {
            YearsConfig::Keyword(keyword) => {
                let current_year = chrono::Utc::now().year();
                match keyword.as_str() {
                    "current" => vec![current_year],
                    "all" => (2000..=current_year).collect(),
                    _ => vec![current_year],
                }
            }
            YearsConfig::Specific(years) => years.clone(),
        }
    }
}

use chrono::Datelike;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct LocationFilter {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default)]
    pub stations: Vec<i32>,
    #[serde(default)]
    pub patterns: Vec<String>,
}

impl LocationFilter {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty() && self.stations.is_empty() && self.patterns.is_empty()
    }

    pub fn matches_file(&self, filename: &str) -> bool {
        if self.is_empty() {
            return true;
        }

        // If only station filter is set (not state or pattern), we need to download
        // the file to check WBANNO, so pass all files at this stage
        let has_file_level_filter = !self.states.is_empty() || !self.patterns.is_empty();
        if !has_file_level_filter {
            // Only station filter is set, will be applied after parsing
            return true;
        }

        // Extract state from filename: CRNH0203-{YEAR}-{STATE}_{LOCATION}...
        if let Some(state) = extract_state_from_filename(filename) {
            if !self.states.is_empty() && self.states.contains(&state.to_uppercase()) {
                return true;
            }
        }

        // Check patterns
        for pattern in &self.patterns {
            if glob::Pattern::new(pattern)
                .map(|p| p.matches(filename))
                .unwrap_or(false)
            {
                return true;
            }
        }

        false
    }

    pub fn matches_station(&self, wbanno: i32) -> bool {
        if self.is_empty() {
            return true;
        }
        if !self.stations.is_empty() {
            return self.stations.contains(&wbanno);
        }
        true
    }
}

fn extract_state_from_filename(filename: &str) -> Option<String> {
    // Format: CRNH0203-{YEAR}-{STATE}_{LOCATION}_{DISTANCE}_{DIRECTION}.txt
    let parts: Vec<&str> = filename.split('-').collect();
    if parts.len() >= 3 {
        let state_part = parts[2];
        if let Some(state) = state_part.split('_').next() {
            if state.len() == 2 {
                return Some(state.to_string());
            }
        }
    }
    None
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())
            .map_err(|e| AppError::Config(format!("Failed to read config file: {}", e)))?;

        // Substitute environment variables
        let expanded = expand_env_vars(&content)?;

        let config: Config = serde_yaml::from_str(&expanded)
            .map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))?;

        // Validate configuration
        config.validate()?;

        Ok(config)
    }

    /// Validate configuration values
    ///
    /// Checks for:
    /// - Unexpanded environment variables
    /// - Valid port ranges
    /// - Non-empty required fields
    /// - Positive time intervals
    /// - Valid URL formats
    fn validate(&self) -> Result<()> {
        // Check if any database field contains unexpanded environment variables
        let fields_to_check = [
            ("DB_HOST", &self.database.host),
            ("DB_NAME", &self.database.name),
            ("DB_USER", &self.database.user),
            ("DB_PASSWORD", &self.database.password),
        ];

        for (field_name, value) in &fields_to_check {
            if value.contains("${") {
                return Err(AppError::Config(format!(
                    "{} environment variable is not set. \
                     Please set it or create a .env file. \
                     See .env.example for required variables.",
                    field_name
                )));
            }
        }

        // Validate host is not empty
        if self.database.host.is_empty() {
            return Err(AppError::Config(
                "Database host cannot be empty".to_string(),
            ));
        }

        // Validate database name is not empty
        if self.database.name.is_empty() {
            return Err(AppError::Config(
                "Database name cannot be empty".to_string(),
            ));
        }

        // Validate user is not empty
        if self.database.user.is_empty() {
            return Err(AppError::Config(
                "Database user cannot be empty".to_string(),
            ));
        }

        // Validate port is not zero (u16 max is 65535, so no upper bound check needed)
        if self.database.port == 0 {
            return Err(AppError::Config("Database port cannot be 0".to_string()));
        }

        // Validate max_connections is reasonable
        if self.database.max_connections == 0 {
            return Err(AppError::Config(
                "Database max_connections must be at least 1".to_string(),
            ));
        }

        if self.database.max_connections > 100 {
            return Err(AppError::Config(format!(
                "Database max_connections {} seems too high, maximum recommended is 100",
                self.database.max_connections
            )));
        }

        // Validate scheduler interval is positive
        if self.scheduler.interval_minutes == 0 {
            return Err(AppError::Config(
                "Scheduler interval_minutes must be greater than 0".to_string(),
            ));
        }

        // Warn if interval is too short
        if self.scheduler.interval_minutes < 5 {
            tracing::warn!(
                "Scheduler interval of {} minutes is very short, consider using at least 5 minutes",
                self.scheduler.interval_minutes
            );
        }

        // Validate base URL format
        if let Err(e) = url::Url::parse(&self.source.base_url) {
            return Err(AppError::Config(format!(
                "Invalid source base_url '{}': {}",
                self.source.base_url, e
            )));
        }

        // Validate base URL is HTTPS
        if let Ok(parsed) = url::Url::parse(&self.source.base_url) {
            if parsed.scheme() != "https" {
                return Err(AppError::Config(format!(
                    "Source base_url must use HTTPS, got: {}",
                    parsed.scheme()
                )));
            }
        }

        // Validate state codes are 2 characters
        for state in &self.locations.states {
            if state.len() != 2 {
                return Err(AppError::Config(format!(
                    "State code '{}' must be exactly 2 characters (e.g., 'CA', 'TX')",
                    state
                )));
            }
        }

        Ok(())
    }
}

fn expand_env_vars(content: &str) -> Result<String> {
    let mut result = content.to_string();
    let re = regex_lite::Regex::new(r"\$\{([^}]+)\}").unwrap();

    let mut missing_vars = Vec::new();

    for cap in re.captures_iter(content) {
        let var_name = &cap[1];
        match std::env::var(var_name) {
            Ok(value) => {
                result = result.replace(&cap[0], &value);
            }
            Err(_) => {
                missing_vars.push(var_name.to_string());
            }
        }
    }

    if !missing_vars.is_empty() {
        return Err(AppError::Config(format!(
            "Missing required environment variable{}: {}\n\n\
             To fix this:\n\
             1. Create a .env file in the project root (copy .env.example)\n\
             2. Set the missing variable{}: export {}=<value>\n\
             3. Or set {} in your environment before running",
            if missing_vars.len() > 1 { "s" } else { "" },
            missing_vars.join(", "),
            if missing_vars.len() > 1 { "s" } else { "" },
            missing_vars[0],
            missing_vars.join(", ")
        )));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_state_from_filename() {
        assert_eq!(
            extract_state_from_filename("CRNH0203-2024-CA_Bodega_6_WSW.txt"),
            Some("CA".to_string())
        );
        assert_eq!(
            extract_state_from_filename("CRNH0203-2024-TX_Austin_33_NW.txt"),
            Some("TX".to_string())
        );
    }

    #[test]
    fn test_location_filter_matches() {
        let filter = LocationFilter {
            states: vec!["CA".to_string(), "TX".to_string()],
            stations: vec![],
            patterns: vec![],
        };

        assert!(filter.matches_file("CRNH0203-2024-CA_Bodega_6_WSW.txt"));
        assert!(filter.matches_file("CRNH0203-2024-TX_Austin_33_NW.txt"));
        assert!(!filter.matches_file("CRNH0203-2024-FL_Everglades_5_NE.txt"));
    }

    #[test]
    fn test_empty_filter_matches_all() {
        let filter = LocationFilter::default();
        assert!(filter.matches_file("CRNH0203-2024-CA_Bodega_6_WSW.txt"));
        assert!(filter.matches_station(12345));
    }

    #[test]
    fn test_station_only_filter_passes_all_files() {
        // When only station filter is set, files can't be filtered by name
        // (WBANNO is in file content), so all files should pass
        let filter = LocationFilter {
            states: vec![],
            stations: vec![3761],
            patterns: vec![],
        };
        assert!(filter.matches_file("CRNH0203-2024-PA_Avondale_2_N.txt"));
        assert!(filter.matches_file("CRNH0203-2024-CA_Bodega_6_WSW.txt"));
        assert!(filter.matches_station(3761)); // Passes station filter
        assert!(!filter.matches_station(12345)); // Fails station filter
    }

    #[test]
    fn test_port_deserialize_from_number() {
        let yaml = r#"
host: localhost
port: 5432
name: test
user: test
password: test
"#;
        let config: DatabaseConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.port, 5432);
    }

    #[test]
    fn test_port_deserialize_from_string() {
        let yaml = r#"
host: localhost
port: "5432"
name: test
user: test
password: test
"#;
        let config: DatabaseConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.port, 5432);
    }

    #[test]
    fn test_port_deserialize_invalid_string() {
        let yaml = r#"
host: localhost
port: "not_a_number"
name: test
user: test
password: test
"#;
        let result: std::result::Result<DatabaseConfig, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid port number") || err_msg.contains("not_a_number"));
    }
}
