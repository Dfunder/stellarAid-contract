// Dispute timeline view with a validated boundary (#965, #1088).

import { asArray, asNullableNumber, asNumber, asObject, asString, buildUrl, fetchJson, field, type Validator } from './net/validate.js';

export interface DisputeEvent {
  timestamp: number;
  eventType: string;
  actor: string;
  details: string;
}

export interface DisputeTimeline {
  disputeId: string;
  events: DisputeEvent[];
  currentStatus: string;
  openedAtMs: number;
  resolvedAtMs: number | null;
}

const validateEvent: Validator<DisputeEvent> = (value, path) => {
  const object = asObject(value, path);
  return {
    timestamp: field(object, 'timestamp', path, asNumber),
    eventType: field(object, 'eventType', path, asString),
    actor: field(object, 'actor', path, asString),
    details: field(object, 'details', path, asString),
  };
};

const validateTimeline: Validator<DisputeTimeline> = (value, path) => {
  const object = asObject(value, path);
  const events = field(object, 'events', path, asArray);
  return {
    disputeId: field(object, 'disputeId', path, asString),
    events: events.map((event, index) => validateEvent(event, `${path}.events[${index}]`)),
    currentStatus: field(object, 'currentStatus', path, asString),
    openedAtMs: field(object, 'openedAtMs', path, asNumber),
    resolvedAtMs: field(object, 'resolvedAtMs', path, asNullableNumber),
  };
};

export async function getDisputeTimeline(
  rpcUrl: string,
  disputeId: string
): Promise<DisputeTimeline> {
  return fetchJson(
    buildUrl(rpcUrl, ['dispute', disputeId, 'timeline']),
    validateTimeline
  );
}
