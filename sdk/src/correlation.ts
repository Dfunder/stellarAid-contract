// Add correlation-id query endpoint so operators can trace a request end to end (#970)
import { randomUUID } from 'crypto';

export function generateCorrelationId(): string {
  return randomUUID();
}

export async function queryWithCorrelationId(
  url: string,
  correlationId?: string
): Promise<{ correlationId: string; data: unknown }> {
  const id = correlationId ?? generateCorrelationId();
  const response = await fetch(url, {
    headers: { 'X-Correlation-ID': id },
  });
  if (!response.ok) {
    throw new Error('Request failed: ' + response.status);
  }
  const data = await response.json();
  return { correlationId: id, data };
}
