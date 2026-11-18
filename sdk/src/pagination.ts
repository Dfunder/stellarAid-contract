// Add pagination support for campaign and dispute_arbiter list queries (#973)

/** Maximum number of items returned in a single page. */
export const MAX_LIMIT = 100;

export interface PaginationParams {
  limit?: number;
  offset?: number;
}

export interface PaginatedResult<T> {
  items: T[];
  total: number;
  limit: number;
  offset: number;
  hasMore: boolean;
}

function normalizeLimit(limit: number | undefined): number {
  const raw = limit ?? 20;
  if (!Number.isFinite(raw)) return 20;
  const asInt = Math.trunc(raw);
  return Math.min(MAX_LIMIT, Math.max(1, asInt));
}

function normalizeOffset(offset: number | undefined): number {
  const raw = offset ?? 0;
  if (!Number.isFinite(raw)) return 0;
  return Math.max(0, Math.trunc(raw));
}

export function applyPagination<T>(
  items: T[],
  { limit, offset }: PaginationParams = {}
): PaginatedResult<T> {
  const normalizedLimit = normalizeLimit(limit);
  const normalizedOffset = normalizeOffset(offset);
  const sliced = items.slice(
    normalizedOffset,
    normalizedOffset + normalizedLimit
  );
  return {
    items: sliced,
    total: items.length,
    limit: normalizedLimit,
    offset: normalizedOffset,
    hasMore: normalizedOffset + sliced.length < items.length,
  };
}
