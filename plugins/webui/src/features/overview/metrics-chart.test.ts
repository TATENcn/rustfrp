import { describe, it, expect } from "bun:test";
import { bucketAverages, type ChartPoint } from "./metrics-chart";

const point = (timestamp: number, host: number, daemon: number): ChartPoint => ({
  timestamp,
  host,
  daemon,
});

describe("chart bucketing", () => {
  it("leaves a short series untouched", () => {
    const points = [point(1, 10, 1), point(2, 20, 2)];
    // Identity, not a copy: there is nothing to smooth and no work to do.
    expect(bucketAverages(points)).toBe(points);
  });

  it("caps the drawn points and averages each bucket", () => {
    // 1000 samples of a constant 50 must still read as 50 after averaging;
    // the cap is about pixels, not about changing the value.
    const points = Array.from({ length: 1000 }, (_, i) => point(i, 50, 50));
    const out = bucketAverages(points);
    expect(out.length).toBeLessThanOrEqual(240);
    for (const p of out) {
      expect(p.host).toBeCloseTo(50);
      expect(p.daemon).toBeCloseTo(50);
    }
  });

  it("smooths a spike instead of hiding it", () => {
    // One 100% sample between 3599 idle ones must not dominate the chart, but
    // it must still move its bucket — a mean, not a filter that drops outliers.
    const points = Array.from({ length: 3600 }, (_, i) =>
      point(i, i === 0 ? 100 : 0, 0),
    );
    const out = bucketAverages(points);
    const first = out[0].host;
    expect(first).toBeGreaterThan(0);
    expect(first).toBeLessThan(100);
    // Everything after the spike bucket is flat.
    for (const p of out.slice(1)) expect(p.host).toBeCloseTo(0);
  });

  it("keeps the series anchored to the newest sample", () => {
    const points = Array.from({ length: 500 }, (_, i) => point(i * 1000, 1, 1));
    const out = bucketAverages(points);
    // The last bucket carries the last timestamp, so the right edge of the
    // chart is still "now" rather than half a bucket in the past.
    expect(out.at(-1)!.timestamp).toBe(points.at(-1)!.timestamp);
    // And the first bucket starts at the first sample, so nothing is clipped.
    expect(out[0].timestamp).toBeGreaterThanOrEqual(points[0].timestamp);
    expect(out[0].timestamp).toBeLessThan(points[240].timestamp);
  });

  it("never produces more buckets than inputs", () => {
    for (const n of [241, 300, 3600]) {
      const points = Array.from({ length: n }, (_, i) => point(i, 1, 1));
      expect(bucketAverages(points).length).toBeLessThanOrEqual(240);
    }
  });
});
