// Add pagination support for campaign and dispute_arbiter list queries (#973)
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

export function applyPagination<T>(
  items: T[],
  { limit = 20, offset = 0 }: PaginationParams
): PaginatedResult<T> {
  const sliced = items.slice(offset, offset + limit);
  return {
    items: sliced,
    total: items.length,
    limit,
    offset,
    hasMore: offset + limit < items.length,
  };
}
