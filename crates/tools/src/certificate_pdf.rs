use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};

/// Configuration for PDF generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfGenerationConfig {
    /// Maximum pages per chunk (default: 50)
    pub pages_per_chunk: usize,
    /// Timeout for entire PDF generation (default: 120s)
    pub timeout_seconds: u64,
    /// Enable streaming mode for large documents
    pub streaming_enabled: bool,
    /// Maximum document size before switching to streaming (in MB)
    pub streaming_threshold_mb: usize,
}

impl Default for PdfGenerationConfig {
    fn default() -> Self {
        Self {
            pages_per_chunk: 50,
            timeout_seconds: 120,
            streaming_enabled: true,
            streaming_threshold_mb: 10,
        }
    }
}

/// Represents a chunk of certificate data for PDF generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateChunk {
    pub chunk_id: usize,
    pub total_chunks: usize,
    pub certificates: Vec<CertificateData>,
    pub page_count: usize,
}

/// Certificate data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateData {
    pub id: String,
    pub recipient_name: String,
    pub campaign_name: String,
    pub amount: i128,
    pub timestamp: u64,
    pub attachments: Vec<String>,
}

/// PDF generation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdfGenerationResult {
    pub success: bool,
    pub file_path: Option<String>,
    pub total_pages: usize,
    pub total_chunks: usize,
    pub generation_time_ms: u64,
    pub error: Option<String>,
    pub is_streamed: bool,
}

/// Handles chunked PDF generation for large certificate sets
pub struct CertificatePdfGenerator {
    config: PdfGenerationConfig,
}

impl CertificatePdfGenerator {
    /// Create a new PDF generator with default configuration
    pub fn new() -> Self {
        Self {
            config: PdfGenerationConfig::default(),
        }
    }

    /// Create a new PDF generator with custom configuration
    pub fn with_config(config: PdfGenerationConfig) -> Self {
        Self { config }
    }

    /// Generate PDF from certificates, automatically choosing chunked or streaming mode
    pub fn generate_pdf(
        &self,
        certificates: &[CertificateData],
        output_path: &str,
    ) -> Result<PdfGenerationResult> {
        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        // Estimate page count (roughly 1 page per certificate + attachments)
        let estimated_pages = self.estimate_page_count(certificates);

        // Determine if we need chunked processing
        if estimated_pages > self.config.pages_per_chunk {
            self.generate_chunked_pdf(certificates, output_path, start_time, timeout)
        } else {
            self.generate_single_pdf(certificates, output_path, start_time, timeout)
        }
    }

    /// Generate PDF using chunked processing for large documents
    fn generate_chunked_pdf(
        &self,
        certificates: &[CertificateData],
        output_path: &str,
        start_time: Instant,
        timeout: Duration,
    ) -> Result<PdfGenerationResult> {
        let chunks = self.create_chunks(certificates);
        let total_chunks = chunks.len();

        println!("📄 Generating PDF in {} chunks...", total_chunks);

        let mut total_pages = 0;
        let mut pdf_buffer = Vec::new();

        // Initialize PDF document
        self.initialize_pdf(&mut pdf_buffer)?;

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            // Check timeout
            if start_time.elapsed() > timeout {
                return Ok(PdfGenerationResult {
                    success: false,
                    file_path: None,
                    total_pages,
                    total_chunks,
                    generation_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Timeout after {} seconds", self.config.timeout_seconds)),
                    is_streamed: true,
                });
            }

            println!("  Processing chunk {}/{} ({} certificates)...", 
                     chunk_idx + 1, total_chunks, chunk.certificates.len());

            // Process chunk
            let chunk_pages = self.process_chunk(chunk, &mut pdf_buffer)?;
            total_pages += chunk_pages;

            // Log progress
            let progress = ((chunk_idx + 1) as f64 / total_chunks as f64) * 100.0;
            println!("  Progress: {:.1}% ({} pages generated)", progress, total_pages);
        }

        // Finalize PDF
        self.finalize_pdf(&mut pdf_buffer)?;

        // Write to file
        std::fs::write(output_path, &pdf_buffer)
            .context(format!("Failed to write PDF to {}", output_path))?;

        let generation_time = start_time.elapsed();

        println!("✅ PDF generation complete: {} pages in {:.2}s", 
                 total_pages, generation_time.as_secs_f64());

