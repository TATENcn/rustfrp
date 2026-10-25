//! Best-effort, read-only transport monitoring for managed frpc processes.
//! Polling does not capture sockets that open and close between snapshots.
use crate::metrics::MetricsStore;
use chrono::{DateTime, Utc};
use rustfrp_client::{db::Database, process::manager::ProcessManager};
use serde::Serialize;

#[cfg(target_os = "linux")]
mod linux;

#[derive(Debug, Clone, Serialize)]
pub struct ProfileCollection {
    pub profile_id: i64,
    pub environment_id: i64,
    pub state: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProxyCollection {
    pub profile_id: i64,
    pub environment_id: i64,
    pub proxy_id: i64,
    pub name: String,
    pub state: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionStatus {
    pub source: &'static str,
    pub state: &'static str,
    pub updated_at: Option<DateTime<Utc>>,
    pub profiles: Vec<ProfileCollection>,
    pub proxies: Vec<ProxyCollection>,
    pub host_state: &'static str,
}
impl Default for CollectionStatus {
    fn default() -> Self {
        Self {
            source: "linux_tcp",
            state: "initializing",
            updated_at: None,
            profiles: Vec::new(),
            proxies: Vec::new(),
            host_state: "initializing",
        }
    }
}

/// Starts a separate sampler; its failures never change the forwarding lifecycle.
pub fn spawn(store: MetricsStore, db: Database, manager: ProcessManager) {
    tokio::spawn(async move {
        #[cfg(target_os = "linux")]
        let mut collector = Collector::default();
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            #[cfg(target_os = "linux")]
            let status = collector.collect(&store, &db, &manager).await;
            #[cfg(not(target_os = "linux"))]
            let status = {
                let _ = (&db, &manager);
                CollectionStatus {
                    state: "unsupported_platform",
                    host_state: "unsupported_platform",
                    updated_at: Some(Utc::now()),
                    ..Default::default()
                }
            };
            store.set_collection(status).await;
        }
    });
}

#[cfg(target_os = "linux")]
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    time::{Duration, Instant},
};
#[cfg(target_os = "linux")]
#[derive(Default)]
struct CounterState {
    identity: (u32, String, i64),
    previous: HashMap<u64, (u64, u64)>,
    received: u64,
    sent: u64,
    initialized: bool,
}
#[cfg(target_os = "linux")]
impl CounterState {
    fn update(
        &mut self,
        identity: (u32, String, i64),
        sockets: &[linux::SocketCounter],
    ) -> (u64, u64) {
        if self.identity != identity {
            *self = Self {
                identity,
                ..Default::default()
            };
        }
        let mut current = HashMap::new();
        for socket in sockets {
            let baseline = self.previous.get(&socket.cookie).copied().unwrap_or({
                if self.initialized {
                    (0, 0)
                } else {
                    (socket.received, socket.sent)
                }
            });
            self.received = self
                .received
                .saturating_add(socket.received.saturating_sub(baseline.0));
            self.sent = self
                .sent
                .saturating_add(socket.sent.saturating_sub(baseline.1));
            current.insert(socket.cookie, (socket.received, socket.sent));
        }
        self.previous = current;
        self.initialized = true;
        (self.received, self.sent)
    }
}

