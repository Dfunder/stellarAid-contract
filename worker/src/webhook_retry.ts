// Add webhook retry with exponential backoff (#959)
export interface WebhookDelivery {
  id: string;
  url: string;
  payload: string;
  attempts: number;
  lastAttemptMs: number;
  delivered: boolean;
}

const MAX_ATTEMPTS = 5;
const BASE_DELAY_MS = 1000;

export function getNextRetryDelayMs(attempts: number): number {
  return BASE_DELAY_MS * Math.pow(2, attempts);
}

export function shouldRetry(delivery: WebhookDelivery): boolean {
  return !delivery.delivered && delivery.attempts < MAX_ATTEMPTS;
}
