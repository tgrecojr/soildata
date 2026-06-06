//! Bronze layer raw capture.
//!
//! Writes the *unparsed, unmodified* bytes of a source response to a shared
//! "bronze" layer on local disk, alongside a `.meta.json` sidecar of provenance.
//! This preserves raw data once, immutably, so it can be reprocessed later
//! without re-fetching from the source.
//!
//! Capture is a best-effort **side effect** layered next to the existing
//! processing: it never alters parsing/transformation and never blocks the
//! processor. A failed bronze write is logged and swallowed.
//!
//! Disabled by default: if `BRONZE_ROOT` is unset or empty, every capture is a
//! complete noop with no side effects.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

const PROCESSOR: &str = "uscrn-ingest";
const PROCESSOR_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: &str = "v1";

/// Monotonic process-local counter, mixed into the `short_id` so captures
/// within the same millisecond never collide on a filename.
static SHORT_ID_SEQ: AtomicU32 = AtomicU32::new(0);

/// Provenance the raw bytes alone don't carry. Recorded in the `.meta.json`
/// sidecar. Must never contain secrets, tokens, or auth headers.
#[derive(Debug, Clone)]
pub struct CaptureMeta {
    /// Enough to understand or replay the fetch. No secrets.
    pub request_url: String,
    /// Query/body params understood by the fetch. No secrets. JSON object.
    pub request_params: serde_json::Value,
    /// Real HTTP status, so the silver layer can treat non-2xx as diagnostic.
    pub http_status: u16,
    /// `Content-Type` value, e.g. `text/plain`.
    pub content_type: Option<String>,
    /// Declared character encoding for text payloads, e.g. `utf-8`.
    pub charset: Option<String>,
    /// How the payload *arrived* over the wire (`gzip`, `identity`, ...).
    pub content_encoding: String,
    /// How the bytes are *actually stored on disk* (`identity` if decompressed).
    pub stored_encoding: String,
    /// Extension of the stored bytes (reflects stored form, not arrival).
    pub ext: String,
    /// Field paths removed before writing (normally empty).
    pub redacted_fields: Vec<String>,
}

/// Bronze capture sink. Construct once via [`Bronze::from_env`] and share it
/// (e.g. behind an `Arc`) so the skip-if-identical cache survives across runs.
pub struct Bronze {
    /// `None` means capture is disabled (BRONZE_ROOT unset or empty).
    root: Option<PathBuf>,
    /// collection -> sha256 of the most recently written payload, so an
    /// unchanged re-fetch is not written again.
    last_hash: Mutex<HashMap<String, String>>,
}

impl Bronze {
    /// Read `BRONZE_ROOT` once. Unset or empty string => disabled.
    /// Logs a single line about the resolved state; never warns per-call.
    pub fn from_env() -> Self {
        let root = match std::env::var("BRONZE_ROOT") {
            Ok(v) if !v.trim().is_empty() => Some(PathBuf::from(v)),
            _ => None,
        };

        match &root {
            Some(p) => info!("bronze capture enabled: BRONZE_ROOT={}", p.display()),
            None => debug!("bronze capture disabled: BRONZE_ROOT not set"),
        }

        Self {
            root,
            last_hash: Mutex::new(HashMap::new()),
        }
    }

    /// Whether capture is enabled (BRONZE_ROOT was set to a non-empty value).
    pub fn is_enabled(&self) -> bool {
        self.root.is_some()
    }

    /// Capture `raw` bytes to bronze. Non-fatal and infallible from the
    /// caller's perspective: any failure is logged and swallowed.
    ///
    /// Returns the path of the written payload, or `None` when capture was a
    /// noop (disabled, skipped as identical, or failed).
    ///
    /// `raw` MUST be the unparsed body bytes (e.g. `response.bytes()`), never a
    /// decoded string.
    pub async fn capture(
        &self,
        source: &str,
        collection: &str,
        raw: &[u8],
        meta: &CaptureMeta,
    ) -> Option<PathBuf> {
        // Single early guard: do nothing at all when disabled.
        let root = self.root.as_ref()?;

        match self.try_capture(root, source, collection, raw, meta).await {
            Ok(written) => written,
            Err(e) => {
                // Best-effort: never raise out of the capture path.
                warn!("bronze capture failed for {}/{}: {}", source, collection, e);
                None
            }
        }
    }

