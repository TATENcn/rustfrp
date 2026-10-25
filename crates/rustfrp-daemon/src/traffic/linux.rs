//! Read-only Linux SOCK_DIAG snapshots. No packet capture, root, or FRP changes.
use std::collections::HashSet;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct SocketCounter {
    pub inode: u64,
    pub cookie: u64,
    pub peer: IpAddr,
    pub port: u16,
    pub received: u64,
    pub sent: u64,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid socket diagnostic response",
    )
}
fn u32_at(bytes: &[u8], offset: usize) -> io::Result<u32> {
    Ok(u32::from_ne_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn u64_at(bytes: &[u8], offset: usize) -> io::Result<u64> {
    Ok(u64::from_ne_bytes(
        bytes
            .get(offset..offset + 8)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn aligned(len: usize) -> usize {
    (len + 3) & !3
}

pub fn process_identity(pid: u32) -> io::Result<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    // comm can contain spaces and parentheses; starttime is field 22.
    let tail = stat.rsplit_once(')').ok_or_else(invalid)?.1;
    tail.split_whitespace()
        .nth(19)
        .map(str::to_owned)
        .ok_or_else(invalid)
}

pub fn process_sockets(pid: u32) -> io::Result<HashSet<u64>> {
    let mut inodes = HashSet::new();
    for (count, entry) in std::fs::read_dir(format!("/proc/{pid}/fd"))?.enumerate() {
        if count >= 65536 {
            return Err(io::Error::other("too many process descriptors"));
        }
        let entry = entry?;
        let link = match std::fs::read_link(entry.path()) {
            Ok(link) => link,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        if let Some(inode) = link
            .to_str()
            .and_then(|v| v.strip_prefix("socket:["))
            .and_then(|v| v.strip_suffix(']'))
            .and_then(|v| v.parse().ok())
        {
            inodes.insert(inode);
        }
    }
    Ok(inodes)
}

fn parse_socket(body: &[u8]) -> io::Result<Option<SocketCounter>> {
    if body.len() < 72 {
        return Err(invalid());
    }
    // Only established TCP sockets; exclude listeners and both local/backend legs.
    if body[1] != 1 {
        return Ok(None);
    }
    let peer = match i32::from(body[0]) {
        libc::AF_INET => IpAddr::V4(Ipv4Addr::new(body[24], body[25], body[26], body[27])),
        libc::AF_INET6 => IpAddr::V6(Ipv6Addr::from(
            <[u8; 16]>::try_from(&body[24..40]).map_err(|_| invalid())?,
        )),
        _ => return Ok(None),
    };
    let mut attrs = &body[72..];
    while attrs.len() >= 4 {
        let len = usize::from(u16::from_ne_bytes([attrs[0], attrs[1]]));
        let kind = u16::from_ne_bytes([attrs[2], attrs[3]]) & 0x3fff;
        if len < 4 || len > attrs.len() {
            return Err(invalid());
        }
        if kind == 2 {
            let info = &attrs[4..len];
            // Linux tcp_info ABI: bytes_acked at 120; bytes_received at 128.
            // Reject old kernels without these fields instead of reporting false zeros.
            return Ok(Some(SocketCounter {
                inode: u64::from(u32_at(body, 68)?),
                cookie: u64_at(body, 44)?,
                peer,
                port: u16::from_be_bytes([body[6], body[7]]),
                received: u64_at(info, 128)?,
                sent: u64_at(info, 120)?.saturating_sub(1), // SYN sequence number
            }));
        }
        let next = aligned(len);
        if next > attrs.len() {
            break;
        }
        attrs = &attrs[next..];
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "TCP byte counters unavailable",
    ))
}

pub fn snapshot() -> io::Result<Vec<SocketCounter>> {
    let mut sockets = dump(libc::AF_INET as u8)?;
    match dump(libc::AF_INET6 as u8) {
        Ok(ipv6) => sockets.extend(ipv6),
        Err(e)
            if matches!(
                e.raw_os_error(),
                Some(libc::EAFNOSUPPORT | libc::EPROTONOSUPPORT)
            ) => {}
        Err(e) => return Err(e),
    }
    Ok(sockets)
}

fn dump(family: u8) -> io::Result<Vec<SocketCounter>> {
    // SAFETY: socket takes scalar arguments and returns a new owned descriptor.
    let raw = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
            libc::NETLINK_SOCK_DIAG,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: raw is a newly created valid descriptor, owned only here.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let timeout = libc::timeval {
        tv_sec: 1,
        tv_usec: 0,
    };
    // SAFETY: valid descriptor and pointer to a timeval with the specified size.
    if unsafe {
        libc::setsockopt(
            fd.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&timeout as *const libc::timeval).cast(),
            std::mem::size_of_val(&timeout) as libc::socklen_t,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut request = [0u8; 72]; // nlmsghdr (16) + inet_diag_req_v2 (56)
    request[0..4].copy_from_slice(&72u32.to_ne_bytes());
    request[4..6].copy_from_slice(&20u16.to_ne_bytes()); // SOCK_DIAG_BY_FAMILY
    request[6..8].copy_from_slice(&0x301u16.to_ne_bytes()); // REQUEST | DUMP
    request[8..12].copy_from_slice(&1u32.to_ne_bytes());
    request[16] = family;
    request[17] = libc::IPPROTO_TCP as u8;
    request[18] = 2; // INET_DIAG_INFO
    request[20..24].copy_from_slice(&2u32.to_ne_bytes()); // TCP_ESTABLISHED
                                                          // SAFETY: zero is a valid initial representation for sockaddr_nl.
    let mut kernel: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    kernel.nl_family = libc::AF_NETLINK as u16;
    // SAFETY: all buffer and sockaddr pointers are valid for their lengths.
    let sent = unsafe {
        libc::sendto(
            fd.as_raw_fd(),
            request.as_ptr().cast(),
            request.len(),
            0,
            (&kernel as *const libc::sockaddr_nl).cast(),
            std::mem::size_of_val(&kernel) as libc::socklen_t,
        )
    };
    if sent != request.len() as isize {
        return Err(io::Error::last_os_error());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut output = Vec::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        if Instant::now() > deadline || output.len() > 100_000 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "socket snapshot limit exceeded",
            ));
        }
        // MSG_TRUNC reports oversized datagrams so partial dumps cannot become valid samples.
        // SAFETY: buffer is writable for its entire allocated length.
        let count = unsafe {
            libc::recv(
                fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::MSG_TRUNC,
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        if count == 0 || count as usize > buffer.len() {
            return Err(invalid());
        }
        let mut messages = &buffer[..count as usize];
        while messages.len() >= 16 {
            let len = u32_at(messages, 0)? as usize;
            if len < 16 || len > messages.len() {
                return Err(invalid());
            }
            let kind = u16::from_ne_bytes([messages[4], messages[5]]);
            let flags = u16::from_ne_bytes([messages[6], messages[7]]);
            if flags & 0x10 != 0 {
                return Err(io::Error::other("socket dump interrupted"));
            }
            if u32_at(messages, 8)? != 1 {
                return Err(invalid());
            }
            let body = &messages[16..len];
            match kind {
                3 => {
                    if body.len() >= 4 && u32_at(body, 0)? != 0 {
                        return Err(io::Error::other("socket dump failed"));
                    }
                    return Ok(output);
                }
                2 => {
                    let code = u32_at(body, 0)? as i32;
                    if code != 0 {
                        return Err(io::Error::from_raw_os_error(-code));
                    }
                }
                20 => {
                    if let Some(socket) = parse_socket(body)? {
                        output.push(socket);
                    }
                }
                _ => return Err(invalid()),
            }
            let next = aligned(len);
            if next > messages.len() {
                return Err(invalid());
            }
            messages = &messages[next..];
        }
        if !messages.is_empty() {
            return Err(invalid());
        }
    }
}

/// Per-interface byte counters in the daemon's network namespace; loopback is excluded.
pub fn interfaces() -> io::Result<Vec<(String, u32, u64, u64)>> {
    let text = std::fs::read_to_string("/proc/net/dev")?;
    let mut result = Vec::new();
    for line in text.lines().skip(2) {
        let (name, values) = line.split_once(':').ok_or_else(invalid)?;
        let name = name.trim();
        if name == "lo" {
            continue;
        }
        let values: Vec<_> = values.split_whitespace().collect();
        let rx = values
            .first()
            .ok_or_else(invalid)?
            .parse()
            .map_err(|_| invalid())?;
        let tx = values
            .get(8)
            .ok_or_else(invalid)?
            .parse()
            .map_err(|_| invalid())?;
        let index = std::fs::read_to_string(format!("/sys/class/net/{name}/ifindex"))?
            .trim()
            .parse()
            .map_err(|_| invalid())?;
        result.push((name.to_string(), index, rx, tx));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    #[test]
    fn reads_actual_tcp_bytes_and_process_ownership() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut sender = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut receiver, _) = listener.accept().unwrap();
        sender.write_all(&vec![42; 32768]).unwrap();
        let mut payload = vec![0; 32768];
        receiver.read_exact(&mut payload).unwrap();
        receiver.write_all(&vec![21; 8192]).unwrap();
        sender.read_exact(&mut vec![0; 8192]).unwrap();
        let inodes = process_sockets(std::process::id()).unwrap();
        let snapshot = snapshot().unwrap();
        let socket = snapshot
            .iter()
            .find(|s| s.port == listener.local_addr().unwrap().port() && inodes.contains(&s.inode))
            .unwrap();
        assert!(socket.received >= 8192);
        assert!(socket.sent >= 32768);
        assert_eq!(socket.peer, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert!(!process_identity(std::process::id()).unwrap().is_empty());
    }

    #[test]
    fn rejects_short_and_malformed_kernel_attributes() {
        assert!(parse_socket(&[0; 12]).is_err());
        let mut data = vec![0; 80];
        data[0] = libc::AF_INET as u8;
        data[1] = 1;
        data[72..74].copy_from_slice(&100u16.to_ne_bytes());
        assert!(parse_socket(&data).is_err());
    }
}
