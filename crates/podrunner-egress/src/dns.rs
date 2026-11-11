//! The resolver of podrunner (T-076): `pod-dns` in its order, an IP
//! literal, the pins, the system resolver, the addresses that carried a
//! session, DNS over HTTPS to resolvers by their address, the old cache.
//! A host whose own resolver fails keeps its egress and its mint.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use pod_core::Resolve;
use pod_proxy::{Dialer, ProxyChoice};

/// The time of one lookup.
const LOOKUP: Duration = Duration::from_secs(10);

/// The resolver of the process: the pins `pins` (a name and one of its
/// addresses each), and DNS over HTTPS when `doh` is true. The dial of DNS
/// over HTTPS uses the pins and the system only, never itself.
pub fn resolver(doh: bool, pins: Vec<(String, IpAddr)>) -> Arc<dyn Resolve> {
    let version = env!("CARGO_PKG_VERSION");
    let settings = pod_dns::Settings { pins, user_agent: format!("podrunner/{version}"), ..Default::default() };
    let plain: Arc<dyn Resolve> = Arc::new(pod_dns::Resolver::new(settings.clone()));
    if !doh {
        return plain;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let context = Arc::new(pod_tls::Context::new(provider, pod_tls::Settings::new(pod_tls::Words::new("podrunner"))));
    let dial = Arc::new(Dialer::new(pod_proxy::Settings::new("podrunner", ProxyChoice::Direct), plain));
    Arc::new(pod_dns::Resolver::new(settings).with_doh(context, dial))
}

/// A blocking lookup through `resolve`, for a caller with no runtime (the
/// dial of a remote mint): the addresses of `host` at `port`, or why not.
pub fn lookup(resolve: Arc<dyn Resolve>) -> impl Fn(&str, u16) -> Result<Vec<SocketAddr>, String> + Send + Sync {
    move |host: &str, port: u16| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("the runtime of a lookup: {}", e.kind()))?;
        rt.block_on(async {
            let deadline = tokio::time::Instant::now() + LOOKUP;
            resolve.resolve_from(host, port, deadline, None).await.map(|f| f.addrs).map_err(|f| f.to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name that the system cannot resolve answers from its pin: the
    /// resolver is `pod-dns`, not the system alone.
    #[test]
    fn a_pinned_name_resolves_where_the_system_does_not() {
        let pinned: IpAddr = "192.0.2.7".parse().unwrap();
        let find = lookup(resolver(false, vec![("mint.podrunner.invalid".into(), pinned)]));
        assert_eq!(find("mint.podrunner.invalid", 8443).unwrap(), vec![SocketAddr::new(pinned, 8443)]);
        let e = find("other.podrunner.invalid", 8443).unwrap_err();
        assert!(e.contains("system"), "the failure names each source: {e}");
        // An IP literal needs no resolver.
        assert_eq!(find("192.0.2.9", 1).unwrap(), vec!["192.0.2.9:1".parse().unwrap()]);
    }
}
