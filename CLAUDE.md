# USCRN Data Ingestion Service

## Overview

Rust application that periodically fetches hourly climate data from NOAA's US Climate Reference Network, tracks processed files, filters by configured locations, and stores data in PostgreSQL.

## Tech Stack

- Language: Rust 1.84+
- Async Runtime: Tokio
- HTTP Client: Reqwest
- Database: PostgreSQL with SQLx
- Config: YAML with environment variable substitution
- Containerization: Docker with multi-stage builds

## Commands

- `cargo build` — Build the application
- `cargo build --release` — Build optimized release binary
- `cargo test` — Run tests
- `cargo run` — Run locally (requires PostgreSQL)
- `docker-compose up --build` — Build and run with Docker
- `docker-compose down -v` — Stop and remove volumes

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

## NOAA Data Source

- URL: https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/
- Format: Space-separated fixed-width ASCII
- Update frequency: Hourly
- Missing data: -9999.0
