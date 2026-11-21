# StellarAid contract workspace Makefile
#
# Benchmarks (closes #860)
#   make bench          Run all workspace benchmarks (cargo bench --workspace)
#   make bench-compare  Run benchmarks and compare against the last saved run
#                       (stores results in .bench/results.txt; previous run in
#                       .bench/results.prev.txt). First run establishes a baseline.

BENCH_DIR := .bench
BENCH_RESULTS := $(BENCH_DIR)/results.txt
BENCH_PREV := $(BENCH_DIR)/results.prev.txt

.PHONY: bench bench-compare

bench:
	cargo bench --workspace

bench-compare:
	@mkdir -p $(BENCH_DIR)
	@if [ -f $(BENCH_RESULTS) ]; then cp $(BENCH_RESULTS) $(BENCH_PREV); fi
	cargo bench --workspace | tee $(BENCH_RESULTS)
	@if [ -f $(BENCH_PREV) ]; then \
		echo ""; \
		echo "=== benchmark comparison (previous vs current) ==="; \
		diff -u $(BENCH_PREV) $(BENCH_RESULTS) || true; \
	else \
		echo ""; \
		echo "No previous benchmark results found; baseline saved to $(BENCH_RESULTS)."; \
	fi
