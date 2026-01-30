use crate::db::models::NewObservation;
use crate::error::{AppError, Result};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use tracing::warn;

const MISSING_VALUE: f32 = -9999.0;
const MISSING_VALUE_INT: i32 = -9999;

/// Default failure threshold - fail if more than 10% of lines fail to parse
const DEFAULT_FAILURE_THRESHOLD: f64 = 0.10;

#[derive(Debug, Clone)]
pub struct ParseStats {
    pub total_lines: usize,
    pub parsed_successfully: usize,
    pub parse_failures: usize,
    pub empty_lines: usize,
    pub failure_rate: f64,
}

impl ParseStats {
    pub fn new() -> Self {
        Self {
            total_lines: 0,
            parsed_successfully: 0,
            parse_failures: 0,
            empty_lines: 0,
            failure_rate: 0.0,
        }
    }

    pub fn finalize(&mut self) {
        let non_empty = self.total_lines - self.empty_lines;
        self.failure_rate = if non_empty > 0 {
            self.parse_failures as f64 / non_empty as f64
        } else {
            0.0
        };
    }

    pub fn exceeds_threshold(&self, threshold: f64) -> bool {
        self.failure_rate > threshold
    }
}

pub struct Parser;

impl Parser {
    /// Parse a USCRN data file and return observations with parse statistics
    pub fn parse_file(content: &str) -> Result<(Vec<NewObservation>, ParseStats)> {
        Self::parse_file_with_threshold(content, DEFAULT_FAILURE_THRESHOLD)
    }

    /// Parse a USCRN data file with a custom failure threshold
    pub fn parse_file_with_threshold(
        content: &str,
        failure_threshold: f64,
    ) -> Result<(Vec<NewObservation>, ParseStats)> {
        let mut observations = Vec::new();
        let mut stats = ParseStats::new();

        for (line_num, line) in content.lines().enumerate() {
            stats.total_lines += 1;

            let line = line.trim();
            if line.is_empty() {
                stats.empty_lines += 1;
                continue;
            }

            match Self::parse_line(line) {
                Ok(obs) => {
                    observations.push(obs);
                    stats.parsed_successfully += 1;
                }
                Err(e) => {
                    stats.parse_failures += 1;
                    warn!(
                        "Failed to parse line {} (failure {}/{}): {} - {}",
                        line_num + 1,
                        stats.parse_failures,
                        stats.total_lines - stats.empty_lines,
                        e,
                        line
                    );
                }
            }
        }

        stats.finalize();

        // Validate parse success rate
        if stats.exceeds_threshold(failure_threshold) {
            return Err(AppError::Parse(format!(
                "Parse failure rate {:.1}% exceeds threshold {:.1}%: {} failures out of {} non-empty lines",
                stats.failure_rate * 100.0,
                failure_threshold * 100.0,
                stats.parse_failures,
                stats.total_lines - stats.empty_lines
            )));
        }

        if observations.is_empty() && stats.total_lines > stats.empty_lines {
            return Err(AppError::Parse(
                "No observations successfully parsed from non-empty file".to_string(),
            ));
        }

        Ok((observations, stats))
    }

