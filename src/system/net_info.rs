//! Link details for network interfaces: kind, state, speed and addresses.

use std::collections::HashMap;
use std::ffi::CStr;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InterfaceInfo {
    pub wireless: bool,
    pub connected: bool,
    /// Link speed in Mbit/s. Wi-Fi drivers usually don't report one.
    pub speed_mbps: Option<u32>,
    pub mac: Option<String>,
    pub ipv4: Vec<Ipv4Addr>,
    pub ipv6: Vec<Ipv6Addr>,
}

impl InterfaceInfo {
    /// Reads the sysfs files for `name`. Addresses come from [`read_addresses`].
    pub fn read(name: &str, addresses: &mut HashMap<String, (Vec<Ipv4Addr>, Vec<Ipv6Addr>)>) -> Self {
        let dir = Path::new("/sys/class/net").join(name);
        let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok().map(|text| text.trim().to_string());
        let (ipv4, ipv6) = addresses.remove(name).unwrap_or_default();
        Self {
            wireless: dir.join("wireless").exists(),
            connected: read("operstate").is_some_and(|state| state == "up"),
            // `speed` is -1, or fails to read, when there's no link or the driver doesn't know.
            speed_mbps: read("speed").and_then(|speed| speed.parse::<i64>().ok()).filter(|&speed| speed > 0).map(|speed| speed as u32),
            mac: read("address").filter(|mac| !mac.is_empty() && mac != "00:00:00:00:00:00"),
            ipv4,
            ipv6,
        }
    }
}

/// IPv4 and IPv6 addresses for every interface, by name.
pub fn read_addresses() -> HashMap<String, (Vec<Ipv4Addr>, Vec<Ipv6Addr>)> {
    let mut addresses: HashMap<String, (Vec<Ipv4Addr>, Vec<Ipv6Addr>)> = HashMap::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs fills `list` with a linked list we free below.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        return addresses;
    }
    let mut node = list;
    while !node.is_null() {
        // SAFETY: `node` is a live entry in the list getifaddrs returned.
        let entry = unsafe { &*node };
        node = entry.ifa_next;
        if entry.ifa_addr.is_null() || entry.ifa_name.is_null() {
            continue;
        }
        // SAFETY: ifa_name is a NUL-terminated string owned by the list.
        let name = unsafe { CStr::from_ptr(entry.ifa_name) }.to_string_lossy().into_owned();
        // SAFETY: ifa_addr is non-null, and sa_family says which sockaddr it is.
        match i32::from(unsafe { (*entry.ifa_addr).sa_family }) {
            libc::AF_INET => {
                let addr = unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in) };
                addresses.entry(name).or_default().0.push(Ipv4Addr::from(u32::from_be(addr.sin_addr.s_addr)));
            }
            libc::AF_INET6 => {
                let addr = unsafe { &*(entry.ifa_addr as *const libc::sockaddr_in6) };
                addresses.entry(name).or_default().1.push(Ipv6Addr::from(addr.sin6_addr.s6_addr));
            }
            _ => {}
        }
    }
    // SAFETY: `list` came from getifaddrs and is freed once.
    unsafe { libc::freeifaddrs(list) };
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_has_an_address() {
        let addresses = read_addresses();
        let (ipv4, _) = addresses.get("lo").expect("every Linux box has a loopback interface");
        assert!(ipv4.contains(&Ipv4Addr::LOCALHOST));
    }
}
