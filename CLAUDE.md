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

## Commands

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
```

## Data Flow

1. Scheduler triggers at configured interval
2. Fetcher lists available files from NOAA
3. Filter by configured locations (states/stations/patterns)
4. Skip already-processed files
5. Download and parse file content
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

### Docker Compose Only
- `POSTGRES_USER` — Database user (for docker-compose)
- `POSTGRES_PASSWORD` — Database password (for docker-compose)
- `POSTGRES_DB` — Database name (for docker-compose)

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

## NOAA Data Source

- URL: https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/
- Format: Space-separated fixed-width ASCII
- Update frequency: Hourly
- Missing data: -9999.0