    fn parse_line(line: &str) -> Result<NewObservation> {
        let fields: Vec<&str> = line.split_whitespace().collect();

        if fields.len() < 28 {
            return Err(AppError::Parse(format!(
                "Expected at least 28 fields, got {}",
                fields.len()
            )));
        }

        // Parse required fields
        let wbanno = parse_int(fields[0])?;
        let utc_date = parse_int(fields[1])?;
        let utc_time = parse_int(fields[2])?;
        let lst_date = parse_int(fields[3])?;
        let lst_time = parse_int(fields[4])?;
        let crx_version = fields[5].to_string();

        // Parse datetime
        let utc_datetime = parse_datetime(utc_date, utc_time)?;
        let lst_datetime = parse_datetime(lst_date, lst_time)?;

        // Parse optional fields with missing value handling
        let t_calc = parse_optional_float(fields.get(8).copied());
        let t_hr_avg = parse_optional_float(fields.get(9).copied());
        let t_max = parse_optional_float(fields.get(10).copied());
        let t_min = parse_optional_float(fields.get(11).copied());
        let p_calc = parse_optional_float(fields.get(12).copied());

        let solarad = parse_optional_float(fields.get(13).copied());
        let solarad_flag = parse_optional_int(fields.get(14).copied());
        let solarad_max = parse_optional_float(fields.get(15).copied());
        let solarad_max_flag = parse_optional_int(fields.get(16).copied());
        let solarad_min = parse_optional_float(fields.get(17).copied());
        let solarad_min_flag = parse_optional_int(fields.get(18).copied());

        let sur_temp_type = fields.get(19).map(|s| s.to_string());
        let sur_temp = parse_optional_float(fields.get(20).copied());
        let sur_temp_flag = parse_optional_int(fields.get(21).copied());
        let sur_temp_max = parse_optional_float(fields.get(22).copied());
        let sur_temp_max_flag = parse_optional_int(fields.get(23).copied());
        let sur_temp_min = parse_optional_float(fields.get(24).copied());
        let sur_temp_min_flag = parse_optional_int(fields.get(25).copied());

        let rh_hr_avg = parse_optional_float(fields.get(26).copied());
        let rh_hr_avg_flag = parse_optional_int(fields.get(27).copied());

        // Soil moisture (5 depths)
        let soil_moisture_5 = parse_optional_float(fields.get(28).copied());
        let soil_moisture_10 = parse_optional_float(fields.get(29).copied());
        let soil_moisture_20 = parse_optional_float(fields.get(30).copied());
        let soil_moisture_50 = parse_optional_float(fields.get(31).copied());
        let soil_moisture_100 = parse_optional_float(fields.get(32).copied());

        // Soil temperature (5 depths)
        let soil_temp_5 = parse_optional_float(fields.get(33).copied());
        let soil_temp_10 = parse_optional_float(fields.get(34).copied());
        let soil_temp_20 = parse_optional_float(fields.get(35).copied());
        let soil_temp_50 = parse_optional_float(fields.get(36).copied());
        let soil_temp_100 = parse_optional_float(fields.get(37).copied());

        Ok(NewObservation {
            wbanno,
            utc_datetime,
            lst_datetime,
            crx_version: Some(crx_version),
            t_calc,
            t_hr_avg,
            t_max,
            t_min,
            p_calc,
            solarad,
            solarad_flag,
            solarad_max,
            solarad_max_flag,
            solarad_min,
            solarad_min_flag,
            sur_temp_type,
            sur_temp,
            sur_temp_flag,
            sur_temp_max,
            sur_temp_max_flag,
            sur_temp_min,
            sur_temp_min_flag,
            rh_hr_avg,
            rh_hr_avg_flag,
            soil_moisture_5,
            soil_moisture_10,
            soil_moisture_20,
            soil_moisture_50,
            soil_moisture_100,
            soil_temp_5,
            soil_temp_10,
            soil_temp_20,
            soil_temp_50,
            soil_temp_100,
            source_file_id: None,
        })
    }
}

fn parse_int(s: &str) -> Result<i32> {
    s.parse::<i32>()
        .map_err(|e| AppError::Parse(format!("Failed to parse int '{}': {}", s, e)))
}

fn parse_optional_int(s: Option<&str>) -> Option<i32> {
    s.and_then(|s| {
        let val = s.parse::<i32>().ok()?;
        if val == MISSING_VALUE_INT {
            None
        } else {
            Some(val)
        }
    })
}

fn parse_optional_float(s: Option<&str>) -> Option<f32> {
    s.and_then(|s| {
        let val = s.parse::<f32>().ok()?;
        if (val - MISSING_VALUE).abs() < 0.1 {
            None
        } else {
            Some(val)
        }
    })
}

