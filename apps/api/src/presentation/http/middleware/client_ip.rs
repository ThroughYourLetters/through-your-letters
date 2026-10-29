use super::super::{errors::AppError, state::AppState};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use sqlx::types::ipnetwork::IpNetwork;
use std::net::{IpAddr, SocketAddr};

/// A validated, canonical address resolved from the transport and trusted proxy chain.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

/// Render's public web ingress always passes through Cloudflare, which replaces
/// CF-Connecting-IP. Unlike X-Forwarded-For, it contains no caller-supplied prefix.
/// The platform's server-side environment enables this path, never a request header.
fn resolve_render(peer: IpAddr, headers: &HeaderMap) -> Result<ClientIp, AppError> {
    let mut values = headers.get_all("cf-connecting-ip").iter();
    let Some(value) = values.next() else {
        // Health probes and internal requests need not traverse the public ingress.
        // Never fall back to a potentially caller-supplied forwarded chain.
        return Ok(ClientIp(peer.to_canonical()));
    };
    if values.next().is_some() {
        return Err(AppError::BadRequest("Invalid client address".into()));
    }
    let ip = value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<IpAddr>().ok())
        .map(|ip| ip.to_canonical())
        .filter(|ip| {
            !ip.is_unspecified() && !ip.is_multicast() && *ip != IpAddr::from([255, 255, 255, 255])
        })
        .ok_or_else(|| AppError::BadRequest("Invalid client address".into()))?;
    Ok(ClientIp(ip))
}

/// Forwarded headers are considered only when the immediate peer is trusted.
/// Walk from the right, stopping at the first untrusted address.
fn resolve(
    peer: IpAddr,
    headers: &HeaderMap,
    trusted: &[IpNetwork],
    max_hops: usize,
) -> Result<ClientIp, AppError> {
    let mut client = peer.to_canonical();
    let is_trusted = |ip| trusted.iter().any(|network| network.contains(ip));
    if !is_trusted(client) {
        return Ok(ClientIp(client));
    }
    let mut chain = Vec::new();
    for value in headers.get_all("x-forwarded-for") {
        let text = value
            .to_str()
            .map_err(|_| AppError::BadRequest("Invalid forwarded address".into()))?;
        for part in text.split(',') {
            if chain.len() == max_hops {
                return Err(AppError::BadRequest(
                    "Forwarded address chain is too long".into(),
                ));
            }
            let ip = part
                .trim()
                .parse::<IpAddr>()
                .map_err(|_| AppError::BadRequest("Invalid forwarded address".into()))?
                .to_canonical();
            if ip.is_unspecified() || ip.is_multicast() || ip == IpAddr::from([255, 255, 255, 255])
            {
                return Err(AppError::BadRequest("Invalid forwarded address".into()));
            }
            chain.push(ip);
        }
    }
    for address in chain.into_iter().rev() {
        if !is_trusted(client) {
            break;
        }
        client = address;
    }
    Ok(ClientIp(client))
}

pub async fn resolve_client_ip(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .ok_or_else(|| AppError::Internal("Transport peer address is unavailable".into()))?
        .0
        .ip();
    let client = if state.config.render_ingress {
        resolve_render(peer, request.headers())?
    } else {
        resolve(
            peer,
            request.headers(),
            &state.config.trusted_proxy_cidrs,
            state.config.limits.security.max_forwarded_hops,
        )?
    };
    request.extensions_mut().insert(client);
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests;
