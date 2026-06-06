# USCRN Data Ingestion Service

## Overview

Rust application that periodically fetches hourly climate data from NOAA's US Climate Reference Network, tracks processed files, filters by configured locations, and stores data in PostgreSQL.

## Tech Stack

- Language: Rust (latest stable)
- Async Runtime: Tokio
- HTTP Client: Reqwest
- Database: PostgreSQL with SQLx
- Config: YAML with environment variable substitution
- Containerization: Docker with multi-stage builds (cargo-chef)
- Build Tool: cargo-chef for optimized Docker layer caching
- Visualization: Grafana with gardening-focused dashboard

## Commands

### Before Creating a PR (REQUIRED)
**Always run this before creating a pull request:**
```bash
./scripts/pre-pr-check.sh
```

This runs:
1. ✅ Code formatting check (`cargo fmt`)
2. ✅ Linter (`cargo clippy`)
3. ✅ Local tests (no database needed)
4. ✅ Build verification

**Rule**: Do NOT create a PR until all pre-PR checks pass!

### Local Development
- `cargo build` — Build the application
- `cargo build --release` — Build optimized release binary
- `cargo test` — Run tests
- `cargo run` — Run locally (requires PostgreSQL)

### Docker
- `docker-compose up --build` — Build and run with Docker
- `docker-compose up -d` — Run in background
- `docker-compose logs -f` — Follow logs
- `docker-compose down` — Stop containers
- `docker-compose down -v` — Stop and remove volumes

### Docker Build (Standalone)
- `docker build -t uscrn-ingest .` — Build image with cargo-chef caching
- `docker build -f Dockerfile.simple -t uscrn-ingest .` — Simple build without caching

## Architecture

```
src/
├── main.rs           # Entry point, signal handling
├── lib.rs            # Module exports
├── config.rs         # YAML config loading
├── error.rs          # Error types
├── fetcher.rs        # NOAA HTTP client
├── parser.rs         # Fixed-width file parser
├── scheduler.rs      # Periodic job runner
└── db/
    ├── mod.rs
    ├── models.rs     # Database models
    └── repository.rs # Database operations

grafana/
├── dashboards/
│   └── gardening-weather.json  # Importable dashboard
└── provisioning/
    ├── dashboards/
    │   └── dashboard.yml       # Dashboard provider config
    └── datasources/
        └── postgres.yml        # PostgreSQL datasource config
```

## Data Flow

1. Scheduler triggers at configured interval
2. Fetcher lists available files from NOAA
3. Filter by configured locations (states/stations/patterns)
4. Skip already-processed files
5. Download file → **capture raw bytes to bronze (best-effort, before parsing)** → parse file content
6. Upsert station metadata
7. Insert observations with deduplication
8. Mark file as processed

## Environment Variables

### Required in config/config.yaml
- `DB_HOST` — Database host (e.g., localhost or postgres)
- `DB_PORT` — Database port (e.g., 5432)
- `DB_NAME` — Database name
- `DB_USER` — Database user
- `DB_PASSWORD` — Database password

