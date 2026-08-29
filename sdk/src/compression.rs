use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression as GzipCompression;
use std::io::{Read, Write};
use thiserror::Error;
use tracing::debug;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("compression failed: {0}")]
    CompressFailed(String),
    #[error("decompression failed: {0}")]
    DecompressFailed(String),
    #[error("unsupported compression format: {0}")]
    UnsupportedFormat(String),
}

// ---------------------------------------------------------------------------
// Format specification
// ---------------------------------------------------------------------------

/// Supported compression formats for query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompressionFormat {
    /// No compression — passthrough.
    None,
    /// Gzip compression (via flate2).
    Gzip,
    /// Zstandard compression — high ratio, fast.
    Zstd,
}

impl CompressionFormat {
    /// Parse a format string (case-insensitive).
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "none" | "" => Some(Self::None),
            "gzip" | "gz" => Some(Self::Gzip),
            "zstd" | "zst" => Some(Self::Zstd),
            _ => None,
        }
    }

    /// Canonical name for headers / logging.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
        }
    }
}

impl std::fmt::Display for CompressionFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Compression ratio monitoring
// ---------------------------------------------------------------------------

/// Tracks compression statistics across a session.
#[derive(Debug, Clone, Default)]
pub struct CompressionStats {
    pub total_input_bytes: u64,
    pub total_output_bytes: u64,
    pub operations: u64,
}

impl CompressionStats {
    /// Ratio of output / input. A value < 1.0 means data shrank.
    pub fn ratio(&self) -> f64 {
        if self.total_input_bytes == 0 {
            return 1.0;
        }
        self.total_output_bytes as f64 / self.total_input_bytes as f64
    }

    /// Percentage of space saved (e.g. 42.5 means 42.5% smaller).
    pub fn space_saved_percent(&self) -> f64 {
        (1.0 - self.ratio()) * 100.0
    }

    /// Record a single compress / decompress operation.
    fn record(&mut self, input_len: usize, output_len: usize) {
        self.total_input_bytes += input_len as u64;
        self.total_output_bytes += output_len as u64;
        self.operations += 1;
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

// ---------------------------------------------------------------------------
// Core API
// ---------------------------------------------------------------------------

/// Compress `data` using the given format.
///
/// Returns the compressed bytes and updates `stats` if provided.
pub fn compress(
    data: &[u8],
    format: CompressionFormat,
    stats: Option<&mut CompressionStats>,
) -> Result<Vec<u8>, CompressionError> {
    let result = match format {
        CompressionFormat::None => data.to_vec(),
        CompressionFormat::Gzip => compress_gzip(data)?,
        CompressionFormat::Zstd => compress_zstd(data)?,
    };

    debug!(
        format = %format,
        input_len = data.len(),
        output_len = result.len(),
        "query result compressed"
    );

    if let Some(s) = stats {
        s.record(data.len(), result.len());
    }

    Ok(result)
}

/// Decompress `data` that was compressed with the given format.
///
/// Returns the decompressed bytes and updates `stats` if provided.
pub fn decompress(
    data: &[u8],
    format: CompressionFormat,
    stats: Option<&mut CompressionStats>,
) -> Result<Vec<u8>, CompressionError> {
    let result = match format {
        CompressionFormat::None => data.to_vec(),
        CompressionFormat::Gzip => decompress_gzip(data)?,
        CompressionFormat::Zstd => decompress_zstd(data)?,
    };

    debug!(
        format = %format,
        input_len = data.len(),
        output_len = result.len(),
        "query result decompressed"
    );

    if let Some(s) = stats {
        s.record(data.len(), result.len());
    }

    Ok(result)
}

/// Convenience: compress then immediately decompress to verify round-trip.
/// Useful in tests and benchmarks.
pub fn roundtrip(
    data: &[u8],
    format: CompressionFormat,
) -> Result<(Vec<u8>, CompressionStats), CompressionError> {
    let mut stats = CompressionStats::default();
    let compressed = compress(data, format, Some(&mut stats))?;
    let decompressed = decompress(&compressed, format, Some(&mut stats))?;
    Ok((decompressed, stats))
}

// ---------------------------------------------------------------------------
// Format-specific implementations
// ---------------------------------------------------------------------------

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    let mut encoder = GzEncoder::new(Vec::new(), GzipCompression::fast());
    encoder
        .write_all(data)
        .map_err(|e| CompressionError::CompressFailed(e.to_string()))?;
    encoder
        .finish()
        .map_err(|e| CompressionError::CompressFailed(e.to_string()))
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| CompressionError::DecompressFailed(e.to_string()))?;
    Ok(out)
}

fn compress_zstd(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    zstd::encode_all(data, 3)
        .map_err(|e| CompressionError::CompressFailed(e.to_string()))
}

fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    zstd::decode_all(data)
        .map_err(|e| CompressionError::DecompressFailed(e.to_string()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Format parsing --

    #[test]
    fn test_format_from_str() {
        assert_eq!(CompressionFormat::from_str_opt("gzip"), Some(CompressionFormat::Gzip));
        assert_eq!(CompressionFormat::from_str_opt("gz"), Some(CompressionFormat::Gzip));
        assert_eq!(CompressionFormat::from_str_opt("ZSTD"), Some(CompressionFormat::Zstd));
        assert_eq!(CompressionFormat::from_str_opt("none"), Some(CompressionFormat::None));
        assert_eq!(CompressionFormat::from_str_opt(""), Some(CompressionFormat::None));
        assert_eq!(CompressionFormat::from_str_opt("brotli"), None);
    }

    #[test]
    fn test_format_display() {
        assert_eq!(CompressionFormat::Gzip.to_string(), "gzip");
        assert_eq!(CompressionFormat::Zstd.to_string(), "zstd");
        assert_eq!(CompressionFormat::None.to_string(), "none");
    }

    // -- Round-trip correctness --

    #[test]
    fn test_roundtrip_none() {
        let data = b"hello world";
        let (result, _) = roundtrip(data, CompressionFormat::None).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_roundtrip_gzip() {
        let data = b"the quick brown fox jumps over the lazy dog. \
                      the quick brown fox jumps over the lazy dog. \
                      the quick brown fox jumps over the lazy dog.";
        let (result, _) = roundtrip(data, CompressionFormat::Gzip).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_roundtrip_zstd() {
        let data = b"the quick brown fox jumps over the lazy dog. \
                      the quick brown fox jumps over the lazy dog. \
                      the quick brown fox jumps over the lazy dog.";
        let (result, _) = roundtrip(data, CompressionFormat::Zstd).unwrap();
        assert_eq!(result, data);
    }

    // -- Compression effectiveness --

    #[test]
    fn test_gzip_compresses_repetitive_data() {
        let data = vec![b'A'; 10_000];
        let compressed = compress(&data, CompressionFormat::Gzip, None).unwrap();
        assert!(compressed.len() < data.len() / 10, "gzip should strongly compress repetitive data");
    }

    #[test]
    fn test_zstd_compresses_repetitive_data() {
        let data = vec![b'A'; 10_000];
        let compressed = compress(&data, CompressionFormat::Zstd, None).unwrap();
        assert!(compressed.len() < data.len() / 10, "zstd should strongly compress repetitive data");
    }

    // -- Stats tracking --

    #[test]
    fn test_stats_tracking() {
        let mut stats = CompressionStats::default();
        let data = b"hello world, this is a test of compression tracking";

        let compressed = compress(data, CompressionFormat::Gzip, Some(&mut stats)).unwrap();
        let _decompressed = decompress(&compressed, CompressionFormat::Gzip, Some(&mut stats)).unwrap();

        assert_eq!(stats.operations, 2);
        assert!(stats.total_input_bytes > 0);
        assert!(stats.total_output_bytes > 0);
    }

    #[test]
    fn test_stats_ratio_and_space_saved() {
        let mut stats = CompressionStats::default();
        let data = vec![b'X'; 10_000];

        let compressed = compress(&data, CompressionFormat::Gzip, Some(&mut stats)).unwrap();
        decompress(&compressed, CompressionFormat::Gzip, Some(&mut stats)).unwrap();

        assert!(stats.ratio() < 1.0, "ratio should be < 1 for compressible data");
        assert!(stats.space_saved_percent() > 0.0);
    }

    #[test]
    fn test_stats_reset() {
        let mut stats = CompressionStats::default();
        let _ = compress(b"test", CompressionFormat::Gzip, Some(&mut stats)).unwrap();
        assert!(stats.operations > 0);
        stats.reset();
        assert_eq!(stats.operations, 0);
        assert_eq!(stats.total_input_bytes, 0);
    }

    #[test]
    fn test_stats_none_format_no_compression() {
        let mut stats = CompressionStats::default();
        let data = b"no compression here";
        let compressed = compress(data, CompressionFormat::None, Some(&mut stats)).unwrap();
        assert_eq!(compressed, data);
        assert_eq!(stats.total_input_bytes, data.len() as u64);
        assert_eq!(stats.total_output_bytes, data.len() as u64);
    }

    // -- Empty input edge case --

    #[test]
    fn test_compress_empty_gzip() {
        let compressed = compress(b"", CompressionFormat::Gzip, None).unwrap();
        let decompressed = decompress(&compressed, CompressionFormat::Gzip, None).unwrap();
        assert!(decompressed.is_empty());
    }

    #[test]
    fn test_compress_empty_zstd() {
        let compressed = compress(b"", CompressionFormat::Zstd, None).unwrap();
        let decompressed = decompress(&compressed, CompressionFormat::Zstd, None).unwrap();
        assert!(decompressed.is_empty());
    }
}
