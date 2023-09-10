use std::{
    io::{self, Read},
    net::SocketAddr,
    path::PathBuf,
};

use axum::{
    extract,
    headers::{Header, Host},
    http::{HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::IntoResponse,
    routing, Json, Router, TypedHeader,
};
use byteorder::{ReadBytesExt, LE};
use clap::Parser;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use server::{
    auth::Auth,
    index_schema,
    store::Store,
    types::{CrateName, SearchCratesQuery},
    HttpError, ToHttpError,
};
use tokio::fs;
use tracing::{error, info};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
struct Opts {
    #[clap(long, env)]
    addr: SocketAddr,
    #[clap(long, env)]
    objstore_endpoint: Option<String>,
    #[clap(long, env)]
    objstore: String,
    #[clap(long, env)]
    rules: PathBuf,
}

#[derive(Serialize, Clone)]
struct IndexConfig {
    dl: String,
    api: String,
    #[serde(rename = "auth-required")]
    auth_required: bool,
}

#[derive(Clone)]
struct State<S, A> {
    addr: SocketAddr,
    store: S,
    auth: A,
}

// configは認証の必要なし
async fn config<S, A>(
    TypedHeader(host): extract::TypedHeader<Host>,
    headers: HeaderMap,
    extract::State(state): extract::State<State<S, A>>,
) -> Result<Json<IndexConfig>, HttpError> {
    let host = if let Some(x_forwared_for) = headers.get("x-forwarded-host") {
        x_forwared_for
            .to_str()
            .http_error(StatusCode::BAD_GATEWAY)?
            .to_string()
    } else {
        host.hostname().to_string()
    };
    let proto = if let Some(x_forwarded_proto) = headers.get("x-forwarded-proto") {
        x_forwarded_proto
            .to_str()
            .http_error(StatusCode::BAD_GATEWAY)?
            .to_string()
    } else {
        "http".to_string()
    };
    let host = if host == "localhost" {
        format!("{host}:{}", state.addr.port())
    } else {
        host
    };
    Ok(Json(IndexConfig {
        dl: format!("{proto}://{host}/api/v1/crates"),
        api: format!("{proto}://{host}"),
        auth_required: true,
    }))
}

#[cfg(not(unix))]
async fn wait_shutdown() {
    tokio::signal::ctrl_c().await.expect("ctrl_c")
}

async fn get_index<S: Store, A: Auth>(
    state: &State<S, A>,
    name: &CrateName,
    token: &str,
) -> Result<String, HttpError> {
    state.auth.readable(token, name).await?;
    let index = state.store.get_index(name).await?;
    let mut response = String::new();
    for index in index {
        response.push_str(&serde_json::to_string(&index).unwrap());
    }
    Ok(response)
}

struct RawAuthorization(String);

impl Header for RawAuthorization {
    fn name() -> &'static axum::http::HeaderName {
        &axum::http::header::AUTHORIZATION
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, axum::headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i HeaderValue>,
    {
        let value = values.next().ok_or_else(axum::headers::Error::invalid)?;
        Ok(Self(
            value
                .to_str()
                .map_err(|_| axum::headers::Error::invalid())?
                .trim()
                .to_string(),
        ))
    }

    fn encode<E: Extend<HeaderValue>>(&self, values: &mut E) {
        let mut value = HeaderValue::from_str(&self.0).unwrap();
        value.set_sensitive(true);
        values.extend(std::iter::once(value))
    }
}

async fn get_index_len_1<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path(name): extract::Path<CrateName>,
) -> impl IntoResponse {
    get_index(&state, &name, &token).await
}

async fn get_index_len_2<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path(name): extract::Path<CrateName>,
) -> impl IntoResponse {
    get_index(&state, &name, &token).await
}

async fn get_index_len_3<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path((_, name)): extract::Path<(char, CrateName)>,
) -> impl IntoResponse {
    get_index(&state, &name, &token).await
}

