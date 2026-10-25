import { describe, it, expect } from "bun:test";
import { trafficRates, trafficAnalysis } from "./traffic-data";
const sample = (
  seconds: number,
  profile = 1,
  received = 100,
  sent = 50,
  epoch = "one",
) => ({
  timestamp: new Date(seconds * 1000).toISOString(),
  environment_id: 1,
  profile_id: profile,
  received_bytes: received,
  sent_bytes: sent,
  active_connections: 1,
  source: "linux_tcp",
  counter_epoch: epoch,
});
describe("traffic rates", () => {
  it("uses independent profile baselines and sums simultaneous intervals", () => {
    expect(
      trafficRates(
        [
          sample(0),
          sample(3, 1, 400, 200),
          sample(0, 2),
          sample(3, 2, 700, 350),
        ],
        0,
      ),
    ).toEqual([{ timestamp: 3000, received: 300, sent: 150 }]);
  });
  it("does not invent a rate for a single sample, reset, long gap, or new process", () => {
    expect(
      trafficRates(
        [
          sample(0),
          sample(3, 1, 10, 5),
          sample(6, 1, 200, 200, "two"),
          sample(40, 1, 300, 300, "two"),
        ],
        0,
      ),
    ).toEqual([]);
  });
  it("uses baseline outside the selected time range and excludes external duplicates", () => {
    expect(
      trafficRates(
        [
          sample(0),
          { ...sample(3, 1, 9999, 9999), source: "external" },
          sample(3, 1, 400, 200),
        ],
        2000,
      ),
    ).toEqual([{ timestamp: 3000, received: 100, sent: 50 }]);
  });
});

it("keeps proxy, server and interface identities separate and sums deltas across restarts", () => {
  const rows = [
    { ...sample(0), proxy_id: 1, source: "linux_local_tcp" },
    { ...sample(1, 1, 200, 100), proxy_id: 1, source: "linux_local_tcp" },
    { ...sample(0), proxy_id: 2, source: "linux_local_tcp" },
    { ...sample(1, 1, 400, 200), proxy_id: 2, source: "linux_local_tcp" },
  ];
  const result = trafficAnalysis(rows, 0);
  expect(result.receivedTotal).toBe(400);
  expect(result.sentTotal).toBe(200);
  expect(result.points[0]?.received).toBe(400);
  expect(
    trafficAnalysis(
      rows.filter((s) => s.proxy_id === 1),
      0,
    ).receivedTotal,
  ).toBe(100);
  const host = rows.map((s) => ({
    ...s,
    proxy_id: undefined,
    profile_id: 0,
    source: "linux_interface",
    interface: s.proxy_id === 1 ? "eth0" : "eth1",
  }));
  expect(trafficAnalysis(host, 0).receivedTotal).toBe(400);
  expect(
    trafficAnalysis(
      [
        ...rows,
        { ...rows[1]!, counter_epoch: "restart", received_bytes: 99999 },
      ],
      0,
    ).receivedTotal,
  ).toBe(400);
});
