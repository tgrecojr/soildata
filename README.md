# USCRN Data Ingestion Service

A Rust application that periodically fetches hourly climate data from NOAA's US Climate Reference Network (USCRN) and stores it in PostgreSQL.

## Features

- Automated polling of NOAA USCRN hourly data
- Configurable location filtering (by state, station ID, or pattern)
- Idempotent processing (tracks processed files)
- Graceful shutdown handling
- Docker deployment ready
- **Grafana dashboard** for gardening/landscaping decisions

## Quick Start

### Using Docker Compose (Recommended)

1. Clone the repository and navigate to the project:
   ```bash
   cd soildata
   ```

2. Create environment file (`.env`):
   ```bash
   cat > .env << 'EOF'
# Database Configuration
DB_HOST=db
DB_PORT=5432
DB_NAME=uscrn
DB_USER=uscrn
DB_PASSWORD=changeme_secure_password

# Logging Level
RUST_LOG=info,uscrn_ingest=debug,sqlx=warn

# Docker Compose Defaults
POSTGRES_USER=uscrn
POSTGRES_PASSWORD=changeme_secure_password
POSTGRES_DB=uscrn

# Grafana (optional)
GRAFANA_ADMIN_USER=admin
GRAFANA_ADMIN_PASSWORD=admin
EOF
   ```
   **⚠️ Important**: Change `DB_PASSWORD` and `POSTGRES_PASSWORD` to a secure password!

   Or manually create `.env` with the content above.

3. Start the services:
   ```bash
   docker-compose up --build
   ```

4. View logs:
   ```bash
   docker-compose logs -f
   ```

5. Access Grafana dashboard:
   - URL: http://localhost:3000
   - Login: admin/admin (or values from `.env`)
   - Dashboard: "Gardening Weather Dashboard" in the Gardening folder

6. Stop services:
   ```bash
   docker-compose down
   ```

### Docker Commands Reference

```bash
# Build and start services in foreground
docker-compose up --build

# Build and start services in background (detached)
docker-compose up --build -d

# View logs
docker-compose logs -f          # All services
docker-compose logs -f app      # App only
docker-compose logs -f db       # Database only
docker-compose logs -f grafana  # Grafana only

# Stop services (keeps volumes)
docker-compose down

# Stop services and remove volumes (⚠️ deletes data)
docker-compose down -v

# Restart services
docker-compose restart

# Check service status
docker-compose ps

# Open shell in app container
docker-compose exec app /bin/bash

# Open PostgreSQL shell
docker-compose exec db psql -U uscrn -d uscrn

# Rebuild after code changes
docker-compose up --build

# Pull latest images
docker-compose pull

# Remove everything (containers, volumes, images)
docker-compose down -v --rmi all
```

### Build Docker Image Standalone

```bash
# Build the image (uses cargo-chef for optimal caching)
docker build -t uscrn-ingest:latest .

# Run with external database (replace with your DB details)
docker run --rm \
  -e DB_HOST=your-db-host \
  -e DB_PORT=5432 \
  -e DB_NAME=uscrn \
  -e DB_USER=uscrn \
  -e DB_PASSWORD=your-password \
  -v $(pwd)/config:/app/config:ro \
  uscrn-ingest:latest

# Tag and push to registry
docker tag uscrn-ingest:latest your-registry/uscrn-ingest:v1.0.0
docker push your-registry/uscrn-ingest:v1.0.0
```

### Docker Build Strategy

The default `Dockerfile` uses **cargo-chef** for intelligent dependency caching and **Google's distroless** for minimal runtime:

**Multi-stage build**:
1. **Planner stage**: Analyzes `Cargo.toml` and generates dependency recipe
2. **Builder stage**: Builds dependencies (cached) then application with size optimizations
3. **Runtime stage**: Distroless image (~20MB) with only the binary

**Size optimizations**:
- ✅ Binary stripped of debug symbols (via Cargo profile)
- ✅ Link-Time Optimization (LTO) enabled
- ✅ Size-optimized compilation (`opt-level = "z"`)
- ✅ Distroless runtime (no shell, package manager, or unnecessary tools)
- ✅ Single static binary deployment

