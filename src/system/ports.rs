//! Listening ports, from `/proc/net/{tcp,tcp6,udp,udp6}`.
//!
//! The kernel lists each socket with its owner's UID and an inode number. The
//! processes holding it are found by matching that inode against the links in
//! `/proc/<pid>/fd`. Other users' fd folders need root, so those owners stay
//! unknown until an admin lookup through `pkexec ss`.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::control;

/// TCP's LISTEN state in `/proc/net/tcp`.
const TCP_LISTEN: &str = "0A";
/// An unconnected UDP socket shows as CLOSE (`07`).
const UDP_UNCONNECTED: &str = "07";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Protocol {
    Tcp,
    Udp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Socket {
    pub protocol: Protocol,
    pub address: SocketAddr,
    pub uid: u32,
    pub inode: u64,
}

impl Socket {
    /// `TCP`, `TCP6`, `UDP` or `UDP6`, like netstat.
    pub fn protocol_label(&self) -> &'static str {
        match (self.protocol, self.address.is_ipv6()) {
            (Protocol::Tcp, false) => "TCP",
            (Protocol::Tcp, true) => "TCP6",
            (Protocol::Udp, false) => "UDP",
            (Protocol::Udp, true) => "UDP6",
        }
    }

    /// Who can connect, going by the address it's bound to.
    pub fn reach(&self) -> &'static str {
        match self.address.ip() {
            ip if ip.is_loopback() => "This computer only",
            ip if ip.is_unspecified() => "Every network",
            ip if ip.is_multicast() => "Multicast group",
            _ => "One address",
        }
    }
}

/// Every listening TCP socket and unconnected UDP socket.
pub fn read() -> Vec<Socket> {
    let tables = [("tcp", Protocol::Tcp), ("tcp6", Protocol::Tcp), ("udp", Protocol::Udp), ("udp6", Protocol::Udp)];
    tables
        .into_iter()
        .flat_map(|(file, protocol)| parse(&std::fs::read_to_string(format!("/proc/net/{file}")).unwrap_or_default(), protocol))
        .collect()
}

fn parse(text: &str, protocol: Protocol) -> Vec<Socket> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (local, remote, state) = (fields.get(1)?, fields.get(2)?, *fields.get(3)?);
            let listening = match protocol {
                Protocol::Tcp => state == TCP_LISTEN,
                Protocol::Udp => state == UDP_UNCONNECTED && parse_address(remote)?.port() == 0,
            };
            if !listening {
                return None;
            }
            let inode = fields.get(9)?.parse().ok().filter(|&inode| inode != 0)?;
            Some(Socket { protocol, address: parse_address(local)?, uid: fields.get(7)?.parse().ok()?, inode })
        })
        .collect()
}