    async fn try_capture(
        &self,
        root: &Path,
        source: &str,
        collection: &str,
        raw: &[u8],
        meta: &CaptureMeta,
    ) -> std::io::Result<Option<PathBuf>> {
        let sha256 = hex::encode(Sha256::digest(raw));

        // Skip-if-identical: don't rewrite a byte-for-byte unchanged re-fetch.
        if self.is_duplicate(collection, &sha256) {
            debug!(
                "bronze skip (unchanged) for {}/{}: sha256={}",
                source, collection, sha256
            );
            return Ok(None);
        }

        let now = Utc::now();
        let fetched_ms = now.timestamp_millis();
        let dt = now.format("%Y-%m-%d");
        let short_id = next_short_id(now.timestamp_subsec_nanos());

        let dir = root
            .join(source)
            .join(collection)
            .join(format!("dt={}", dt));
        let base = format!("{}_{}_{}", collection, fetched_ms, short_id);
        let payload_path = dir.join(format!("{}.{}", base, meta.ext));
        let sidecar_path = dir.join(format!("{}.meta.json", base));

        tokio::fs::create_dir_all(&dir).await?;

        let sidecar = serde_json::json!({
            "source": source,
            "collection": collection,
            "fetched_at": now.to_rfc3339_opts(SecondsFormat::Millis, true),
            "fetched_at_unix_ms": fetched_ms,
            "request_url": meta.request_url,
            "request_params": meta.request_params,
            "http_status": meta.http_status,
            "content_type": meta.content_type,
            "charset": meta.charset,
            "content_encoding": meta.content_encoding,
            "stored_encoding": meta.stored_encoding,
            "byte_size": raw.len(),
            "sha256": sha256,
            "redacted_fields": meta.redacted_fields,
            "processor": PROCESSOR,
            "processor_version": PROCESSOR_VERSION,
            "schema_version": SCHEMA_VERSION,
        });
        let sidecar_bytes = serde_json::to_vec_pretty(&sidecar)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Write payload first, then sidecar; both atomically (temp + rename).
        write_atomic(&payload_path, raw).await?;
        write_atomic(&sidecar_path, &sidecar_bytes).await?;

        // Only remember the hash once the write actually succeeded.
        self.remember(collection, sha256);

        debug!(
            "bronze captured {} ({} bytes)",
            payload_path.display(),
            raw.len()
        );
        Ok(Some(payload_path))
    }

    fn is_duplicate(&self, collection: &str, sha256: &str) -> bool {
        // Lock held only for the lookup, never across an await.
        self.last_hash
            .lock()
            .map(|cache| cache.get(collection).map(String::as_str) == Some(sha256))
            .unwrap_or(false)
    }

    fn remember(&self, collection: &str, sha256: String) {
        if let Ok(mut cache) = self.last_hash.lock() {
            cache.insert(collection.to_string(), sha256);
        }
    }
}

/// Derive a short, unique hex suffix. Mixes the sub-second nanos with a
/// process-local counter so two captures in the same millisecond differ.
fn next_short_id(subsec_nanos: u32) -> String {
    let seq = SHORT_ID_SEQ.fetch_add(1, Ordering::Relaxed);
    let mix = subsec_nanos.wrapping_mul(2_654_435_761).wrapping_add(seq) & 0x00FF_FFFF;
    format!("{:06x}", mix)
}