async fn get_index_len_at_least_4<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path((_, _, name)): extract::Path<(String, String, CrateName)>,
) -> impl IntoResponse {
    get_index(&state, &name, &token).await
}

async fn publish_crate<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    mut stream: extract::BodyStream,
) -> Result<StatusCode, HttpError> {
    let mut full = Vec::new();
    while let Some(body) = stream.next().await {
        let body = body.http_error(StatusCode::BAD_REQUEST)?;
        full.append(&mut body.to_vec());
    }
    let mut full = io::Cursor::new(full);
    let index_len = full.read_u32::<LE>().http_error(StatusCode::BAD_REQUEST)?;
    let mut index = vec![0u8; index_len as usize];
    full.read_exact(&mut index)
        .http_error(StatusCode::BAD_REQUEST)?;
    let crate_len = full.read_u32::<LE>().http_error(StatusCode::BAD_REQUEST)?;
    let mut crate_archive = vec![0u8; crate_len as usize];
    full.read_exact(&mut crate_archive)
        .http_error(StatusCode::BAD_REQUEST)?;

    let index: index_schema::PostIndexRequest =
        serde_json::from_slice(&index).http_error(StatusCode::BAD_REQUEST)?;

    let name = index.name.parse().http_error(StatusCode::BAD_REQUEST)?;
    state.auth.writable(&token, &name).await?;
    state.store.put(&index, crate_archive).await?;

    info!(
        name = index.name,
        version = index.vers.to_string(),
        "publish"
    );

    Ok(StatusCode::OK)
}

#[derive(Serialize)]
struct YankResponse {
    ok: bool,
}

async fn yank_crate<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path((name, ver)): extract::Path<(CrateName, semver::Version)>,
) -> Result<impl IntoResponse, HttpError> {
    state.auth.writable(&token, &name).await?;
    state.store.set_yank(&name, ver, true).await?;
    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

async fn unyank_crate<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path((name, ver)): extract::Path<(CrateName, semver::Version)>,
) -> Result<impl IntoResponse, HttpError> {
    state.auth.writable(&token, &name).await?;
    state.store.set_yank(&name, ver, false).await?;
    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

async fn get_owners<S: Store, A: Auth + Clone>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path(name): extract::Path<CrateName>,
) -> Result<impl IntoResponse, HttpError> {
    state.auth.readable(&token, &name).await?;
    let owners = state.store.get_owners(&name).await?;
    let owners = owners
        .into_iter()
        .map(|owner| {
            let auth = state.auth.clone();
            let token = token.clone();
            async move { auth.as_registry_user(&token, &owner).await }
        })
        .collect::<Vec<_>>();
    let owners = futures_util::future::join_all(owners)
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    Ok((StatusCode::OK, Json(json!({ "users": owners }))))
}

#[derive(Deserialize)]
struct AddOwnerRequest {
    users: Vec<String>,
}

fn natural_human_names(names: &[String]) -> String {
    assert_ne!(names.len(), 0);
    if names.len() < 3 {
        names.join(" and ")
    } else {
        format!(
            "{} and {}",
            names[..names.len() - 1].join(", "),
            names.last().unwrap()
        )
    }
}

async fn add_owner<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path(name): extract::Path<CrateName>,
    Json(req): Json<AddOwnerRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let names = natural_human_names(&req.users);
    state.auth.writable(&token, &name).await?;
    state.store.add_owner(&name, req.users).await?;
    Ok((
        StatusCode::OK,
        Json(
            json!({"ok": true, "msg": format!("user {names} has been added to {}", name.original)}),
        ),
    ))
}

async fn delete_owner<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path(name): extract::Path<CrateName>,
    Json(req): Json<AddOwnerRequest>,
) -> Result<impl IntoResponse, HttpError> {
    let names = natural_human_names(&req.users);
    state.auth.writable(&token, &name).await?;
    state.store.delete_owner(&name, req.users).await?;
    Ok((
        StatusCode::OK,
        Json(
            json!({"ok": true, "msg": format!("user {names} has been added to {}", name.original)}),
        ),
    ))
}

