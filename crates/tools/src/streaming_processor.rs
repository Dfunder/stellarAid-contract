use anyhow::{Result, Context, anyhow};
use serde::{Serialize, Deserialize};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Configuration for streaming data processing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    /// Maximum records to load into memory at once
    pub batch_size: usize,
    /// Maximum memory usage in MB
    pub max_memory_mb: usize,
    /// Enable disk-based caching for overflow
    pub enable_disk_cache: bool,
    /// Cache directory path
    pub cache_dir: String,
    /// Flush interval (number of records between flushes)
    pub flush_interval: usize,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            batch_size: 1000,
            max_memory_mb: 512,
            enable_disk_cache: false,
            cache_dir: "/tmp/stellaraid_cache".to_string(),
            flush_interval: 500,
        }
    }
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub current_records: usize,
    pub current_memory_mb: f64,
    pub peak_memory_mb: f64,
    pub total_processed: usize,
    pub batches_processed: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
}

/// Streaming data processor for large datasets
pub struct StreamingProcessor<T> {
    config: StreamingConfig,
    buffer: VecDeque<T>,
    stats: MemoryStats,
    start_time: Option<Instant>,
}

impl<T> StreamingProcessor<T> 
where
    T: Serialize + for<'de> Deserialize<'de> + Clone + std::fmt::Debug,
{
    /// Create a new streaming processor with default configuration
    pub fn new() -> Self {
        Self {
            config: StreamingConfig::default(),
            buffer: VecDeque::with_capacity(1000),
            stats: MemoryStats {
                current_records: 0,
                current_memory_mb: 0.0,
                peak_memory_mb: 0.0,
                total_processed: 0,
                batches_processed: 0,
                cache_hits: 0,
                cache_misses: 0,
            },
            start_time: None,
        }
    }

    /// Create a new streaming processor with custom configuration
    pub fn with_config(config: StreamingConfig) -> Self {
        Self {
            config,
            buffer: VecDeque::with_capacity(1000),
            stats: MemoryStats {
                current_records: 0,
                current_memory_mb: 0.0,
                peak_memory_mb: 0.0,
                total_processed: 0,
                batches_processed: 0,
                cache_hits: 0,
                cache_misses: 0,
            },
            start_time: None,
        }
    }

    /// Process a large dataset using streaming/chunked approach
    pub fn process_stream<F, R>(
        &mut self,
        data: &[T],
        mut processor: F,
    ) -> Result<Vec<R>>
    where
        F: FnMut(&[T]) -> Result<Vec<R>>,
    {
        self.start_time = Some(Instant::now());
        let total_records = data.len();
        let mut results = Vec::new();

        println!("🔄 Processing {} records in streaming mode...", total_records);
        println!("   Batch size: {} records", self.config.batch_size);
        println!("   Max memory: {} MB", self.config.max_memory_mb);

        // Process in chunks
        for chunk in data.chunks(self.config.batch_size) {
            // Check memory usage before processing
            self.check_memory_usage()?;

            // Load chunk into buffer
            self.buffer.extend(chunk.iter().cloned());
            self.stats.current_records = self.buffer.len();

            // Process the chunk
            let chunk_result = processor(chunk)?;
            results.extend(chunk_result);

            // Update statistics
            self.stats.total_processed += chunk.len();
            self.stats.batches_processed += 1;

            // Clear buffer to free memory
            self.buffer.clear();
            self.stats.current_records = 0;

            // Log progress
            let progress = (self.stats.total_processed as f64 / total_records as f64) * 100.0;
            let elapsed = self.start_time.unwrap().elapsed();
            
            if self.stats.batches_processed % 10 == 0 || self.stats.total_processed == total_records {
                println!("   Progress: {:.1}% ({}/{} records, {:.2}s elapsed)", 
                         progress, self.stats.total_processed, total_records, elapsed.as_secs_f64());
            }

            // Periodic flush if needed
            if self.stats.total_processed % self.config.flush_interval == 0 {
                self.flush_memory()?;
            }
        }

        self.update_memory_stats();
        self.print_final_stats(total_records);

        Ok(results)
    }

    /// Process data from an iterator (true streaming)
    pub fn process_iterator<I, F, R>(
        &mut self,
        iterator: I,
        mut processor: F,
    ) -> Result<Vec<R>>
    where
        I: Iterator<Item = T>,
        F: FnMut(&[T]) -> Result<Vec<R>>,
    {
        self.start_time = Some(Instant::now());
        let mut results = Vec::new();
        let mut batch = Vec::with_capacity(self.config.batch_size);
        let mut total_count = 0;

        println!("🔄 Processing data stream (batch size: {})...", self.config.batch_size);

        for item in iterator {
            batch.push(item);
            total_count += 1;

            // Process when batch is full
            if batch.len() >= self.config.batch_size {
                self.check_memory_usage()?;

                let chunk_result = processor(&batch)?;
                results.extend(chunk_result);

                self.stats.total_processed += batch.len();
                self.stats.batches_processed += 1;

                // Clear batch to free memory
                batch.clear();
                self.flush_memory()?;
            }
        }

        // Process remaining items
        if !batch.is_empty() {
            let chunk_result = processor(&batch)?;
            results.extend(chunk_result);
            self.stats.total_processed += batch.len();
            self.stats.batches_processed += 1;
        }

        self.update_memory_stats();
        println!("✅ Processed {} total records in {} batches", 
                 self.stats.total_processed, self.stats.batches_processed);

        Ok(results)
    }

    /// Check current memory usage and enforce limits
    fn check_memory_usage(&self) -> Result<()> {
        let current_mem = self.estimate_memory_usage_mb();
        
        if current_mem > self.config.max_memory_mb as f64 {
            return Err(anyhow!(
                "Memory limit exceeded: {:.1} MB > {} MB",
                current_mem,
                self.config.max_memory_mb
            ));
        }

        Ok(())
    }

    /// Estimate current memory usage in MB
    fn estimate_memory_usage_mb(&self) -> f64 {
        // Rough estimate: each record ~1KB (adjust based on actual data size)
        let record_size_kb = 1.0;
        (self.stats.current_records as f64 * record_size_kb) / 1024.0
    }

    /// Update memory statistics
    fn update_memory_stats(&mut self) {
        let current = self.estimate_memory_usage_mb();
        self.stats.current_memory_mb = current;
        
        if current > self.stats.peak_memory_mb {
            self.stats.peak_memory_mb = current;
        }
    }

    /// Flush memory (clear buffers, trigger GC)
    fn flush_memory(&mut self) -> Result<()> {
        self.buffer.clear();
        self.stats.current_records = 0;
        self.update_memory_stats();
        Ok(())
    }

    /// Print final processing statistics
    fn print_final_stats(&self, total_records: usize) {
        let elapsed = self.start_time.unwrap().elapsed();
        
        println!("\n📊 Processing Statistics:");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("Total Records:      {}", total_records);
        println!("Batches Processed:  {}", self.stats.batches_processed);
        println!("Peak Memory:        {:.2} MB", self.stats.peak_memory_mb);
        println!("Processing Time:    {:.2}s", elapsed.as_secs_f64());
        println!("Throughput:         {:.0} records/sec", 
                 total_records as f64 / elapsed.as_secs_f64());
    }

    /// Get current memory statistics
    pub fn get_stats(&self) -> &MemoryStats {
        &self.stats
    }

    /// Get processing time
    pub fn get_elapsed(&self) -> Option<Duration> {
        self.start_time.map(|start| start.elapsed())
    }
}

