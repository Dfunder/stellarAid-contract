/**
 * SDK network boundary validation (closes #1088).
 *
 * `response.json()` returns plain JSON: `bigint` fields arrive as strings and
 * `as Promise<Concrete>` only silences the compiler. Every query in `sdk/src`
 * parses through a validator from this module instead, so malformed payloads
 * fail immediately with a `ValidationError`.
 */

export class ValidationError extends Error {
  /** Where the bad value came from, e.g. a URL or a dotted JSON path. */
  readonly path: string;

  constructor(message: string, path = '<response>') {
    super(`${path}: ${message}`);
    this.name = 'ValidationError';
    this.path = path;
  }
}

export type Validator<T> = (value: unknown, path: string) => T;

function describe(value: unknown): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

function fail(path: string, expected: string, value: unknown): never {
  throw new ValidationError(`expected ${expected}, got ${describe(value)}`, path);
}

export function asObject(value: unknown, path: string): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    fail(path, 'object', value);
  }
  return value as Record<string, unknown>;
}

export function asArray(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) fail(path, 'array', value);
  return value;
}

export function asString(value: unknown, path: string): string {
  if (typeof value !== 'string') fail(path, 'string', value);
  return value;
}

export function asNullableString(value: unknown, path: string): string | null {
  return value === null ? null : asString(value, path);
}

export function asNumber(value: unknown, path: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    fail(path, 'finite number', value);
  }
  return value;
}

export function asNullableNumber(value: unknown, path: string): number | null {
  return value === null ? null : asNumber(value, path);
}

export function asBoolean(value: unknown, path: string): boolean {
  if (typeof value !== 'boolean') fail(path, 'boolean', value);
  return value;
}

/**
 * JSON has no bigint. Amounts must arrive as decimal strings and are parsed
 * here, after validation, so no consumer ever does NaN math on a number that
 * lost precision.
 */
export function asBigIntString(value: unknown, path: string): bigint {
  if (typeof value !== 'string' || !/^-?(?:0|[1-9]\d*)$/.test(value)) {
    fail(path, 'decimal string for a bigint field', value);
  }
  return BigInt(value);
}

/** Read and validate a required property, with a useful path in any error. */
export function field<T>(
  object: Record<string, unknown>,
  key: string,
  path: string,
  validate: Validator<T>
): T {
  return validate(object[key], `${path}.${key}`);
}

export function parseJson<T>(text: string, validate: Validator<T>, path = '<response>'): T {
  let raw: unknown;
  try {
    raw = JSON.parse(text) as unknown;
  } catch {
    throw new ValidationError('response is not valid JSON', path);
  }
  return validate(raw, path);
}

/** Fetch with a timeout and validate the body before it reaches any caller. */
export async function fetchJson<T>(
  url: string,
  validate: Validator<T>,
  init: RequestInit = {}
): Promise<T> {
  let response: Response;
  try {
    response = await fetch(url, {
      ...init,
      signal: init.signal ?? AbortSignal.timeout(10_000),
    });
  } catch (error: unknown) {
    const detail = error instanceof Error ? error.message : 'network error';
    throw new ValidationError(`request failed: ${detail}`, url);
  }
  if (!response.ok) {
    throw new ValidationError(`request failed with HTTP ${response.status}`, url);
  }
  return parseJson(await response.text(), validate, url);
}

/** Every dynamic path segment goes through encodeURIComponent. */
export function encodePath(value: string): string {
  return encodeURIComponent(value);
}

/** Build a URL from encoded segments and query parameters. */
export function buildUrl(
  base: string,
  segments: readonly string[] = [],
  params: Record<string, string | readonly string[] | undefined> = {}
): string {
  const url = new URL(
    `${base.replace(/\/+$/, '')}/${segments.map(encodePath).join('/')}`
  );
  for (const [key, raw] of Object.entries(params)) {
    if (raw === undefined) continue;
    for (const value of Array.isArray(raw) ? raw : [raw]) {
      url.searchParams.append(key, value);
    }
  }
  return url.toString();
}
