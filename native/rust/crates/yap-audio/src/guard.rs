//! Keeps the playback HTTP clients away from the machine's own network.
//!
//! A radio station's `streamUrl` (and a MyInstants clip) is a stranger's
//! URL, while the client runs a media relay on `127.0.0.1`. So a redirect
//! may only lead to another public https host, and a public-looking name
//! must not resolve to a loopback, private or link-local address.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::redirect::{Action, Attempt, Policy};
use reqwest::Url;
use yap_protocol::voice_app::sanitize_public_https_url;

/// Redirects a station or clip fetch follows before giving up.
const MAX_REDIRECTS: usize = 5;

/// What the redirect policy does with one hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hop {
    Follow,
    /// The response (a 3xx) is returned as it is.
    Stop,
    TooMany,
}

/// Whether `ip` is an address a stranger's URL may reach: not unspecified,
/// loopback, private, link-local, shared (CGNAT), multicast or broadcast.
pub(crate) fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => is_public_v6(ip),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [first, second, ..] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || first == 0
        // 100.64.0.0/10, carrier-grade NAT.
        || (first == 100 && (64..=127).contains(&second)))
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_v4(mapped);
    }
    let first = ip.segments()[0];
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        // fc00::/7, unique local.
        || (first & 0xfe00) == 0xfc00
        // fe80::/10, link-local.
        || (first & 0xffc0) == 0xfe80)
}

/// The public addresses among `addrs`.
pub(crate) fn filter_public(addrs: impl IntoIterator<Item = SocketAddr>) -> Vec<SocketAddr> {
    addrs
        .into_iter()
        .filter(|addr| is_public_ip(addr.ip()))
        .collect()
}

/// A redirect target is followed only if it is https and its host passes
/// the room's own radio-URL rule (`sanitize_public_https_url`), and, for an
/// IP literal, the address check above.
pub(crate) fn redirect_target_allowed(url: &Url) -> bool {
    if sanitize_public_https_url(url.as_str()).is_none() {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => is_public_v4(ip),
        Some(url::Host::Ipv6(ip)) => is_public_v6(ip),
        Some(url::Host::Domain(_)) => true,
        None => false,
    }
}

/// The decision for a redirect to `target` after `previous_hops` hops.
pub(crate) fn decide_hop(target: &Url, previous_hops: usize) -> Hop {
    if previous_hops >= MAX_REDIRECTS {
        Hop::TooMany
    } else if redirect_target_allowed(target) {
        Hop::Follow
    } else {
        Hop::Stop
    }
}

fn policy_decision(attempt: Attempt<'_>) -> Action {
    match decide_hop(attempt.url(), attempt.previous().len()) {
        Hop::Follow => attempt.follow(),
        Hop::Stop => attempt.stop(),
        Hop::TooMany => attempt.error("too many redirects"),
    }
}

/// The redirect policy for a stranger's URL.
pub(crate) fn public_redirect_policy() -> Policy {
    Policy::custom(policy_decision)
}

/// A DNS resolver that drops non-public addresses and fails when none
/// remain.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let kept = filter_public(resolved);
            if kept.is_empty() {
                return Err(format!("{host} does not resolve to a public address").into());
            }
            Ok(Box::new(kept.into_iter()) as Addrs)
        })
    }
}

/// A client for a stranger's URL: public addresses only, redirects only to
/// public https hosts. Build it inside a tokio runtime context.
pub(crate) fn public_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .dns_resolver(PublicResolver)
        .redirect(public_redirect_policy())
}

/// [`public_client_builder`] built (30 s per request), for the host app's
/// soundboard fetches from a host other than the API.
pub fn public_http_client() -> reqwest::Result<reqwest::Client> {
    public_client_builder()
        .timeout(Duration::from_secs(30))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> IpAddr {
        text.parse().expect("ip")
    }

    fn url(text: &str) -> Url {
        Url::parse(text).expect("url")
    }

    fn addr(text: &str) -> SocketAddr {
        text.parse().expect("addr")
    }

    #[test]
    fn private_addresses_should_not_be_public() {
        for bad in [
            "127.0.0.1",
            "127.8.8.8",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "febf::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad}");
        }
    }

    #[test]
    fn public_addresses_should_pass() {
        for good in [
            "8.8.8.8",
            "172.15.0.1",
            "172.32.0.1",
            "192.169.0.1",
            "100.63.0.1",
            "2606:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip(good)), "{good}");
        }
    }

    #[test]
    fn the_filter_should_keep_only_public_addresses() {
        let kept = filter_public([
            addr("127.0.0.1:0"),
            addr("8.8.8.8:0"),
            addr("[::1]:0"),
            addr("10.0.0.5:0"),
            addr("1.1.1.1:0"),
        ]);
        assert_eq!(kept, vec![addr("8.8.8.8:0"), addr("1.1.1.1:0")]);
        assert!(filter_public([addr("127.0.0.1:0")]).is_empty());
    }

    #[test]
    fn a_redirect_should_follow_only_to_a_public_https_host() {
        assert_eq!(
            decide_hop(&url("https://cdn.example.org/live.mp3"), 0),
            Hop::Follow
        );
        for bad in [
            "http://cdn.example.org/live.mp3",
            "https://localhost/x",
            "https://radio.local/x",
            "https://127.0.0.1/x",
            "https://10.0.0.1/x",
            "https://192.168.0.9/x",
            "https://169.254.169.254/x",
            "https://100.64.0.1/x",
            "https://[::1]/x",
            "https://user:pass@cdn.example.org/x",
            "ftp://cdn.example.org/x",
        ] {
            assert_eq!(decide_hop(&url(bad), 0), Hop::Stop, "{bad}");
        }
    }

    #[test]
    fn too_many_redirects_should_error() {
        let target = url("https://cdn.example.org/a");
        assert_eq!(decide_hop(&target, MAX_REDIRECTS - 1), Hop::Follow);
        assert_eq!(decide_hop(&target, MAX_REDIRECTS), Hop::TooMany);
    }

    #[test]
    fn a_name_for_a_loopback_address_should_be_refused() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let name: Name = "localhost".parse().expect("name");
        let result = runtime.block_on(PublicResolver.resolve(name));
        assert!(result.is_err());
    }
}
