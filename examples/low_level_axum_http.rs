use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::{routing::get, Router};
use axum_server::bind;
use clap::Parser;
use http::{header, HeaderValue, StatusCode};
use rustls_acme::caches::DirCache;
use rustls_acme::tower::TowerHttp01ChallengeService;
use rustls_acme::UseChallenge::Http01;
use rustls_acme::{AcmeConfig, ResolvesServerCertAcme};
use std::net::{Ipv6Addr, SocketAddr};
use std::os::macos::raw::stat;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_stream::StreamExt;

#[derive(Parser, Debug)]
struct Args {
    /// Domains
    #[clap(short, required = true)]
    domains: Vec<String>,

    /// Contact info
    #[clap(short)]
    email: Vec<String>,

    /// Cache directory
    #[clap(short, parse(from_os_str))]
    cache: Option<PathBuf>,

    /// Use Let's Encrypt production environment
    /// (see https://letsencrypt.org/docs/staging-environment/)
    #[clap(long)]
    prod: bool,

    #[clap(short, long, default_value = "443")]
    port: u16,
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let args = Args::parse();

    let mut state = AcmeConfig::new(args.domains)
        .contact(args.email.iter().map(|e| format!("mailto:{}", e)))
        .cache_option(args.cache.clone().map(DirCache::new))
        .directory_lets_encrypt(args.prod)
        .challenge_type(Http01)
        .state();
    let acceptor = state.axum_acceptor(state.default_rustls_config());
    let tower_service: TowerHttp01ChallengeService = state.http01_challenge_tower_service();
    let http_challenge_app = Router::new().route_service("/.well-known/acme-challenge/{challenge_token}", tower_service);
    tokio::spawn(challenge_http_app(http_challenge_app));

    tokio::spawn(async move {
        loop {
            match state.next().await.unwrap() {
                Ok(ok) => log::info!("event: {:?}", ok),
                Err(err) => log::error!("error: {:?}", err),
            }
        }
    });

    let app = Router::new().route("/", get(|| async { "Hello Tls!" }));
    let addr = SocketAddr::from((Ipv6Addr::UNSPECIFIED, args.port));
    bind(addr).acceptor(acceptor).serve(app.into_make_service()).await.unwrap();
}

async fn challenge_http_app(http_challenge_app: Router) {
    let listener = tokio::net::TcpListener::bind((Ipv6Addr::UNSPECIFIED, 80)).await.unwrap();
    axum::serve(listener, http_challenge_app.into_make_service()).await.unwrap();
}
