use crate::db::models::{InsertResult, NewObservation, NewProcessedFile, NewStation, ProcessedFile};
use crate::error::Result;
use sqlx::PgPool;
use tracing::{debug, info};

pub struct Repository {
    pool: PgPool,
}

impl Repository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations...");
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        info!("Database migrations completed");
        Ok(())
    }

    pub async fn is_file_processed(&self, file_name: &str) -> Result<bool> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM processed_files WHERE file_name = $1",
        )
        .bind(file_name)
        .fetch_one(&self.pool)
        .await?;

        Ok(result > 0)
    }

    pub async fn get_processed_files_for_year(&self, year: i32) -> Result<Vec<String>> {
        let file_names = sqlx::query_scalar::<_, String>(
            "SELECT file_name FROM processed_files WHERE year = $1",
        )
        .bind(year)
        .fetch_all(&self.pool)
        .await?;

        Ok(file_names)
    }

    pub async fn mark_file_processed(&self, file: NewProcessedFile) -> Result<i32> {
        let id = sqlx::query_scalar::<_, i32>(
            r#"
            INSERT INTO processed_files
                (file_name, file_url, year, state, station_name, last_modified,
                 rows_processed, file_hash, observations_inserted, observations_updated,
                 parse_failures, processing_status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (file_name) DO UPDATE SET
                rows_processed = EXCLUDED.rows_processed,
                observations_inserted = EXCLUDED.observations_inserted,
                observations_updated = EXCLUDED.observations_updated,
                parse_failures = EXCLUDED.parse_failures,
                processing_status = EXCLUDED.processing_status,
                processed_at = NOW(),
                file_hash = EXCLUDED.file_hash
            RETURNING id
            "#,
        )
        .bind(&file.file_name)
        .bind(&file.file_url)
        .bind(file.year)
        .bind(&file.state)
        .bind(&file.station_name)
        .bind(file.last_modified)
        .bind(file.rows_processed)
        .bind(&file.file_hash)
        .bind(file.observations_inserted)
        .bind(file.observations_updated)
        .bind(file.parse_failures)
        .bind(&file.processing_status)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn get_processed_file(&self, file_name: &str) -> Result<Option<ProcessedFile>> {
        let result = sqlx::query_as::<_, ProcessedFile>(
            "SELECT * FROM processed_files WHERE file_name = $1",
        )
        .bind(file_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result)
    }

    pub async fn upsert_station(&self, station: NewStation) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO stations (wbanno, name, state, latitude, longitude)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (wbanno) DO UPDATE SET
                name = COALESCE(EXCLUDED.name, stations.name),
                latitude = COALESCE(EXCLUDED.latitude, stations.latitude),
                longitude = COALESCE(EXCLUDED.longitude, stations.longitude)
            "#,
        )
        .bind(station.wbanno)
        .bind(&station.name)
        .bind(&station.state)
        .bind(station.latitude)
        .bind(station.longitude)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn insert_observations(
        &self,
        observations: &[NewObservation],
        source_file_id: i32,
    ) -> Result<InsertResult> {
        if observations.is_empty() {
            return Ok(InsertResult {
                inserted: 0,
                updated: 0,
                total_rows_affected: 0,
            });
        }

        // Count existing observations before we start
        // This helps us estimate inserts vs updates
        let existing_count = self.count_existing_observations(observations).await?;

        let mut total_rows_affected = 0;
        let mut tx = self.pool.begin().await?;

        // Process in batches of 1000 to avoid query size limits
        const BATCH_SIZE: usize = 1000;

        for (batch_idx, chunk) in observations.chunks(BATCH_SIZE).enumerate() {
            debug!(
                "Inserting batch {}/{} ({} observations)",
                batch_idx + 1,
                (observations.len() + BATCH_SIZE - 1) / BATCH_SIZE,
                chunk.len()
            );

            let mut query_builder = sqlx::QueryBuilder::new(
                "INSERT INTO observations (
                    wbanno, utc_datetime, lst_datetime, crx_version,
                    t_calc, t_hr_avg, t_max, t_min,
                    p_calc,
                    solarad, solarad_flag, solarad_max, solarad_max_flag, solarad_min, solarad_min_flag,
                    sur_temp_type, sur_temp, sur_temp_flag, sur_temp_max, sur_temp_max_flag, sur_temp_min, sur_temp_min_flag,
                    rh_hr_avg, rh_hr_avg_flag,
                    soil_moisture_5, soil_moisture_10, soil_moisture_20, soil_moisture_50, soil_moisture_100,
                    soil_temp_5, soil_temp_10, soil_temp_20, soil_temp_50, soil_temp_100,
                    source_file_id
                ) "
            );

            query_builder.push_values(chunk, |mut b, obs| {
                b.push_bind(obs.wbanno)
                    .push_bind(obs.utc_datetime)
                    .push_bind(obs.lst_datetime)
                    .push_bind(&obs.crx_version)
                    .push_bind(obs.t_calc)
                    .push_bind(obs.t_hr_avg)
                    .push_bind(obs.t_max)
                    .push_bind(obs.t_min)
                    .push_bind(obs.p_calc)
                    .push_bind(obs.solarad)
                    .push_bind(obs.solarad_flag)
                    .push_bind(obs.solarad_max)
                    .push_bind(obs.solarad_max_flag)
                    .push_bind(obs.solarad_min)
                    .push_bind(obs.solarad_min_flag)
                    .push_bind(&obs.sur_temp_type)
                    .push_bind(obs.sur_temp)
                    .push_bind(obs.sur_temp_flag)
                    .push_bind(obs.sur_temp_max)
                    .push_bind(obs.sur_temp_max_flag)
                    .push_bind(obs.sur_temp_min)
                    .push_bind(obs.sur_temp_min_flag)
                    .push_bind(obs.rh_hr_avg)
                    .push_bind(obs.rh_hr_avg_flag)
                    .push_bind(obs.soil_moisture_5)
                    .push_bind(obs.soil_moisture_10)
                    .push_bind(obs.soil_moisture_20)
                    .push_bind(obs.soil_moisture_50)
                    .push_bind(obs.soil_moisture_100)
                    .push_bind(obs.soil_temp_5)
                    .push_bind(obs.soil_temp_10)
                    .push_bind(obs.soil_temp_20)
                    .push_bind(obs.soil_temp_50)
                    .push_bind(obs.soil_temp_100)
                    .push_bind(source_file_id);
            });

            query_builder.push(
                " ON CONFLICT (wbanno, utc_datetime) DO UPDATE SET \
                lst_datetime = EXCLUDED.lst_datetime, \
                crx_version = EXCLUDED.crx_version, \
                t_calc = EXCLUDED.t_calc, \
                t_hr_avg = EXCLUDED.t_hr_avg, \
                t_max = EXCLUDED.t_max, \
                t_min = EXCLUDED.t_min, \
                p_calc = EXCLUDED.p_calc, \
                solarad = EXCLUDED.solarad, \
                solarad_flag = EXCLUDED.solarad_flag, \
                solarad_max = EXCLUDED.solarad_max, \
                solarad_max_flag = EXCLUDED.solarad_max_flag, \
                solarad_min = EXCLUDED.solarad_min, \
                solarad_min_flag = EXCLUDED.solarad_min_flag, \
                sur_temp_type = EXCLUDED.sur_temp_type, \
                sur_temp = EXCLUDED.sur_temp, \
                sur_temp_flag = EXCLUDED.sur_temp_flag, \
                sur_temp_max = EXCLUDED.sur_temp_max, \
                sur_temp_max_flag = EXCLUDED.sur_temp_max_flag, \
                sur_temp_min = EXCLUDED.sur_temp_min, \
                sur_temp_min_flag = EXCLUDED.sur_temp_min_flag, \
                rh_hr_avg = EXCLUDED.rh_hr_avg, \
                rh_hr_avg_flag = EXCLUDED.rh_hr_avg_flag, \
                soil_moisture_5 = EXCLUDED.soil_moisture_5, \
                soil_moisture_10 = EXCLUDED.soil_moisture_10, \
                soil_moisture_20 = EXCLUDED.soil_moisture_20, \
                soil_moisture_50 = EXCLUDED.soil_moisture_50, \
                soil_moisture_100 = EXCLUDED.soil_moisture_100, \
                soil_temp_5 = EXCLUDED.soil_temp_5, \
                soil_temp_10 = EXCLUDED.soil_temp_10, \
                soil_temp_20 = EXCLUDED.soil_temp_20, \
                soil_temp_50 = EXCLUDED.soil_temp_50, \
                soil_temp_100 = EXCLUDED.soil_temp_100, \
                source_file_id = EXCLUDED.source_file_id"
            );

            let result = query_builder.build().execute(&mut *tx).await?;

            total_rows_affected += result.rows_affected() as usize;
        }

        tx.commit().await?;

        // Estimate inserts vs updates
        // If rows_affected > existing_count, the difference is new inserts
        // Otherwise, all were updates
        let inserted = total_rows_affected.saturating_sub(existing_count);
        let updated = existing_count.min(total_rows_affected);

        Ok(InsertResult {
            inserted,
            updated,
            total_rows_affected,
        })
    }

    async fn count_existing_observations(&self, observations: &[NewObservation]) -> Result<usize> {
        if observations.is_empty() {
            return Ok(0);
        }

        // Build a query to count matching (wbanno, utc_datetime) pairs
        // Using a VALUES clause for efficiency
        let mut query_builder = sqlx::QueryBuilder::new(
            "SELECT COUNT(*) FROM observations WHERE (wbanno, utc_datetime) IN ("
        );

        let mut first = true;
        for obs in observations {
            if !first {
                query_builder.push(", ");
            }
            query_builder.push("(");
            query_builder.push_bind(obs.wbanno);
            query_builder.push(", ");
            query_builder.push_bind(obs.utc_datetime);
            query_builder.push(")");
            first = false;
        }

        query_builder.push(")");

        let count: i64 = query_builder
            .build_query_scalar()
            .fetch_one(&self.pool)
            .await?;

        Ok(count as usize)
    }
}
