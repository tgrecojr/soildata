# USCRN Data Ingestion Service

A Rust application that periodically fetches hourly climate data from NOAA's US Climate Reference Network (USCRN) and stores it in PostgreSQL.

## Features

- Automated polling of NOAA USCRN hourly data
- Configurable location filtering (by state, station ID, or pattern)
- Idempotent processing (tracks processed files)
- Graceful shutdown handling
- Docker deployment ready

## Quick Start

### Using Docker Compose

1. Clone the repository and navigate to the project:
   ```bash
   cd soildata
   ```

2. Copy and configure environment variables:
   ```bash
   cp .env.example .env
   # Edit .env if needed
   ```

3. Start the services:
   ```bash
   docker-compose up --build
   ```

### Running Locally

1. Install Rust 1.84+ and PostgreSQL

2. Create the database:
   ```bash
   createdb uscrn
   ```

3. Set environment variables:
   ```bash
   export DATABASE_URL=postgres://user:pass@localhost:5432/uscrn
   export RUST_LOG=info,uscrn_ingest=debug
   ```

4. Run the application:
   ```bash
   cargo run
   ```

## Configuration

Edit `config/config.yaml` to customize the ingestion:

```yaml
# Polling frequency
scheduler:
  interval_minutes: 60

# Data source
source:
  base_url: "https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/"
  years_to_fetch: "current"  # "all", "current", or [2024, 2025]

# Location filtering (empty = all locations)
locations:
  states: ["CA", "TX"]  # Filter by state
  stations: []          # Filter by WBANNO IDs
  patterns: []          # Filter by glob patterns
```

## Database Schema

The application creates three tables:

- **stations**: Station metadata (ID, name, location)
- **observations**: Hourly climate measurements
- **processed_files**: Tracking of ingested files

## Data Fields

Key observation fields from USCRN:

| Field | Description | Unit |
|-------|-------------|------|
| t_calc | Calculated temperature | °C |
| t_hr_avg | Average hourly temperature | °C |
| p_calc | Precipitation | mm |
| solarad | Solar radiation | W/m² |
| rh_hr_avg | Relative humidity | % |
| soil_moisture_* | Soil moisture at 5-100cm | fraction |
| soil_temp_* | Soil temperature at 5-100cm | °C |

## Querying Data

Example queries after data is ingested:

```sql
-- Recent observations
SELECT * FROM observations ORDER BY utc_datetime DESC LIMIT 10;

-- Station summary
SELECT s.name, s.state, COUNT(o.id) as obs_count
FROM stations s
JOIN observations o ON s.wbanno = o.wbanno
GROUP BY s.wbanno, s.name, s.state;

-- Temperature by station
SELECT s.name, AVG(o.t_hr_avg) as avg_temp
FROM stations s
JOIN observations o ON s.wbanno = o.wbanno
WHERE o.utc_datetime > NOW() - INTERVAL '7 days'
GROUP BY s.wbanno, s.name;
```

## Troubleshooting

### Error: "Missing required environment variable: DATABASE_URL"

**Cause**: The `DATABASE_URL` environment variable is not set.

**Solution**:
1. Create a `.env` file by copying `.env.example`:
   ```bash
   cp .env.example .env
   ```
2. Edit `.env` and set your database credentials:
   ```
   DATABASE_URL=postgres://username:password@localhost:5432/uscrn
   ```

Alternatively, export the variable before running:
```bash
export DATABASE_URL=postgres://username:password@localhost:5432/uscrn
cargo run
```

### Error: "Failed to connect to database"

**Cause**: PostgreSQL is not running or connection details are incorrect.

**Solutions**:
1. Ensure PostgreSQL is running:
   ```bash
   # Check if postgres is running
   pg_isadmin
   ```

2. Create the database if it doesn't exist:
   ```bash
   createdb uscrn
   ```

3. Verify connection string format:
   ```
   postgres://username:password@host:port/database
   ```

4. Test connection manually:
   ```bash
   psql postgres://username:password@localhost:5432/uscrn
   ```

### Error: "Failed to read config file"

**Cause**: The `config/config.yaml` file is missing or unreadable.

**Solution**: Ensure you're running from the project root where `config/` directory exists.

## Development

```bash
# Run tests
cargo test

# Build release
cargo build --release

# Check formatting
cargo fmt --check

# Run clippy
cargo clippy
```

## License

MIT