/// `0100007F:0277` → `127.0.0.1:631`. The kernel prints each 32-bit word of
/// the address as a native-endian number, and the port as a plain one.
fn parse_address(text: &str) -> Option<SocketAddr> {
    let (ip, port) = text.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let word = |chunk: &str| u32::from_str_radix(chunk, 16).ok().map(u32::to_ne_bytes);
    let ip = match ip.len() {
        8 => IpAddr::V4(Ipv4Addr::from(word(ip)?)),
        32 => {
            let mut bytes = [0u8; 16];
            for (ix, chunk) in bytes.chunks_mut(4).enumerate() {
                chunk.copy_from_slice(&word(ip.get(ix * 8..ix * 8 + 8)?)?);
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

/// Which PIDs hold each of `wanted`, from the fd folders we're allowed to
/// read. Stops early once every inode is found.
pub fn owners(wanted: &HashSet<u64>) -> HashMap<u64, Vec<u32>> {
    let mut owners: HashMap<u64, Vec<u32>> = HashMap::new();
    if wanted.is_empty() {
        return owners;
    }
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return owners;
    };
    let pids = entries.filter_map(Result::ok).filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok());
    for pid in pids {
        // Fails quickly for other users' processes.
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        for fd in fds.filter_map(Result::ok) {
            let Some(inode) = std::fs::read_link(fd.path()).ok().and_then(|link| socket_inode(link.to_str()?)) else {
                continue;
            };
            if wanted.contains(&inode) {
                let pids = owners.entry(inode).or_default();
                if !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
    }
    owners
}

/// `socket:[8989]` → `8989`.
fn socket_inode(link: &str) -> Option<u64> {
    link.strip_prefix("socket:[")?.strip_suffix(']')?.parse().ok()
}

/// Every listening socket's owners, read as root with `ss`. Blocks until the
/// password prompt is answered, so call it off the UI thread.
pub fn owners_as_admin() -> Result<HashMap<u64, Vec<u32>>, String> {
    let command = ["/usr/bin/ss", "--tcp", "--udp", "--listening", "--numeric", "--processes", "--extended", "--no-header"];
    control::run_pkexec(&command.map(String::from)).map(|output| parse_ss(&output))
}

/// Pulls `ino:` and every `pid=` out of each `ss -tulpneH` line.
fn parse_ss(text: &str) -> HashMap<u64, Vec<u32>> {
    text.lines()
        .filter_map(|line| {
            let inode = line.split_whitespace().find_map(|token| token.strip_prefix("ino:")?.parse::<u64>().ok())?;
            let mut pids: Vec<u32> = line
                .split("pid=")
                .skip(1)
                .filter_map(|rest| rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok())
                .collect();
            pids.dedup();
            (!pids.is_empty()).then_some((inode, pids))
        })
        .collect()
}

/// The PID to show and act on. Socket-activated services share their socket
/// with systemd (PID 1), so prefer anything else, then the oldest (lowest)
/// PID, which is the parent in a forked server like nginx.
pub fn main_pid(pids: &[u32]) -> Option<u32> {
    pids.iter().copied().filter(|&pid| pid != 1).min().or_else(|| pids.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 8989 1 0000000000000000 100 0 0 10 0
   1: 0100007F:A1B2 0100007F:0277 01 00000000:00000000 00:00000000 00000000  1000        0 47453 1 0000000000000000 20 4 30 10 -1
   2: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 51234 1 0000000000000000 100 0 0 10 0";

    const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000001000000:0277 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 8990 1 0000000000000000 100 0 0 10 0";

    const UDP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops
 3797: FB0000E0:14E9 00000000:0000 07 00000000:00000000 00:00000000 00000000  1000        0 36086993 2 0000000000000000 0
 3798: 0F02000A:D431 08080808:0035 01 00000000:00000000 00:00000000 00000000  1000        0 36086994 2 0000000000000000 0";

    #[test]
    fn keeps_only_listening_tcp() {
        let sockets = parse(TCP, Protocol::Tcp);
        assert_eq!(sockets.len(), 2);
        assert_eq!(sockets[0].address, "127.0.0.1:631".parse().unwrap());
        assert_eq!((sockets[0].uid, sockets[0].inode), (0, 8989));
        assert_eq!(sockets[1].address, "0.0.0.0:8080".parse().unwrap());
        assert_eq!(sockets[1].uid, 1000);
    }

    #[test]
    fn reads_ipv6_words() {
        let sockets = parse(TCP6, Protocol::Tcp);
        assert_eq!(sockets[0].address, "[::1]:631".parse().unwrap());
        assert_eq!(sockets[0].protocol_label(), "TCP6");
        assert_eq!(sockets[0].reach(), "This computer only");
    }

    #[test]
    fn keeps_only_unconnected_udp() {
        let sockets = parse(UDP, Protocol::Udp);
        assert_eq!(sockets.len(), 1);
        assert_eq!(sockets[0].address, "224.0.0.251:5353".parse().unwrap());
        assert_eq!(sockets[0].reach(), "Multicast group");
    }

    #[test]
    fn rejects_bad_addresses() {
        assert_eq!(parse_address("0100007F"), None);
        assert_eq!(parse_address("01007F:0277"), None);
        assert_eq!(parse_address("0100007F:ZZZZ"), None);
    }

    #[test]
    fn parses_socket_links() {
        assert_eq!(socket_inode("socket:[8989]"), Some(8989));
        assert_eq!(socket_inode("pipe:[8989]"), None);
        assert_eq!(socket_inode("/dev/null"), None);
    }

    #[test]
    fn parses_ss_owners() {
        let text = "udp UNCONN 0 0 224.0.0.251:5353 0.0.0.0:* users:((\"chrome\",pid=2424915,fd=115)) uid:1000 ino:36086993 sk:1006 <->
tcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"nginx\",pid=901,fd=6),(\"nginx\",pid=900,fd=6)) ino:4242 sk:2 <->
udp UNCONN 0 0 0.0.0.0:5355 0.0.0.0:* uid:974 ino:10675 sk:100a <->";
        let owners = parse_ss(text);
        assert_eq!(owners.len(), 2);
        assert_eq!(owners[&36086993], vec![2424915]);
        assert_eq!(owners[&4242], vec![901, 900]);
    }

    #[test]
    fn picks_the_main_pid() {
        assert_eq!(main_pid(&[1, 812]), Some(812));
        assert_eq!(main_pid(&[901, 900]), Some(900));
        assert_eq!(main_pid(&[1]), Some(1));
        assert_eq!(main_pid(&[]), None);
    }

    #[test]
    fn finds_a_socket_we_own() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let socket = read().into_iter().find(|socket| socket.address == address).expect("our listener shows up");
        assert_eq!(socket.protocol, Protocol::Tcp);
        let owners = owners(&HashSet::from([socket.inode]));
        assert_eq!(owners[&socket.inode], vec![std::process::id()]);
    }
}