fn parse_datetime(date: i32, time: i32) -> Result<chrono::DateTime<Utc>> {
    // Date format: YYYYMMDD
    // Time format: HHMM

    let year = date / 10000;
    let month = (date % 10000) / 100;
    let day = date % 100;

    let hour = time / 100;
    let minute = time % 100;

    // Validate ranges before creating date/time
    if year < 1900 || year > 2100 {
        return Err(AppError::Parse(format!(
            "Year {} out of valid range (1900-2100) from date {}",
            year, date
        )));
    }

    if month < 1 || month > 12 {
        return Err(AppError::Parse(format!(
            "Month {} out of valid range (1-12) from date {}",
            month, date
        )));
    }

    if day < 1 || day > 31 {
        return Err(AppError::Parse(format!(
            "Day {} out of valid range (1-31) from date {}",
            day, date
        )));
    }

    if hour > 23 {
        return Err(AppError::Parse(format!(
            "Hour {} out of valid range (0-23) from time {}",
            hour, time
        )));
    }

    if minute > 59 {
        return Err(AppError::Parse(format!(
            "Minute {} out of valid range (0-59) from time {}",
            minute, time
        )));
    }

    let naive_date = NaiveDate::from_ymd_opt(year, month as u32, day as u32).ok_or_else(|| {
        AppError::Parse(format!(
            "Invalid date combination: year={}, month={}, day={} from {}",
            year, month, day, date
        ))
    })?;

    let naive_time = NaiveTime::from_hms_opt(hour as u32, minute as u32, 0).ok_or_else(|| {
        AppError::Parse(format!(
            "Invalid time combination: hour={}, minute={} from {}",
            hour, minute, time
        ))
    })?;

    let naive_datetime = NaiveDateTime::new(naive_date, naive_time);

    Ok(Utc.from_utc_datetime(&naive_datetime))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_datetime() {
        let result = parse_datetime(20240115, 1430).unwrap();
        assert_eq!(
            result.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-01-15 14:30:00"
        );
    }

    #[test]
    fn test_parse_optional_float_missing() {
        assert_eq!(parse_optional_float(Some("-9999.0")), None);
        assert_eq!(parse_optional_float(Some("-9999")), None);
    }

    #[test]
    fn test_parse_optional_float_valid() {
        assert_eq!(parse_optional_float(Some("25.5")), Some(25.5));
        assert_eq!(parse_optional_float(Some("0.0")), Some(0.0));
    }

    #[test]
    fn test_parse_line() {
        // Sample line from USCRN data
        let line = "53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

        let result = Parser::parse_line(line);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());

        let obs = result.unwrap();
        assert_eq!(obs.wbanno, 53104);
        assert_eq!(obs.t_hr_avg, Some(4.1));
        assert_eq!(obs.t_max, Some(4.9));
        assert_eq!(obs.t_min, Some(3.4));
        assert_eq!(obs.p_calc, Some(0.0));
        assert_eq!(obs.t_calc, None); // -9999.0 should be None
        assert_eq!(obs.soil_moisture_5, None); // -9999.0 should be None
    }

    #[test]
    fn test_parse_file_with_stats() {
        let content = "53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0\n\
                        53104 20240115 1500 20240115 0700 3   -81.74    36.53  -9999.0     4.5     5.2     4.0     0.0    52.3 0    65.4 0    42.1 0 C     1.8 0     2.5 0    -0.2 0    78.5 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0";

        let result = Parser::parse_file(content);
        assert!(result.is_ok());

        let (observations, stats) = result.unwrap();
        assert_eq!(observations.len(), 2);
        assert_eq!(stats.parsed_successfully, 2);
        assert_eq!(stats.parse_failures, 0);
        assert!(stats.failure_rate < 0.01);
    }

    #[test]
    fn test_parse_file_failure_threshold() {
        // Mix of valid and invalid lines
        let content = "invalid line 1\n\
                        53104 20240115 1400 20240115 0600 3   -81.74    36.53  -9999.0     4.1     4.9     3.4     0.0    45.5 0    58.6 0    35.9 0 C     1.1 0     2.1 0    -0.5 0    81.9 0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0   -9999.0\n\
                        invalid line 2\n\
                        invalid line 3";

        // Should fail with default 10% threshold (3 failures out of 4 lines = 75%)
        let result = Parser::parse_file(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds threshold"));
    }
}
