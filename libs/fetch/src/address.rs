use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use lodestone_core::FetchError;
use tokio::net::lookup_host;
use url::{Host, Url};

pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 198 && (b == 18 || b == 19)))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_v4(mapped);
    }
    let segments = ip.segments();
    let first = segments[0];
    let second = segments[1];
    if first == 0x0064 && second == 0xff9b && segments[2..6] == [0, 0, 0, 0] {
        let [.., high, low] = segments;
        let embedded = Ipv4Addr::from((u32::from(high) << 16) | u32::from(low));
        return is_public_v4(embedded);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first == 0x2001 && second == 0x0db8))
}

pub(crate) async fn ensure_public(url: &Url) -> Result<(), FetchError> {
    let port = url.port_or_known_default().unwrap_or(80);
    match url.host() {
        Some(Host::Ipv4(ip)) => require(is_public_ip(IpAddr::V4(ip))),
        Some(Host::Ipv6(ip)) => require(is_public_ip(IpAddr::V6(ip))),
        Some(Host::Domain(domain)) => {
            let addresses = lookup_host((domain, port))
                .await
                .map_err(|_| FetchError::Network("dns lookup failed".to_owned()))?;
            let mut seen = false;
            for address in addresses {
                seen = true;
                if !is_public_ip(address.ip()) {
                    return Err(FetchError::NonPublicTarget);
                }
            }
            require(seen)
        }
        None => Err(FetchError::InvalidUrl),
    }
}

fn require(public: bool) -> Result<(), FetchError> {
    if public {
        Ok(())
    } else {
        Err(FetchError::NonPublicTarget)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(raw: &str) -> bool {
        is_public_ip(raw.parse().unwrap())
    }

    #[test]
    fn rejects_non_public_v4() {
        for raw in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "0.1.2.3",
            "255.255.255.255",
            "192.0.2.1",
            "198.51.100.1",
            "203.0.113.1",
            "100.64.0.1",
            "100.127.255.255",
            "224.0.0.1",
            "240.0.0.1",
            "198.18.0.1",
        ] {
            assert!(!v4(raw), "{raw}");
        }
    }

    #[test]
    fn accepts_public_v4() {
        for raw in [
            "8.8.8.8",
            "1.1.1.1",
            "100.63.255.255",
            "100.128.0.1",
            "172.32.0.1",
        ] {
            assert!(v4(raw), "{raw}");
        }
    }

    #[test]
    fn rejects_non_public_v6() {
        for raw in [
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "2001:db8::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!v4(raw), "{raw}");
        }
    }

    #[test]
    fn accepts_public_v6() {
        for raw in [
            "2606:4700:4700::1111",
            "2001:4860:4860::8888",
            "::ffff:8.8.8.8",
        ] {
            assert!(v4(raw), "{raw}");
        }
    }

    #[tokio::test]
    async fn ip_literals_are_classified_without_lookup() {
        let private = Url::parse("http://192.168.0.1/").unwrap();
        assert_eq!(
            ensure_public(&private).await,
            Err(FetchError::NonPublicTarget)
        );
        let v6 = Url::parse("http://[::1]/").unwrap();
        assert_eq!(ensure_public(&v6).await, Err(FetchError::NonPublicTarget));
        let public = Url::parse("http://8.8.8.8/").unwrap();
        assert_eq!(ensure_public(&public).await, Ok(()));
    }

    #[test]
    fn nat64_follows_the_embedded_ipv4() {
        assert!(!is_public_ip("64:ff9b::a00:1".parse().unwrap()));
        assert!(!is_public_ip("64:ff9b::7f00:1".parse().unwrap()));
        assert!(is_public_ip("64:ff9b::808:808".parse().unwrap()));
    }
}
