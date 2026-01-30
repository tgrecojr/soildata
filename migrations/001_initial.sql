-- Track processed files to avoid re-processing
CREATE TABLE IF NOT EXISTS processed_files (
    id SERIAL PRIMARY KEY,
    file_name VARCHAR(255) UNIQUE NOT NULL,
    file_url TEXT NOT NULL,
    year INTEGER NOT NULL,
    state VARCHAR(2) NOT NULL,
    station_name VARCHAR(100) NOT NULL,
    last_modified TIMESTAMPTZ,
    rows_processed INTEGER NOT NULL DEFAULT 0,
    processed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    file_hash VARCHAR(64)
);

-- Station metadata
CREATE TABLE IF NOT EXISTS stations (
    wbanno INTEGER PRIMARY KEY,
    name VARCHAR(255),
    state VARCHAR(2) NOT NULL,
    latitude DOUBLE PRECISION,
    longitude DOUBLE PRECISION,
    first_seen TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Hourly observations (main data table)
CREATE TABLE IF NOT EXISTS observations (
    id BIGSERIAL PRIMARY KEY,
    wbanno INTEGER NOT NULL REFERENCES stations(wbanno),
    utc_datetime TIMESTAMPTZ NOT NULL,
    lst_datetime TIMESTAMPTZ NOT NULL,
    crx_version VARCHAR(10),

    -- Temperature (Celsius)
    t_calc REAL,
    t_hr_avg REAL,
    t_max REAL,
    t_min REAL,

    -- Precipitation (mm)
    p_calc REAL,

    -- Solar radiation (W/m^2)
    solarad REAL,
    solarad_flag INTEGER,
    solarad_max REAL,
    solarad_max_flag INTEGER,
    solarad_min REAL,
    solarad_min_flag INTEGER,

    -- Surface temperature
    sur_temp_type CHAR(1),
    sur_temp REAL,
    sur_temp_flag INTEGER,
    sur_temp_max REAL,
    sur_temp_max_flag INTEGER,
    sur_temp_min REAL,
    sur_temp_min_flag INTEGER,

    -- Humidity
    rh_hr_avg REAL,
    rh_hr_avg_flag INTEGER,

    -- Soil moisture (fractional water content)
    soil_moisture_5 REAL,
    soil_moisture_10 REAL,
    soil_moisture_20 REAL,
    soil_moisture_50 REAL,
    soil_moisture_100 REAL,

    -- Soil temperature (Celsius)
    soil_temp_5 REAL,
    soil_temp_10 REAL,
    soil_temp_20 REAL,
    soil_temp_50 REAL,
    soil_temp_100 REAL,

    -- Metadata
    source_file_id INTEGER REFERENCES processed_files(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(wbanno, utc_datetime)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_observations_datetime ON observations(utc_datetime);
CREATE INDEX IF NOT EXISTS idx_observations_station ON observations(wbanno);
CREATE INDEX IF NOT EXISTS idx_observations_station_datetime ON observations(wbanno, utc_datetime);
CREATE INDEX IF NOT EXISTS idx_processed_files_year ON processed_files(year);
