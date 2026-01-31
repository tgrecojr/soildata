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

- `DATABASE_URL` — PostgreSQL connection string
- `RUST_LOG` — Logging level (default: info,uscrn_ingest=debug)
- `POSTGRES_USER` — Database user (for docker-compose)
- `POSTGRES_PASSWORD` — Database password (for docker-compose)
- `POSTGRES_DB` — Database name (for docker-compose)

## Configuration

Edit `config/config.yaml` to customize:

- `scheduler.interval_minutes` — Polling frequency
- `source.years_to_fetch` — "current", "all", or specific years
- `locations.states` — Filter by 2-letter state codes
- `locations.stations` — Filter by WBANNO station IDs
- `locations.patterns` — Filter by glob patterns

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