/// Atomic write: write to a temp file in the same directory, then rename into
/// place so a half-written file never appears under the final name. Maps
/// cleanly to S3 put-once semantics later.
async fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Sanitize a string into a bronze-safe path segment: lowercase, with only
/// `[a-z0-9_-]` retained and any other run collapsed to a single underscore.
pub fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = false;
    for c in input.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '-' {
            out.push(c);
            prev_underscore = false;
        } else if !prev_underscore {
            out.push('_');
            prev_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_meta() -> CaptureMeta {
        CaptureMeta {
            request_url: "https://www.ncei.noaa.gov/pub/data/uscrn/products/hourly02/2026/CRNH0203-2026-PA_Avondale_2_N.txt".to_string(),
            request_params: serde_json::json!({}),
            http_status: 200,
            content_type: Some("text/plain".to_string()),
            charset: Some("utf-8".to_string()),
            content_encoding: "identity".to_string(),
            stored_encoding: "identity".to_string(),
            ext: "txt".to_string(),
            redacted_fields: vec![],
        }
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("PA_Avondale_2_N"), "pa_avondale_2_n");
        assert_eq!(slugify("CA Bodega 6 WSW"), "ca_bodega_6_wsw");
        assert_eq!(slugify("__weird--name__"), "weird--name");
    }

    #[test]
    fn test_short_id_format() {
        let id = next_short_id(123_456);
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_disabled_is_complete_noop() {
        // No root => disabled. Capture must create nothing.
        let dir = tempfile::tempdir().unwrap();
        let probe = dir.path().join("should-not-exist");
        let bronze = Bronze {
            root: None,
            last_hash: Mutex::new(HashMap::new()),
        };

        let result = bronze
            .capture("uscrn", "pa_avondale_2_n", b"data", &test_meta())
            .await;

        assert!(result.is_none());
        assert!(!bronze.is_enabled());
        // Nothing at all should have been created under our probe dir.
        assert!(!probe.exists());
        // The temp dir should still be empty.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn test_capture_writes_payload_and_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let bronze = Bronze {
            root: Some(dir.path().to_path_buf()),
            last_hash: Mutex::new(HashMap::new()),
        };
        let raw = b"03761 20260308 0400 ...\n";

        let path = bronze
            .capture("uscrn", "pa_avondale_2_n", raw, &test_meta())
            .await
            .expect("should have written");

        // Payload bytes are stored byte-for-byte.
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert_eq!(path.extension().unwrap(), "txt");

        // Path follows the standard: uscrn/<collection>/dt=YYYY-MM-DD/<file>.
        let rel = path.strip_prefix(dir.path()).unwrap();
        let comps: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        assert_eq!(comps[0], "uscrn");
        assert_eq!(comps[1], "pa_avondale_2_n");
        assert!(comps[2].starts_with("dt="));
        assert!(comps[3].starts_with("pa_avondale_2_n_"));

        // Sidecar exists, is valid JSON, and the sha256 matches stored bytes.
        let sidecar_path = path.with_file_name(format!(
            "{}.meta.json",
            path.file_stem().unwrap().to_string_lossy()
        ));
        let sidecar: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&sidecar_path).unwrap()).unwrap();
        let expect_sha = hex::encode(Sha256::digest(raw));
        assert_eq!(sidecar["sha256"], expect_sha);
        assert_eq!(sidecar["byte_size"], raw.len());
        assert_eq!(sidecar["source"], "uscrn");
        assert_eq!(sidecar["collection"], "pa_avondale_2_n");
        assert_eq!(sidecar["stored_encoding"], "identity");
        assert_eq!(sidecar["schema_version"], "v1");
        // No temp files left behind.
        assert!(!path.with_extension("txt.tmp").exists());
    }

    #[tokio::test]
    async fn test_skip_if_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let bronze = Bronze {
            root: Some(dir.path().to_path_buf()),
            last_hash: Mutex::new(HashMap::new()),
        };
        let raw = b"identical payload";

        let first = bronze
            .capture("uscrn", "pa_avondale_2_n", raw, &test_meta())
            .await;
        assert!(first.is_some(), "first capture should write");

        let second = bronze
            .capture("uscrn", "pa_avondale_2_n", raw, &test_meta())
            .await;
        assert!(second.is_none(), "identical re-fetch should be skipped");

        // A changed payload writes a new, distinct file.
        let third = bronze
            .capture("uscrn", "pa_avondale_2_n", b"changed payload", &test_meta())
            .await;
        assert!(third.is_some(), "changed payload should write");
        assert_ne!(first.unwrap(), third.unwrap());
    }

    #[tokio::test]
    async fn test_capture_failure_is_non_fatal() {
        // Point the root at a path that cannot be a directory (a regular file),
        // so create_dir_all fails. capture() must swallow it and return None.
        let file = tempfile::NamedTempFile::new().unwrap();
        let bronze = Bronze {
            root: Some(file.path().to_path_buf()),
            last_hash: Mutex::new(HashMap::new()),
        };

        let result = bronze
            .capture("uscrn", "pa_avondale_2_n", b"data", &test_meta())
            .await;
        assert!(result.is_none());
    }
}
