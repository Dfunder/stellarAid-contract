import { buildDonationPaymentLink } from '../sep7';

describe('Issue #373: SEP-0007 Payment Link Serialization Tests', () => {
  const mockParams = {
    destination: 'GBALBEDO76V6K67X4PZ72NC556XU54K75QNZP2Z5F4O7V6Y456QWERTY',
    amount: '150.5000',
    memo: 'CAMPAIGN_99',
    campaignId: 'c10293',
  };

  it('should successfully output a properly prefixed web+stellar:pay protocol link', () => {
    const result = buildDonationPaymentLink(mockParams);
    expect(result).toBeDefined();
    expect(result.startsWith('web+stellar:pay?')).toBe(true);
  });

  it('should explicitly URI-encode query tracking text metrics and match key specifications', () => {
    const result = buildDonationPaymentLink(mockParams);
    const urlParams = new URLSearchParams(result.split('?')[1]);

    expect(urlParams.get('destination')).toBe(mockParams.destination);
    expect(urlParams.get('amount')).toBe(mockParams.amount);
    expect(urlParams.get('memo')).toBe(mockParams.memo);
    expect(urlParams.get('memo_type')).toBe('MEMO_TEXT');
    expect(urlParams.get('msg')).toContain('c10293');
  });

  it('should drop execution routines if the destination string breaks structural integrity rules', () => {
    expect(() => {
      buildDonationPaymentLink({ ...mockParams, destination: 'INVALID_STELLAR_ADDRESS' });
    }).toThrow();
  });
});