async fn get_crate<S: Store, A: Auth>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Path((name, ver)): extract::Path<(CrateName, semver::Version)>,
) -> impl IntoResponse {
    state.auth.readable(&token, &name).await?;
    state.store.get_crate(&name, ver).await
}

async fn search_crates<S: Store, A: Auth + Clone>(
    extract::TypedHeader(RawAuthorization(token)): extract::TypedHeader<RawAuthorization>,
    extract::State(state): extract::State<State<S, A>>,
    extract::Query(query): extract::Query<SearchCratesQuery>,
) -> Result<impl IntoResponse, HttpError> {
    let (packages, total) = state.store.search(&query).await?;
    let packages = futures_util::stream::iter(packages);
    let packages = packages
        .filter(|package| {
            let token = token.clone();
            let name = package.name.parse();
            let auth = state.auth.clone();
            async move {
                let Ok(name) = name else {
                    return false;
                };
                auth.readable(&token, &name).await.is_ok()
            }
        })
        .collect::<Vec<_>>()
        .await;
    Ok((
        StatusCode::OK,
        Json(json!({"crates": packages, "meta": { "total": total }})),
    ))
}

#[cfg(unix)]
async fn wait_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut sigint = signal(SignalKind::interrupt()).expect("sigint handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("sigterm handler");
    let mut sighup = signal(SignalKind::hangup()).expect("sighup handler");
    tokio::select! {
        Some(()) = sigint.recv() => (),
        Some(()) = sigterm.recv() => (),
        Some(()) = sighup.recv() => (),
    }
}

async fn access_log_on_request<B>(
    req: Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, StatusCode> {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(ToString::to_string);
    let response = next.run(req).await;
    info!(
        method,
        path,
        query,
        status = response.status().as_u16(),
        "access"
    );
    Ok(response)
}

async fn run(opts: Opts) -> anyhow::Result<()> {
    let store = server::store::s3::S3Store::new(opts.objstore, opts.objstore_endpoint).await;
    let auth_rules = fs::read_to_string(&opts.rules).await?;
    let auth_rules = serde_yaml::from_str(&auth_rules)?;
    let auth = server::auth::github::GitHubAuth::new_from_config(auth_rules);
    store.health_check().await?;
    info!("store_healthcheck_passed");
    let state = State {
        addr: opts.addr,
        store,
        auth,
    };

    let v1_api = Router::new()
        .route("/crates/new", routing::put(publish_crate))
        .route("/crates/:name/:ver", routing::get(get_crate))
        .route("/crates/:name/:ver/yank", routing::delete(yank_crate))
        .route("/crates/:name/:ver/yank", routing::put(unyank_crate))
        .route("/crates/:name/owners", routing::get(get_owners))
        .route("/crates/:name/owners", routing::put(add_owner))
        .route("/crates/:name/owners", routing::delete(delete_owner))
        .route("/crates", routing::get(search_crates))
        .with_state(state.clone());

    let app = Router::new()
        .nest("/api/v1", v1_api)
        .route("/config.json", routing::get(config))
        .route("/1/:name", routing::get(get_index_len_1))
        .route("/2/:name", routing::get(get_index_len_2))
        .route("/3/:prefix/:name", routing::get(get_index_len_3))
        .route(
            "/:prefix1/:prefix2/:name",
            routing::get(get_index_len_at_least_4),
        )
        .layer(axum::middleware::from_fn(access_log_on_request))
        .with_state(state);

    info!(addr = opts.addr.to_string(), "init");

    axum::Server::bind(&opts.addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(wait_shutdown())
        .await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let opts = Opts::parse();
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=Info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    if let Err(e) = run(opts).await {
        error!(e = e.to_string(), "error");
    }
}
