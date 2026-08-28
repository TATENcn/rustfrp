//! Bounded local time-series storage for resources and FRP traffic counters.

use rustfrp_client::process::manager::ProcessManager;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use sysinfo::{Pid, ProcessesToUpdate, System};
use tokio::sync::RwLock;

pub const RESOURCE_SAMPLE_INTERVAL_SECS: u64 = 3;
pub const METRICS_HISTORY_CAPACITY: usize = 1_200;

#[derive(Debug, Clone, Serialize)]
pub struct ProcessResourceSample {
    pub profile_id: i64,
    pub pid: u32,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceSample {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub daemon_cpu_percent: f32,
    pub daemon_memory_bytes: u64,
    pub system_cpu_percent: f32,
    pub system_memory_used_bytes: u64,
    pub system_memory_total_bytes: u64,
    pub ebpf: EbpfStatus,
    pub processes: Vec<ProcessResourceSample>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EbpfStatus {
    /// Kernel BTF and bpffs are present, so an external/privileged eBPF collector can attach.
    pub available: bool,
    pub pinned_objects: u64,
    pub unprivileged_disabled: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficSample {
    #[serde(default = "chrono::Utc::now")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub environment_id: i64,
    pub profile_id: i64,
    pub received_bytes: u64,
    pub sent_bytes: u64,
    #[serde(default)]
    pub active_connections: u64,
}

#[derive(Clone)]
pub struct MetricsStore {
    capacity: usize,
    resources: Arc<RwLock<VecDeque<ResourceSample>>>,
    traffic: Arc<RwLock<VecDeque<TrafficSample>>>,
}

impl MetricsStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            resources: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            traffic: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
        }
    }

    pub fn spawn_sampler(&self, process_manager: ProcessManager) {
        let store = self.clone();
        tokio::spawn(async move {
            let mut system = System::new();
            loop {
                let processes = process_manager.list_running().await;
                let mut pids = vec![sysinfo::get_current_pid().expect("daemon process id")];
                pids.extend(
                    processes
                        .iter()
                        .filter_map(|process| process.pid.map(Pid::from_u32)),
                );
                system.refresh_memory();
                system.refresh_cpu_usage();
                system.refresh_processes(ProcessesToUpdate::Some(&pids), true);
                let daemon = system.process(pids[0]);
                let process_samples = processes
                    .into_iter()
                    .filter_map(|process| {
                        let pid = process.pid?;
                        let info = system.process(Pid::from_u32(pid))?;
                        Some(ProcessResourceSample {
                            profile_id: process.profile_id,
                            pid,
                            cpu_percent: info.cpu_usage(),
                            memory_bytes: info.memory(),
                        })
                    })
                    .collect();
                store
                    .push_resource(ResourceSample {
                        timestamp: chrono::Utc::now(),
                        daemon_cpu_percent: daemon.map_or(0.0, |item| item.cpu_usage()),
                        daemon_memory_bytes: daemon.map_or(0, |item| item.memory()),
                        system_cpu_percent: system.global_cpu_usage(),
                        system_memory_used_bytes: system.used_memory(),
                        system_memory_total_bytes: system.total_memory(),
                        ebpf: detect_ebpf(),
                        processes: process_samples,
                    })
                    .await;
                tokio::time::sleep(std::time::Duration::from_secs(
                    RESOURCE_SAMPLE_INTERVAL_SECS,
                ))
                .await;
            }
        });
    }

    pub async fn push_resource(&self, sample: ResourceSample) {
        let mut resources = self.resources.write().await;
        push_bounded(&mut resources, sample, self.capacity);
    }

    pub async fn push_traffic(&self, sample: TrafficSample) {
        let mut traffic = self.traffic.write().await;
        push_bounded(&mut traffic, sample, self.capacity);
    }

    pub async fn resources(&self, limit: usize) -> Vec<ResourceSample> {
        self.resources
            .read()
            .await
            .iter()
            .rev()
            .take(limit.min(self.capacity))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub async fn traffic(
        &self,
        environment_id: Option<i64>,
        profile_id: Option<i64>,
        limit: usize,
    ) -> Vec<TrafficSample> {
        self.traffic
            .read()
            .await
            .iter()
            .rev()
            .filter(|sample| {
                environment_id.is_none_or(|id| sample.environment_id == id)
                    && profile_id.is_none_or(|id| sample.profile_id == id)
            })
            .take(limit.min(self.capacity))
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }
}

fn detect_ebpf() -> EbpfStatus {
    detect_ebpf_at(
        std::path::Path::new("/sys/kernel/btf/vmlinux"),
        std::path::Path::new("/sys/fs/bpf"),
        std::path::Path::new("/proc/sys/kernel/unprivileged_bpf_disabled"),
    )
}

fn detect_ebpf_at(
    btf: &std::path::Path,
    bpffs: &std::path::Path,
    unprivileged: &std::path::Path,
) -> EbpfStatus {
    let pinned_objects = std::fs::read_dir(bpffs)
        .map(|entries| entries.flatten().take(10_000).count() as u64)
        .unwrap_or(0);
    let unprivileged_disabled = std::fs::read_to_string(unprivileged)
        .ok()
        .and_then(|value| value.trim().parse().ok());
    EbpfStatus {
        available: btf.is_file() && bpffs.is_dir(),
        pinned_objects,
        unprivileged_disabled,
    }
}

fn push_bounded<T>(queue: &mut VecDeque<T>, value: T, capacity: usize) {
    if queue.len() == capacity {
        queue.pop_front();
    }
    queue.push_back(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn traffic_history_is_bounded_and_filterable() {
        let store = MetricsStore::new(2);
        for profile_id in [1, 2, 1] {
            store
                .push_traffic(TrafficSample {
                    timestamp: chrono::Utc::now(),
                    environment_id: 7,
                    profile_id,
                    received_bytes: 1,
                    sent_bytes: 2,
                    active_connections: 0,
                })
                .await;
        }
        assert_eq!(store.traffic(None, None, 10).await.len(), 2);
        assert_eq!(store.traffic(Some(7), Some(1), 10).await.len(), 1);
    }

    #[test]
    fn detects_ebpf_capability_without_requiring_privileges() {
        let root = tempfile::tempdir().unwrap();
        let btf = root.path().join("vmlinux");
        let bpffs = root.path().join("bpf");
        let unprivileged = root.path().join("unprivileged");
        std::fs::write(&btf, b"btf").unwrap();
        std::fs::create_dir(&bpffs).unwrap();
        std::fs::write(bpffs.join("rustfrp-flow"), b"").unwrap();
        std::fs::write(&unprivileged, b"2\n").unwrap();
        let status = detect_ebpf_at(&btf, &bpffs, &unprivileged);
        assert!(status.available);
        assert_eq!(status.pinned_objects, 1);
        assert_eq!(status.unprivileged_disabled, Some(2));
    }
}