        Ok(PdfGenerationResult {
            success: true,
            file_path: Some(output_path.to_string()),
            total_pages,
            total_chunks,
            generation_time_ms: generation_time.as_millis() as u64,
            error: None,
            is_streamed: true,
        })
    }

    /// Generate PDF for small document sets (single pass)
    fn generate_single_pdf(
        &self,
        certificates: &[CertificateData],
        output_path: &str,
        start_time: Instant,
        timeout: Duration,
    ) -> Result<PdfGenerationResult> {
        if start_time.elapsed() > timeout {
            return Ok(PdfGenerationResult {
                success: false,
                file_path: None,
                total_pages: 0,
                total_chunks: 0,
                generation_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Timeout occurred".to_string()),
                is_streamed: false,
            });
        }

        let mut pdf_buffer = Vec::new();
        self.initialize_pdf(&mut pdf_buffer)?;

        let total_pages = self.render_certificates(certificates, &mut pdf_buffer)?;

        self.finalize_pdf(&mut pdf_buffer)?;

        std::fs::write(output_path, &pdf_buffer)
            .context(format!("Failed to write PDF to {}", output_path))?;

        let generation_time = start_time.elapsed();

        Ok(PdfGenerationResult {
            success: true,
            file_path: Some(output_path.to_string()),
            total_pages,
            total_chunks: 1,
            generation_time_ms: generation_time.as_millis() as u64,
            error: None,
            is_streamed: false,
        })
    }

    /// Split certificates into chunks
    fn create_chunks(&self, certificates: &[CertificateData]) -> Vec<CertificateChunk> {
        let total_certs = certificates.len();
        let chunks_per_set = (total_certs + self.config.pages_per_chunk - 1) / self.config.pages_per_chunk;
        let mut chunks = Vec::new();

        for chunk_idx in 0..chunks_per_set {
            let start = chunk_idx * self.config.pages_per_chunk;
            let end = (start + self.config.pages_per_chunk).min(total_certs);
            
            let chunk_certs = certificates[start..end].to_vec();
            let page_count = self.estimate_page_count(&chunk_certs);

            chunks.push(CertificateChunk {
                chunk_id: chunk_idx,
                total_chunks: chunks_per_set,
                certificates: chunk_certs,
                page_count,
            });
        }

        chunks
    }

    /// Estimate page count for certificates
    fn estimate_page_count(&self, certificates: &[CertificateData]) -> usize {
        // Base: 1 page per certificate
        // Additional pages for attachments (roughly 1 page per 5 attachments)
        certificates.iter().map(|cert| {
            1 + (cert.attachments.len() + 4) / 5
        }).sum()
    }

    /// Initialize PDF document structure
    fn initialize_pdf(&self, buffer: &mut Vec<u8>) -> Result<()> {
        // PDF header and basic structure
        buffer.extend_from_slice(b"%PDF-1.4\n");
        buffer.extend_from_slice(b"%% Generated by StellarAid Certificate Generator\n");
        // In production, use a proper PDF library like lopdf or printpdf
        Ok(())
    }

    /// Process a chunk of certificates
    fn process_chunk(
        &self,
        chunk: &CertificateChunk,
        buffer: &mut Vec<u8>,
    ) -> Result<usize> {
        let pages = self.render_certificates(&chunk.certificates, buffer)?;
        Ok(pages)
    }

    /// Render certificates to PDF buffer
    fn render_certificates(
        &self,
        certificates: &[CertificateData],
        buffer: &mut Vec<u8>,
    ) -> Result<usize> {
        let mut page_count = 0;

        for cert in certificates {
            // Render certificate header
            let cert_header = format!(
                "\n% Certificate: {}\n",
                cert.id
            );
            buffer.extend_from_slice(cert_header.as_bytes());

            // Render certificate content
            let content = format!(
                "Recipient: {}\nCampaign: {}\nAmount: {}\nTimestamp: {}\n",
                cert.recipient_name,
                cert.campaign_name,
                cert.amount,
                cert.timestamp
            );
            buffer.extend_from_slice(content.as_bytes());

            // Render attachments if any
            if !cert.attachments.is_empty() {
                let att_header = format!("Attachments ({}):\n", cert.attachments.len());
                buffer.extend_from_slice(att_header.as_bytes());
                
                for (idx, attachment) in cert.attachments.iter().enumerate() {
                    let att_line = format!("  {}. {}\n", idx + 1, attachment);
                    buffer.extend_from_slice(att_line.as_bytes());
                }
            }

            buffer.extend_from_slice(b"---\n");
            page_count += 1 + (cert.attachments.len() + 4) / 5;
        }

        Ok(page_count)
    }

    /// Finalize PDF document
    fn finalize_pdf(&self, buffer: &mut Vec<u8>) -> Result<()> {
        buffer.extend_from_slice(b"\n%%EOF\n");
        Ok(())
    }

    /// Generate PDF with streaming mode for very large datasets
    pub fn generate_streaming_pdf(
        &self,
        certificates: &[CertificateData],
        output_path: &str,
    ) -> Result<PdfGenerationResult> {
        use std::io::Write;
        use std::fs::File;

        let start_time = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds);

        let mut file = File::create(output_path)
            .context(format!("Failed to create file: {}", output_path))?;

        // Write PDF header
        writeln!(file, "%PDF-1.4")?;
        writeln!(file, "%% Generated by StellarAid Certificate Generator (Streaming Mode)")?;

        let total_certs = certificates.len();
        let mut page_count = 0;
        let mut processed = 0;

        // Process in streaming chunks
        for chunk in certificates.chunks(self.config.pages_per_chunk) {
            if start_time.elapsed() > timeout {
                return Ok(PdfGenerationResult {
                    success: false,
                    file_path: Some(output_path.to_string()),
                    total_pages: page_count,
                    total_chunks: (total_certs + self.config.pages_per_chunk - 1) / self.config.pages_per_chunk,
                    generation_time_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(format!("Timeout after {} seconds", self.config.timeout_seconds)),
                    is_streamed: true,
                });
            }

            for cert in chunk {
                writeln!(file, "\n% Certificate: {}", cert.id)?;
                writeln!(file, "Recipient: {}", cert.recipient_name)?;
                writeln!(file, "Campaign: {}", cert.campaign_name)?;
                writeln!(file, "Amount: {}", cert.amount)?;
                writeln!(file, "Timestamp: {}", cert.timestamp)?;
                
                if !cert.attachments.is_empty() {
                    writeln!(file, "Attachments ({}):", cert.attachments.len())?;
                    for (idx, attachment) in cert.attachments.iter().enumerate() {
                        writeln!(file, "  {}. {}", idx + 1, attachment)?;
                    }
                }
                
                writeln!(file, "---")?;
                page_count += 1 + (cert.attachments.len() + 4) / 5;
                processed += 1;
            }

            // Flush periodically to manage memory
            if processed % 100 == 0 {
                file.flush()?;
                println!("  Processed {} / {} certificates...", processed, total_certs);
            }
        }

        writeln!(file, "\n%%EOF")?;
        file.flush()?;

        let generation_time = start_time.elapsed();

        Ok(PdfGenerationResult {
            success: true,
            file_path: Some(output_path.to_string()),
            total_pages: page_count,
            total_chunks: (total_certs + self.config.pages_per_chunk - 1) / self.config.pages_per_chunk,
            generation_time_ms: generation_time.as_millis() as u64,
            error: None,
            is_streamed: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_certificates(count: usize) -> Vec<CertificateData> {
        (0..count).map(|i| CertificateData {
            id: format!("cert_{:04}", i),
            recipient_name: format!("Recipient {}", i),
            campaign_name: format!("Campaign {}", i % 10),
            amount: 1000 + (i as i128),
            timestamp: 1234567890 + (i as u64),
            attachments: if i % 5 == 0 {
                vec!["doc1.pdf".to_string(), "doc2.pdf".to_string()]
            } else {
                vec![]
            },
        }).collect()
    }

    #[test]
    fn test_small_pdf_generation() {
        let generator = CertificatePdfGenerator::new();
        let certs = create_test_certificates(10);
        let result = generator.generate_pdf(&certs, "/tmp/test_small.pdf").unwrap();
        
        assert!(result.success);
        assert_eq!(result.total_chunks, 1);
        assert!(!result.is_streamed);
        
        // Cleanup
        let _ = std::fs::remove_file("/tmp/test_small.pdf");
    }

    #[test]
    fn test_large_pdf_generation_with_chunking() {
        let generator = CertificatePdfGenerator::new();
        let certs = create_test_certificates(150); // Should trigger chunking (>50 pages)
        let result = generator.generate_pdf(&certs, "/tmp/test_large.pdf").unwrap();
        
        assert!(result.success);
        assert!(result.total_chunks > 1);
        assert!(result.is_streamed);
        
        // Cleanup
        let _ = std::fs::remove_file("/tmp/test_large.pdf");
    }

    #[test]
    fn test_streaming_pdf_generation() {
        let generator = CertificatePdfGenerator::new();
        let certs = create_test_certificates(200);
        let result = generator.generate_streaming_pdf(&certs, "/tmp/test_streaming.pdf").unwrap();
        
        assert!(result.success);
        assert!(result.is_streamed);
        assert!(result.total_pages > 0);
        
        // Cleanup
        let _ = std::fs::remove_file("/tmp/test_streaming.pdf");
    }

    #[test]
    fn test_chunk_creation() {
        let generator = CertificatePdfGenerator::new();
        let certs = create_test_certificates(150);
        let chunks = generator.create_chunks(&certs);
        
        assert!(chunks.len() > 1);
        assert_eq!(chunks.iter().map(|c| c.certificates.len()).sum::<usize>(), 150);
    }

    #[test]
    fn test_page_count_estimation() {
        let generator = CertificatePdfGenerator::new();
        let certs = create_test_certificates(10);
        let pages = generator.estimate_page_count(&certs);
        
        assert!(pages >= 10); // At least 1 page per certificate
    }

    #[test]
    fn test_custom_config() {
        let config = PdfGenerationConfig {
            pages_per_chunk: 25,
            timeout_seconds: 60,
            streaming_enabled: true,
            streaming_threshold_mb: 5,
        };
        let generator = CertificatePdfGenerator::with_config(config);
        
        let certs = create_test_certificates(100);
        let result = generator.generate_pdf(&certs, "/tmp/test_config.pdf").unwrap();
        
        assert!(result.success);
        assert!(result.total_chunks > 1);
        
        // Cleanup
        let _ = std::fs::remove_file("/tmp/test_config.pdf");
    }
}