/// Chunked data loader for reading large datasets efficiently
pub struct ChunkedLoader<T> {
    config: StreamingConfig,
}

impl<T> ChunkedLoader<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    /// Create a new chunked loader
    pub fn new(config: StreamingConfig) -> Self {
        Self { config }
    }

    /// Load data from a file in chunks
    pub fn load_from_file<F>(
        &self,
        file_path: &str,
        mut chunk_handler: F,
    ) -> Result<()>
    where
        F: FnMut(&[T]) -> Result<()>,
    {
        use std::fs::File;
        use std::io::BufReader;
        use std::io::Read;

        println!("📂 Loading data from: {}", file_path);

        // Read file in chunks
        let file = File::open(file_path)
            .context(format!("Failed to open file: {}", file_path))?;
        let mut reader = BufReader::new(file);

        let mut buffer = Vec::with_capacity(self.config.batch_size);
        let mut chunk_buffer = String::new();
        let mut decoder = serde_json::Deserializer::from_reader(&mut reader).into_iter::<T>();

        for item in &mut decoder {
            match item {
                Ok(record) => {
                    buffer.push(record);

                    if buffer.len() >= self.config.batch_size {
                        chunk_handler(&buffer)?;
                        buffer.clear();
                    }
                }
                Err(e) => {
                    return Err(anyhow!("Error parsing JSON: {}", e));
                }
            }
        }

        // Process remaining items
        if !buffer.is_empty() {
            chunk_handler(&buffer)?;
        }

        println!("✅ File loading complete");
        Ok(())
    }

    /// Load data from a JSON array file
    pub fn load_json_array<F>(
        &self,
        file_path: &str,
        mut chunk_handler: F,
    ) -> Result<usize>
    where
        F: FnMut(&[T]) -> Result<()>,
    {
        use std::fs::File;
        use std::io::BufReader;

        let file = File::open(file_path)
            .context(format!("Failed to open file: {}", file_path))?;
        let reader = BufReader::new(file);

        let mut total_count = 0;
        let mut buffer = Vec::with_capacity(self.config.batch_size);

        // Parse JSON array in streaming fashion
        let mut deserializer = serde_json::Deserializer::from_reader(reader);
        let mut seq = deserializer.begin_array().unwrap();

        while let Ok(Some(item)) = seq.next() {
            let record: T = item?;
            buffer.push(record);
            total_count += 1;

            if buffer.len() >= self.config.batch_size {
                chunk_handler(&buffer)?;
                buffer.clear();
            }
        }

        // Process remaining
        if !buffer.is_empty() {
            chunk_handler(&buffer)?;
        }

        Ok(total_count)
    }
}