#[cfg(target_os = "linux")]
#[derive(Default)]
struct Collector {
    counters: HashMap<i64, CounterState>,
    proxy_counters: HashMap<(i64, i64), CounterState>,
    host_counters: HashMap<String, CounterState>,
    dns: HashMap<String, (Instant, Vec<IpAddr>)>,
}
#[cfg(target_os = "linux")]
fn normalized(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(ip)),
        other => other,
    }
}
#[cfg(target_os = "linux")]
impl Collector {
    async fn collect_host(&mut self, store: &MetricsStore, status: &mut CollectionStatus) {
        let result = tokio::task::spawn_blocking(linux::interfaces).await;
        match result {
            Ok(Ok(rows)) => {
                status.host_state = "collecting";
                self.host_counters
                    .retain(|name, _| rows.iter().any(|r| &r.0 == name));
                let timestamp = Utc::now();
                for (name, index, rx, tx) in rows {
                    let counter = self.host_counters.entry(name.clone()).or_default();
                    let socket = linux::SocketCounter {
                        inode: 0,
                        cookie: 0,
                        peer: IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
                        port: 0,
                        received: rx,
                        sent: tx,
                    };
                    // Interface replacement or a reset begins a fresh epoch.
                    if counter
                        .previous
                        .get(&0)
                        .is_some_and(|(r, t)| rx < *r || tx < *t)
                    {
                        *counter = CounterState::default();
                    }
                    if !counter.initialized {
                        counter.identity = (
                            index,
                            Utc::now()
                                .timestamp_nanos_opt()
                                .unwrap_or_default()
                                .to_string(),
                            0,
                        );
                    }
                    let identity = (index, counter.identity.1.clone(), 0);
                    let (received_bytes, sent_bytes) = counter.update(identity, &[socket]);
                    store
                        .push_traffic(crate::metrics::TrafficSample {
                            timestamp,
                            environment_id: 0,
                            profile_id: 0,
                            proxy_id: None,
                            interface: Some(name),
                            received_bytes,
                            sent_bytes,
                            active_connections: 0,
                            source: "linux_interface".into(),
                            counter_epoch: format!("{index}:{}", counter.identity.1),
                        })
                        .await;
                }
            }
            _ => status.host_state = "collector_unavailable",
        }
    }

