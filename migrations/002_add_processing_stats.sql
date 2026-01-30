-- Add detailed processing statistics to processed_files table

ALTER TABLE processed_files
ADD COLUMN IF NOT EXISTS observations_inserted INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS observations_updated INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS parse_failures INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS processing_status VARCHAR(20) DEFAULT 'completed';

-- Add index for status queries
CREATE INDEX IF NOT EXISTS idx_processed_files_status ON processed_files(processing_status);

COMMENT ON COLUMN processed_files.rows_processed IS 'Total observations in file (excluding parse failures)';
COMMENT ON COLUMN processed_files.observations_inserted IS 'New observations inserted into database';
COMMENT ON COLUMN processed_files.observations_updated IS 'Existing observations updated';
COMMENT ON COLUMN processed_files.parse_failures IS 'Lines that failed to parse';
COMMENT ON COLUMN processed_files.processing_status IS 'Status: completed, failed, partial';