**Security benefits**:
- ✅ Minimal attack surface (distroless has ~50% fewer CVEs than debian:slim)
- ✅ No shell or package manager in production
- ✅ Runs as non-root user by default
- ✅ Immutable infrastructure

**Benefits**:
- ✅ Production-ready (industry best practices)
- ✅ Fast rebuilds when only code changes (~30-60 seconds)
- ✅ Small final image (~50-80MB vs 200MB+ with debian:slim)
- ✅ Efficient CI/CD with layer caching

### Running Locally

1. Install Rust (latest stable) and PostgreSQL
   ```bash
   # Install Rust via rustup
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

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

1. Copy the example configuration:
   ```bash
   cp config/config.yaml.example config/config.yaml
   ```

2. Edit `config/config.yaml` to customize the ingestion (this file is gitignored):

```yaml
# Database connection (supports environment variable substitution)
database:
  host: "${DB_HOST}"
  port: "${DB_PORT}"  # Accepts both number (5432) and string ("5432")
  name: "${DB_NAME}"
  user: "${DB_USER}"
  password: "${DB_PASSWORD}"

# Polling frequency
scheduler:
  interval_minutes: 60

# Data source
source:
  base_url: "https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/"
  years_to_fetch: "current"  # "all", "current", or [2024, 2025]

# Location filtering (empty = all locations)
locations:
  states: []     # Filter by state (2-letter codes)
  stations: []   # Filter by WBANNO station IDs
  patterns: []   # Filter by filename glob patterns
```

**Note**: The port field is robust and accepts both numeric and string values. This handles environment variable substitution gracefully whether the value comes in as `5432` or `"5432"`.

### Location Filtering

You can filter data collection in three ways. Leave all arrays empty to collect data from all stations.

#### 1. Filter by State

Collect data from all stations in specific states using 2-letter state codes:

```yaml
locations:
  states: ["CA", "TX", "PA"]
  stations: []
  patterns: []
```

This will download files like:
- `CRNH0203-2026-CA_Bodega_6_WSW.txt`
- `CRNH0203-2026-TX_Austin_33_NW.txt`
- `CRNH0203-2026-PA_Avondale_2_N.txt`

#### 2. Filter by Station ID (WBANNO)

Collect data from specific stations using their WBANNO identifier:

```yaml
locations:
  states: []
  stations: [3761, 54762, 93107]
  patterns: []
```

**Important**:
- WBANNO values in NOAA files appear with leading zeros (e.g., `03761`), but you must specify them **without** leading zeros in the config (e.g., `3761`). The application automatically handles the conversion.
- Station filtering happens **after** downloading files (WBANNO is inside file content), so all files will be listed and downloaded, then filtered during processing. For efficiency, combine with state or pattern filters if possible.

**Finding Station IDs:**
1. Browse available files at https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/2026/
2. Download a file for your desired station
3. The first column in the data file is the WBANNO
4. Use that number without leading zeros in your config

Example: Avondale, PA station shows `03761` in the data file → use `3761` in config

#### 3. Filter by Filename Pattern (Glob)

Use glob patterns to match specific filenames:

```yaml
locations:
  states: []
  stations: []
  patterns: ["*PA_Avondale*"]
```

**Pattern Examples:**

| Pattern | Matches |
|---------|---------|
| `*PA_Avondale*` | All Avondale, PA files across all years |
| `CRNH0203-2026-*.txt` | All stations for year 2026 only |
| `*_Bodega_*` | All Bodega stations (any state/year) |
| `CRNH0203-*-CA_*.txt` | All California stations, all years |

NOAA filename format: `CRNH0203-{YEAR}-{STATE}_{LOCATION}_{DISTANCE}_{DIRECTION}.txt`

Example: `CRNH0203-2026-PA_Avondale_2_N.txt`
- Year: 2026
- State: PA
- Location: Avondale
- Distance: 2 miles
- Direction: N (North)

#### 4. Combine Filters

Filters work together with OR logic (any match will be included):

```yaml
locations:
  states: ["CA"]              # All California stations
  stations: [3761]            # Plus Avondale, PA
  patterns: ["*_Bodega_*"]    # Plus all Bodega stations
