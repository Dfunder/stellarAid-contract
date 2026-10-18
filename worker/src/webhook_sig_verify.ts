// Use constant-time comparison in webhook signature check (#960)
import { createHmac, timingSafeEqual } from 'crypto';

export function verifyWebhookSignature(
  payload: string,
  secret: string,
  receivedSig: string
): boolean {
  const expected = createHmac('sha256', secret)
    .update(payload, 'utf8')
    .digest('hex');
  const expectedBuf = Buffer.from('sha256=' + expected, 'utf8');
  const receivedBuf = Buffer.from(receivedSig, 'utf8');
  if (expectedBuf.length !== receivedBuf.length) return false;
  return timingSafeEqual(expectedBuf, receivedBuf);
}
