// Add webhook HMAC signing so shared secret is never sent in plaintext (#958)
import { createHmac } from 'crypto';

export interface SignedWebhookPayload {
  payload: string;
  signature: string;
  timestamp: number;
}

export function signWebhookPayload(
  payload: string,
  secret: string
): SignedWebhookPayload {
  const timestamp = Date.now();
  const message = timestamp + '.' + payload;
  const signature = 'sha256=' + createHmac('sha256', secret)
    .update(message, 'utf8')
    .digest('hex');
  return { payload, signature, timestamp };
}