```

This collects: All CA stations + Avondale PA (WBANNO 03761) + Any Bodega station from any state.

## Grafana Dashboard

The project includes a pre-configured Grafana dashboard designed for **gardening and landscaping decisions**.

### Access

- **URL**: http://localhost:3000
- **Default login**: admin/admin (configurable via `GRAFANA_ADMIN_USER` and `GRAFANA_ADMIN_PASSWORD`)
- **Dashboard location**: Gardening folder → "Gardening Weather Dashboard"

### Dashboard Features

| Section | Panels |
|---------|--------|
| **Current Conditions** | Air temp, Soil temp (10cm), Soil moisture, Humidity, 24h precipitation, Frost alert |
| **Soil Conditions** | Temperature & moisture at 5 depths (5-100cm) with threshold indicators |
| **Temperature & Growing** | Air temperature with frost zone, Growing Degree Days (cumulative, base 50°F) |
| **Precipitation & Disease Risk** | Rainfall chart, Humidity with disease risk thresholds |
| **Decision Support** | Activity recommendations table, Daily summary with GDD |

### Key Gardening Thresholds

| Metric | Threshold | Meaning |
|--------|-----------|---------|
| Soil temp 10cm | >50°F | Safe for cool-season crops |
| Soil temp 10cm | >59°F | Safe for warm-season crops |
| Soil moisture | <10% | Irrigation needed |
| Soil moisture | >40% | Saturated - avoid fertilizer |
| Air temp | <32°F | Frost warning |
| Humidity | >80% | Disease risk increased |

### Importing to External Grafana

To use the dashboard in your own Grafana instance:

1. Copy `grafana/dashboards/gardening-weather.json`
2. In Grafana: Dashboards → Import → Upload JSON file
3. Configure a PostgreSQL datasource pointing to your USCRN database

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

### Before Creating a Pull Request (REQUIRED)

**Always run the pre-PR checklist before creating a PR:**

```bash
chmod +x scripts/pre-pr-check.sh
./scripts/pre-pr-check.sh
```

This ensures:
- ✅ Code is properly formatted (`cargo fmt`)
- ✅ No linter warnings (`cargo clippy`)
- ✅ Tests pass locally
- ✅ Build succeeds

**Rule**: Do NOT create a PR until all pre-PR checks pass!

---

### Local Development

```bash
# Run tests
cargo test

# Build debug
cargo build

# Build release
cargo build --release

# Run locally (requires PostgreSQL)
cargo run

# Check formatting
cargo fmt --check

# Format code
cargo fmt

# Run clippy
cargo clippy

# Run with custom config
RUST_LOG=debug cargo run
```

### Docker Development Workflow

```bash
# 1. Make code changes
vim src/scheduler.rs

# 2. Rebuild and restart
docker-compose up --build

# 3. Watch logs
docker-compose logs -f app

# 4. Test database changes
docker-compose exec db psql -U uscrn -d uscrn
SELECT COUNT(*) FROM observations;

# 5. Backup database before experiments
docker-compose exec db pg_dump -U uscrn uscrn > backup.sql

# 6. Restore if needed
docker-compose exec -T db psql -U uscrn uscrn < backup.sql
```

### Production Deployment

1. **Build optimized image**:
   ```bash
   docker build -t uscrn-ingest:v1.0.0 .
   ```

2. **Run security scan**:
   ```bash
   docker scan uscrn-ingest:v1.0.0
   ```

3. **Deploy to production**:
   ```bash
   # Using Docker
   docker run -d \
     --name uscrn-ingest \
     --restart unless-stopped \
     --env-file .env.production \
     -v $(pwd)/config:/app/config:ro \
     uscrn-ingest:v1.0.0

   # Or using Docker Compose
   docker-compose -f docker-compose.prod.yml up -d
   ```

4. **Monitor logs**:
   ```bash
   docker logs -f uscrn-ingest
   ```

## License

MIT
