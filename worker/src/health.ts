// Add health and readiness endpoint to the worker (#963)
export interface HealthStatus {
  status: 'ok' | 'degraded' | 'down';
  contractReachable: boolean;
  rpcUrl: string;
  uptimeMs: number;
  checkedAtMs: number;
}

const startedAtMs = Date.now();

export async function getHealthStatus(
  rpcUrl: string,
  contractId: string
): Promise<HealthStatus> {
  let contractReachable = false;
  try {
    const r = await fetch(rpcUrl + '/contract/' + contractId + '/status');
    contractReachable = r.ok;
  } catch { contractReachable = false; }

  return {
    status: contractReachable ? 'ok' : 'degraded',
    contractReachable,
    rpcUrl,
    uptimeMs: Date.now() - startedAtMs,
    checkedAtMs: Date.now(),
  };
}