    async fn addresses(&mut self, host: &str) -> Result<Vec<IpAddr>, ()> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![normalized(ip)]);
        }
        if let Some((at, ips)) = self.dns.get(host) {
            if at.elapsed() < Duration::from_secs(60) {
                return if ips.is_empty() {
                    Err(())
                } else {
                    Ok(ips.clone())
                };
            }
        }
        let addresses =
            match tokio::time::timeout(Duration::from_secs(1), tokio::net::lookup_host((host, 0)))
                .await
            {
                Ok(Ok(addresses)) => addresses.map(|a| normalized(a.ip())).collect::<Vec<_>>(),
                _ => Vec::new(),
            };
        self.dns
            .insert(host.to_owned(), (Instant::now(), addresses.clone()));
        if addresses.is_empty() {
            Err(())
        } else {
            Ok(addresses)
        }
    }

    async fn collect(
        &mut self,
        store: &MetricsStore,
        db: &Database,
        manager: &ProcessManager,
    ) -> CollectionStatus {
        let mut status = CollectionStatus {
            state: "ready",
            updated_at: Some(Utc::now()),
            ..Default::default()
        };
        self.collect_host(store, &mut status).await;
        let bindings = db.list_bindings().await;
        let proxies = db.list_proxies().await;
        let (bindings, proxies) = match (bindings, proxies) {
            (Ok(b), Ok(p)) => (b, p),
            _ => {
                status.state = "database_error";
                return status;
            }
        };
        self.proxy_counters.retain(|(profile, proxy), _| {
            bindings
                .iter()
                .any(|b| b.enabled && b.profile_id == *profile && b.proxy_id == *proxy)
        });
        let profiles = match db.list_profiles().await {
            Ok(profiles) => profiles,
            Err(_) => {
                status.state = "database_error";
                return status;
            }
        };
        let ids: HashSet<_> = profiles.iter().filter_map(|p| p.id).collect();
        self.counters.retain(|id, _| ids.contains(id));
        self.dns.retain(|host, _| {
            profiles.iter().any(|p| &p.server_addr == host)
                || proxies.iter().any(|p| &p.local_ip == host)
        });
        let processes = manager.list_running().await;
        // One bounded kernel dump for all profiles; keep blocking syscalls off the async executor.
        let snapshot = if processes.iter().any(|p| p.running) {
            match tokio::task::spawn_blocking(linux::snapshot).await {
                Ok(Ok(sockets)) => Some(sockets),
                _ => None,
            }
        } else {
            Some(Vec::new())
        };
        let timestamp = Utc::now();
        for profile in profiles {
            let Some(id) = profile.id else { continue };
            let environment_id = match db.profile_environment_id(id).await {
                Ok(id) => id,
                Err(_) => {
                    status.state = "database_error";
                    continue;
                }
            };
            let mut item = ProfileCollection {
                profile_id: id,
                environment_id,
                state: "stopped",
            };
            let process = processes.iter().find(|p| p.profile_id == id && p.running);
            if let Some(process) = process {
                if !matches!(
                    profile.transport_protocol.as_str(),
                    "tcp" | "websocket" | "wss"
                ) || profile.proxy_url.as_ref().is_some_and(|v| !v.is_empty())
                {
                    item.state = "unsupported_transport";
                } else if snapshot.is_none() {
                    item.state = "collector_unavailable";
                }
                if let Some(pid) = process.pid {
                    match self.addresses(&profile.server_addr).await {
                        Err(()) => item.state = "resolve_failed",
                        Ok(addresses) => {
                            let config = process.config_path.clone();
                            let ownership =
                                tokio::task::spawn_blocking(move || -> std::io::Result<_> {
                                    let identity = linux::process_identity(pid)?;
                                    let cmd = std::fs::read(format!("/proc/{pid}/cmdline"))?;
                                    if !cmd.split(|c| *c == 0).any(|arg| arg == config.as_bytes()) {
                                        return Err(std::io::Error::other(
                                            "process identity changed",
                                        ));
                                    }
                                    let inodes = linux::process_sockets(pid)?;
                                    if linux::process_identity(pid)? != identity {
                                        return Err(std::io::Error::other(
                                            "process identity changed",
                                        ));
                                    }
                                    Ok((identity, inodes))
                                })
                                .await;
                            match ownership {
                                Ok(Ok((identity, inodes))) => {
                                    let owned: Vec<_> = snapshot
                                        .as_ref()
                                        .into_iter()
                                        .flatten()
                                        .filter(|s| inodes.contains(&s.inode))
                                        .cloned()
                                        .collect();
                                    let mut targets = Vec::new();
                                    for binding in
                                        bindings.iter().filter(|b| b.profile_id == id && b.enabled)
                                    {
                                        if let Some(proxy) =
                                            proxies.iter().find(|p| p.id == Some(binding.proxy_id))
                                        {
                                            let ips = self
                                                .addresses(&proxy.local_ip)
                                                .await
                                                .unwrap_or_default();
                                            targets.push((proxy, ips));
                                        }
                                    }
                                    for (proxy, ips) in &targets {
                                        let Some(proxy_id) = proxy.id else { continue };
                                        let ambiguous = targets.iter().any(|(other, other_ips)| {
                                            other.id != proxy.id
                                                && other.local_port == proxy.local_port
                                                && ips.iter().any(|ip| other_ips.contains(ip))
                                        });
                                        let mut local_state = "collecting";
                                        if matches!(
                                            proxy.proxy_type.to_string().as_str(),
                                            "udp" | "sudp"
                                        ) || proxy.plugin_config.is_some()
                                        {
                                            local_state = "unsupported_proxy";
                                        } else if ips.is_empty() {
                                            local_state = "resolve_failed";
                                        } else if ambiguous
                                            || (proxy.local_port == profile.server_port
                                                && ips.iter().any(|ip| addresses.contains(ip)))
                                        {
                                            local_state = "ambiguous_endpoint";
                                        } else if snapshot.is_none() {
                                            local_state = "collector_unavailable";
                                        }
                                        if local_state == "collecting" {
                                            let local: Vec<_> = owned
                                                .iter()
                                                .filter(|s| {
                                                    s.port == proxy.local_port
                                                        && ips.contains(&normalized(s.peer))
                                                })
                                                .cloned()
                                                .collect();
                                            let counter = self
                                                .proxy_counters
                                                .entry((id, proxy_id))
                                                .or_default();
                                            let (rx, tx) = counter.update(
                                                (
                                                    pid,
                                                    format!(
                                                        "{identity}:{}:{}",
                                                        proxy.local_ip, proxy.local_port
                                                    ),
                                                    environment_id,
                                                ),
                                                &local,
                                            );
                                            // Direction is from the local service: inbound is delivered to it, outbound is read from it.
                                            store
                                                .push_traffic(crate::metrics::TrafficSample {
                                                    timestamp,
                                                    environment_id,
                                                    profile_id: id,
                                                    proxy_id: Some(proxy_id),
                                                    interface: None,
                                                    received_bytes: tx,
                                                    sent_bytes: rx,
                                                    active_connections: local.len() as u64,
                                                    source: "linux_local_tcp".into(),
                                                    counter_epoch: format!(
                                                        "{pid}:{}",
                                                        counter.identity.1
                                                    ),
                                                })
                                                .await;
                                        }
                                        if local_state != "collecting" {
                                            self.proxy_counters.remove(&(id, proxy_id));
                                        }
                                        status.proxies.push(ProxyCollection {
                                            profile_id: id,
                                            environment_id,
                                            proxy_id,
                                            name: proxy.name.clone(),
                                            state: local_state,
                                        });
                                    }
                                    if item.state == "unsupported_transport" || snapshot.is_none() {
                                        status.profiles.push(item);
                                        continue;
                                    }
                                    let sockets: Vec<_> = snapshot
                                        .as_ref()
                                        .into_iter()
                                        .flatten()
                                        .filter(|s| {
                                            inodes.contains(&s.inode)
                                                && s.port == profile.server_port
                                                && addresses.contains(&normalized(s.peer))
                                        })
                                        .cloned()
                                        .collect();
                                    let (received, sent) = self
                                        .counters
                                        .entry(id)
                                        .or_default()
                                        .update((pid, identity, environment_id), &sockets);
                                    item.state = if sockets.is_empty() {
                                        "no_connection"
                                    } else {
                                        "collecting"
                                    };
                                    // No connection is unknown, not a fabricated zero sample.
                                    if !sockets.is_empty() {
                                        store
                                            .push_traffic(crate::metrics::TrafficSample {
                                                timestamp,
                                                environment_id,
                                                profile_id: id,
                                                received_bytes: received,
                                                sent_bytes: sent,
                                                active_connections: sockets.len() as u64,
                                                proxy_id: None,
                                                interface: None,
                                                source: "linux_tcp".into(),
                                                counter_epoch: format!(
                                                    "{pid}-{}",
                                                    self.counters[&id].identity.1
                                                ),
                                            })
                                            .await;
                                    }
                                }
                                _ => item.state = "process_unavailable",
                            }
                        }
                    }
                }
            } else {
                self.counters.remove(&id);
                self.proxy_counters.retain(|(profile, _), _| *profile != id);
            }
            for binding in bindings.iter().filter(|b| b.profile_id == id && b.enabled) {
                if !status
                    .proxies
                    .iter()
                    .any(|p| p.profile_id == id && p.proxy_id == binding.proxy_id)
                {
                    status.proxies.push(ProxyCollection {
                        profile_id: id,
                        environment_id,
                        proxy_id: binding.proxy_id,
                        name: proxies
                            .iter()
                            .find(|p| p.id == Some(binding.proxy_id))
                            .map(|p| p.name.clone())
                            .unwrap_or_default(),
                        state: item.state,
                    });
                }
            }
            status.profiles.push(item);
        }
        status
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    fn socket(cookie: u64, received: u64, sent: u64) -> linux::SocketCounter {
        linux::SocketCounter {
            inode: 1,
            cookie,
            peer: "127.0.0.1".parse().unwrap(),
            port: 7000,
            received,
            sent,
        }
    }
    #[test]
    fn counter_deltas_do_not_double_count_and_reset_on_process_or_environment_change() {
        let mut counters = CounterState::default();
        let identity = (42, "100".into(), 1);
        assert_eq!(
            counters.update(identity.clone(), &[socket(1, 100, 200)]),
            (0, 0)
        );
        assert_eq!(
            counters.update(identity.clone(), &[socket(1, 150, 260)]),
            (50, 60)
        );
        assert_eq!(
            counters.update(identity.clone(), &[socket(1, 150, 260)]),
            (50, 60)
        );
        assert_eq!(
            counters.update(identity.clone(), &[socket(2, 20, 30)]),
            (70, 90)
        );
        assert_eq!(counters.update(identity.clone(), &[]), (70, 90));
        assert_eq!(
            counters.update((42, "101".into(), 1), &[socket(3, 50, 50)]),
            (0, 0)
        );
        assert_eq!(
            counters.update((42, "101".into(), 2), &[socket(3, 80, 80)]),
            (0, 0)
        );
    }
}
