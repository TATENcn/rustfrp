import type { TrafficSample } from "@/api/types";

// Compare counters only within the same instance, collector, and process lifetime.
export function trafficAnalysis(samples: TrafficSample[], cutoff: number) {
  let receivedTotal = 0,
    sentTotal = 0;
  const previous = new Map<string, TrafficSample>();
  const points = new Map<
    number,
    { timestamp: number; received: number; sent: number }
  >();
  const native = new Set(
    samples.filter((s) => s.source === "linux_tcp").map((s) => s.profile_id),
  );
  for (const sample of [...samples].sort(
    (a, b) => Date.parse(a.timestamp) - Date.parse(b.timestamp),
  )) {
    if (native.has(sample.profile_id) && sample.source !== "linux_tcp")
      continue;
    const timestamp = Date.parse(sample.timestamp);
    if (!Number.isFinite(timestamp)) continue;
    const key = [
      sample.environment_id,
      sample.profile_id,
      sample.proxy_id ?? "",
      sample.interface ?? "",
      sample.source ?? "external",
      sample.counter_epoch ?? "",
    ].join(":");
    const prior = previous.get(key);
    previous.set(key, sample);
    if (!prior || timestamp < cutoff) continue;
    const seconds = (timestamp - Date.parse(prior.timestamp)) / 1000;
    const received = sample.received_bytes - prior.received_bytes;
    const sent = sample.sent_bytes - prior.sent_bytes;
    // Do not bridge missed collection intervals or counter resets with invented rates.
    if (seconds <= 0 || seconds > 15 || received < 0 || sent < 0) continue;
    const point = points.get(timestamp) ?? { timestamp, received: 0, sent: 0 };
    receivedTotal += received;
    sentTotal += sent;
    point.received += received / seconds;
    point.sent += sent / seconds;
    points.set(timestamp, point);
  }
  return { points: [...points.values()], receivedTotal, sentTotal };
}

export function trafficRates(samples: TrafficSample[], cutoff: number) {
  return trafficAnalysis(samples, cutoff).points;
}