### Optional Environment Variables
- `RUST_LOG` — Logging level (default: info,uscrn_ingest=debug)
- `BRONZE_ROOT` — Root directory for the bronze raw-capture layer (e.g. `/data/bronze`). **Disabled by default**: if unset or empty, bronze capture is a complete noop and the app behaves exactly as before. See [Bronze Layer](#bronze-layer-raw-capture).

### Docker Compose Only
- `POSTGRES_USER` — Database user (for docker-compose)
- `POSTGRES_PASSWORD` — Database password (for docker-compose)
- `POSTGRES_DB` — Database name (for docker-compose)
- `GRAFANA_ADMIN_USER` — Grafana admin username (default: admin)
- `GRAFANA_ADMIN_PASSWORD` — Grafana admin password (default: admin)

## Configuration

1. Copy the example configuration:
   ```bash
   cp config/config.yaml.example config/config.yaml
   ```

2. Edit `config/config.yaml` to customize:
   - `scheduler.interval_minutes` — Polling frequency (default: 60)
   - `source.years_to_fetch` — "current", "all", or specific years [2024, 2025]
   - `locations.states` — Filter by 2-letter state codes ["CA", "TX"]
   - `locations.stations` — Filter by WBANNO IDs [3761] (no leading zeros)
   - `locations.patterns` — Filter by glob patterns ["*PA_Avondale*"]

**Note**: `config/config.yaml` is gitignored. Only `config.yaml.example` is tracked.

### Location Filtering Examples

**By State:**
```yaml
locations:
  states: ["PA"]
  stations: []
  patterns: []
```

**By Station ID (WBANNO):**
```yaml
locations:
  states: []
  stations: [3761]  # Avondale, PA (use number without leading zero)
  patterns: []
```

**By Glob Pattern:**
```yaml
locations:
  states: []
  stations: []
  patterns: ["*PA_Avondale*"]  # All Avondale, PA files
```

**Combined (OR logic):**
```yaml
locations:
  states: ["CA"]
  stations: [3761]
  patterns: ["*_Bodega_*"]
```

## Bronze Layer (Raw Capture)

The app optionally captures the **exact raw bytes** of each downloaded NOAA file to a local "bronze" layer, so raw data is preserved once, immutably, and can be reprocessed later without re-fetching. This is a side-effect added alongside the existing parsing/DB logic — it never alters how data is parsed or stored.

### Enabling
Set `BRONZE_ROOT` to a directory (e.g. `/data/bronze`). Unset or empty = complete noop (default). Capture is opt-in per environment.

### What is captured
- **Captured (data):** the per-station hourly data files from `download_file` — the actual climate records.
- **Not captured (plumbing):** the NOAA directory-listing HTML used to discover years/files.
- No secrets, tokens, or auth are involved in this source, so nothing is ever redacted.

### How append-at-source maps to bronze
NOAA appends a new hourly row to each station-year file in place. The app always re-downloads the **entire current-year file**, so every poll is a complete snapshot of the file at that moment. Each snapshot is written as a **new immutable bronze object** (partitioned by fetch date, never overwriting). Overlapping rows across snapshots are a silver-layer concern; the existing DB upsert already dedupes at runtime. To curb redundancy, a byte-identical re-fetch is **skipped** (compared against the last capture's sha256, cached in-process).

### Layout
```
{BRONZE_ROOT}/uscrn/{station_slug}/dt={YYYY-MM-DD}/{station_slug}_{fetched_at_unix_ms}_{short_id}.txt
```
- `source` = `uscrn`; `collection` = per-station slug (e.g. `pa_avondale_2_n`).
- Each payload has a `.meta.json` sidecar (provenance: request URL, HTTP status, content-type/charset, `content_encoding` arrival vs `stored_encoding` on disk, `byte_size`, `sha256`, processor/version, `schema_version`).
- Stored form is plain text (`identity` encoding), so `ext` is `txt`.
- Writes are atomic (temp file + rename). Capture failures are logged and **non-fatal**.

### Implementation
- `src/bronze.rs` — `Bronze` sink (`from_env`, skip-if-identical cache, atomic write), `CaptureMeta`, `slugify`.
- `src/fetcher.rs` — `download_file` reads `response.bytes()`, captures to bronze, then decodes for the parser.
- `src/scheduler.rs` — owns the shared `Arc<Bronze>` so the dedup cache persists across runs.

### Deferred (not built yet)
- Error-response body capture (non-2xx with content) — NOAA failures here carry no useful body.
- At-rest compression and retention/cleanup — explicit non-goals for v1.
- S3 backend — `BRONZE_ROOT` is the single swappable base location, kept S3-migration-friendly.

## Database Schema

- `stations` — Station metadata (WBANNO, name, state, coordinates)
- `observations` — Hourly climate observations (temperature, precipitation, soil data)
- `processed_files` — Tracking of ingested files

## Docker Build Strategy

The project uses **cargo-chef** + **distroless** for optimized production builds:

### Build Stages
1. **chef** — Base image with cargo-chef installed
2. **planner** — Analyzes dependencies from Cargo.toml
3. **builder** — Builds dependencies (cached) + application with size optimizations
4. **runtime** — Google distroless image (~20MB, no shell/package manager)

### Size Optimizations
- Binary stripping (removes debug symbols)
- Link-Time Optimization (LTO = "thin")
- Size-optimized compilation (opt-level = "z")
- Single codegen unit for better optimization
- Distroless runtime (60-70% smaller than debian:slim)

### Security Benefits
- Minimal attack surface (~50% fewer CVEs than debian:slim)
- No shell or package manager in production container
- Runs as non-root (user ID 65532)
- Immutable infrastructure pattern

### Performance
- Caches dependencies separately from source code
- Only rebuilds deps when Cargo.toml changes
- Faster CI/CD pipelines (5-10x faster on cache hit)
- Final image: ~50-80MB (vs 200MB+ with debian:slim)

## Grafana Dashboard

The project includes a Grafana instance with a pre-configured gardening-focused dashboard.

### Access
- URL: http://localhost:3000
- Default credentials: admin/admin (configurable via environment variables)

### Dashboard Features
The "Gardening Weather Dashboard" is designed for landscaping and gardening decisions:

**Current Conditions** (stat panels):
- Air Temperature, Soil Temperature (10cm), Soil Moisture, Humidity
- 24h Precipitation total
- Frost Alert indicator

**Soil Conditions**:
- Soil temperature at 5 depths (5, 10, 20, 50, 100cm) with seeding thresholds
- Soil moisture at 5 depths with irrigation thresholds

**Temperature & Growing**:
- Air temperature with frost threshold visualization
- Growing Degree Days (GDD) cumulative chart (base 10°C)

**Precipitation & Disease Risk**:
- Precipitation bar chart
- Humidity with disease risk thresholds (>80%)

**Decision Support**:
- Activity recommendations table (seeding, fertilizing, irrigation, fungicide)
- Daily summary with temperature range and GDD

### Importing to External Grafana
To use the dashboard in your own Grafana instance:
1. Copy `grafana/dashboards/gardening-weather.json`
2. In Grafana: Dashboards → Import → Upload JSON file
3. Configure your PostgreSQL datasource to match the schema

### Key Gardening Thresholds
| Metric | Threshold | Meaning |
|--------|-----------|---------|
| Soil temp 10cm | >50°F | Safe for cool-season crops |
| Soil temp 10cm | >59°F | Safe for warm-season crops |
| Soil moisture | <0.10 | Irrigation needed |
| Soil moisture | >0.40 | Saturated - avoid fertilizer |
| Air temp | <32°F | Frost warning |
| Humidity | >80% | Disease risk increased |

## NOAA Data Source

- URL: https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/
- Format: Space-separated fixed-width ASCII
- Update frequency: Hourly
- Missing data: -9999.0
