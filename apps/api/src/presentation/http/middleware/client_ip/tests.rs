use super::*;

#[test]
fn render_ingress_uses_one_edge_address_and_ignores_forwarded_prefixes() {
    let peer = "10.0.0.1".parse().unwrap();
    for value in ["198.51.100.4", "::ffff:198.51.100.4", "2001:db8::4"] {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", value.parse().unwrap());
        headers.insert(
            "x-forwarded-for",
            "203.0.113.99, not-an-ip".parse().unwrap(),
        );
        headers.insert("true-client-ip", "203.0.113.98".parse().unwrap());
        headers.insert("x-real-ip", "203.0.113.97".parse().unwrap());
        assert_eq!(
            resolve_render(peer, &headers).unwrap().0,
            value.parse::<IpAddr>().unwrap().to_canonical()
        );
    }
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.99".parse().unwrap());
    assert_eq!(resolve_render(peer, &headers).unwrap().0, peer);
}

#[test]
fn render_ingress_rejects_ambiguous_or_invalid_edge_addresses() {
    for value in [
        "",
        "unknown",
        "198.51.100.4:1234",
        "[2001:db8::1]",
        "198.51.100.4, 203.0.113.1",
        "0.0.0.0",
        "224.0.0.1",
        "::",
        "255.255.255.255",
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", value.parse().unwrap());
        assert!(resolve_render("10.0.0.1".parse().unwrap(), &headers).is_err());
    }
    let mut headers = HeaderMap::new();
    headers.append("cf-connecting-ip", "198.51.100.4".parse().unwrap());
    headers.append("cf-connecting-ip", "203.0.113.1".parse().unwrap());
    assert!(resolve_render("10.0.0.1".parse().unwrap(), &headers).is_err());
}

fn address(
    peer: &str,
    values: &[&str],
    trusted: &[&str],
    max_hops: usize,
) -> Result<IpAddr, AppError> {
    let mut headers = HeaderMap::new();
    for value in values {
        headers.append("x-forwarded-for", value.parse().unwrap());
    }
    headers.insert("x-real-ip", "203.0.113.99".parse().unwrap());
    headers.insert("cf-connecting-ip", "203.0.113.98".parse().unwrap());
    headers.insert("forwarded", "for=203.0.113.97".parse().unwrap());
    let networks = trusted
        .iter()
        .map(|s| s.parse().unwrap())
        .collect::<Vec<_>>();
    resolve(peer.parse().unwrap(), &headers, &networks, max_hops).map(|ip| ip.0)
}

#[test]
fn untrusted_peers_cannot_choose_their_identity_through_headers() {
    for value in [
        "203.0.113.1",
        "127.0.0.1",
        "not an IP",
        "",
        "203.0.113.1, 203.0.113.2",
    ] {
        assert_eq!(
            address("198.51.100.4", &[value], &["10.0.0.1/32"], 16).unwrap(),
            "198.51.100.4".parse::<IpAddr>().unwrap()
        );
    }
}

#[test]
fn trusted_chains_stop_at_the_first_untrusted_hop_and_canonicalize_addresses() {
    for values in [
        &["203.0.113.99, 198.51.100.4, 10.0.0.2"][..],
        &["203.0.113.99", "198.51.100.4, 10.0.0.2"],
    ] {
        assert_eq!(
            address("10.0.0.1", values, &["10.0.0.1/32", "10.0.0.2/32"], 16).unwrap(),
            "198.51.100.4".parse::<IpAddr>().unwrap()
        );
    }
    assert_eq!(
        address("::ffff:198.51.100.4", &[], &[], 16).unwrap(),
        "198.51.100.4".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        address("10.0.0.1", &["::ffff:198.51.100.4"], &["10.0.0.1/32"], 16).unwrap(),
        "198.51.100.4".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        address("::1", &["2001:db8::1"], &["::1/128"], 16).unwrap(),
        "2001:db8::1".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        address("10.0.0.1", &[], &["10.0.0.1/32"], 16).unwrap(),
        "10.0.0.1".parse::<IpAddr>().unwrap()
    );
}

#[test]
fn malformed_or_excessive_trusted_headers_fail_without_falling_back() {
    for value in [
        "",
        "unknown",
        "198.51.100.4:1234",
        "[2001:db8::1]",
        "198.51.100.4,,10.0.0.2",
        "0.0.0.0",
        "224.0.0.1",
        "::",
        "255.255.255.255",
    ] {
        assert!(address("10.0.0.1", &[value], &["10.0.0.1/32"], 16).is_err());
    }
    assert!(address("10.0.0.1", &["198.51.100.4, 10.0.0.2"], &["10.0.0.1/32"], 1).is_err());
}