/// Memory-efficient data aggregator
pub struct StreamingAggregator {
    config: StreamingConfig,
    count: usize,
    sum: f64,
    min: f64,
    max: f64,
}

impl StreamingAggregator {
    /// Create a new streaming aggregator
    pub fn new(config: StreamingConfig) -> Self {
        Self {
            config,
            count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
        }
    }

    /// Process records and compute aggregates
    pub fn aggregate<F, T>(
        &mut self,
        data: &[T],
        value_extractor: F,
    ) -> Result<AggregationResult>
    where
        F: Fn(&T) -> f64,
    {
        let chunk_size = self.config.batch_size;

        for chunk in data.chunks(chunk_size) {
            for record in chunk {
                let value = value_extractor(record);
                self.count += 1;
                self.sum += value;
                self.min = self.min.min(value);
                self.max = self.max.max(value);
            }
        }

        Ok(AggregationResult {
            count: self.count,
            sum: self.sum,
            mean: if self.count > 0 { self.sum / self.count as f64 } else { 0.0 },
            min: self.min,
            max: self.max,
        })
    }
}

/// Aggregation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationResult {
    pub count: usize,
    pub sum: f64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
}

impl std::fmt::Display for AggregationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "📊 Aggregation Results:")?;
        writeln!(f, "  Count: {}", self.count)?;
        writeln!(f, "  Sum: {:.2}", self.sum)?;
        writeln!(f, "  Mean: {:.2}", self.mean)?;
        writeln!(f, "  Min: {:.2}", self.min)?;
        writeln!(f, "  Max: {:.2}", self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestRecord {
        id: usize,
        value: f64,
        name: String,
    }

    #[test]
    fn test_streaming_processor_small_dataset() {
        let mut processor = StreamingProcessor::new();
        
        let data: Vec<TestRecord> = (0..100)
            .map(|i| TestRecord {
                id: i,
                value: i as f64 * 1.5,
                name: format!("Record {}", i),
            })
            .collect();

        let results = processor.process_stream(&data, |chunk| {
            Ok(chunk.iter().map(|r| r.id * 2).collect())
        }).unwrap();

        assert_eq!(results.len(), 100);
        assert_eq!(results[0], 0);
        assert_eq!(results[99], 198);
    }

    #[test]
    fn test_streaming_processor_large_dataset() {
        let mut processor = StreamingProcessor::with_config(StreamingConfig {
            batch_size: 50,
            max_memory_mb: 100,
            ..StreamingConfig::default()
        });

        let data: Vec<TestRecord> = (0..1000)
            .map(|i| TestRecord {
                id: i,
                value: i as f64,
                name: format!("Record {}", i),
            })
            .collect();

        let results = processor.process_stream(&data, |chunk| {
            Ok(chunk.iter().map(|r| r.value).collect())
        }).unwrap();

        assert_eq!(results.len(), 1000);
        assert!(processor.stats.batches_processed > 1);
    }

    #[test]
    fn test_memory_usage_tracking() {
        let mut processor = StreamingProcessor::new();
        
        let data: Vec<TestRecord> = (0..500)
            .map(|i| TestRecord {
                id: i,
                value: i as f64,
                name: format!("Record {}", i),
            })
            .collect();

        processor.process_stream(&data, |chunk| {
            Ok(chunk.iter().map(|r| r.id).collect::<Vec<_>>())
        }).unwrap();

        assert!(processor.stats.peak_memory_mb > 0.0);
        assert_eq!(processor.stats.total_processed, 500);
    }

    #[test]
    fn test_streaming_aggregator() {
        let config = StreamingConfig::default();
        let mut aggregator = StreamingAggregator::new(config);

        let data: Vec<TestRecord> = (0..100)
            .map(|i| TestRecord {
                id: i,
                value: i as f64,
                name: format!("Record {}", i),
            })
            .collect();

        let result = aggregator.aggregate(&data, |r| r.value).unwrap();

        assert_eq!(result.count, 100);
        assert!((result.mean - 49.5).abs() < 0.1);
        assert!((result.min - 0.0).abs() < 0.01);
        assert!((result.max - 99.0).abs() < 0.01);
    }

    #[test]
    fn test_chunked_processing() {
        let mut processor = StreamingProcessor::with_config(StreamingConfig {
            batch_size: 10,
            ..StreamingConfig::default()
        });

        let data: Vec<TestRecord> = (0..100).map(|i| TestRecord {
            id: i,
            value: i as f64,
            name: format!("Item {}", i),
        }).collect();

        let mut batch_count = 0;
        let results = processor.process_stream(&data, |chunk| {
            batch_count += 1;
            Ok(chunk.iter().map(|r| r.id).collect::<Vec<_>>())
        }).unwrap();

        assert_eq!(batch_count, 10); // 100 records / 10 batch_size
        assert_eq!(results.len(), 100);
    }
}
