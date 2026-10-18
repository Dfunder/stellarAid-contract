// Add rate-limit status endpoint to the worker (#969)
export interface RateLimitBucket {
  key: string;
  limit: number;
  used: number;
  remaining: number;
  resetAtMs: number;
}

const buckets = new Map<string, RateLimitBucket>();

export function getRateLimitStatus(key: string): RateLimitBucket | null {
  return buckets.get(key) ?? null;
}

export function getAllRateLimitBuckets(): RateLimitBucket[] {
  return Array.from(buckets.values());
}

export function setRateLimitBucket(bucket: RateLimitBucket): void {
  buckets.set(bucket.key, bucket);
}
