use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use common::*;
use rand::seq::SliceRandom;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub mod customer_proto {
    tonic::include_proto!("customer_db");
}

pub mod product_proto {
    tonic::include_proto!("product_db");
}

use customer_proto::customer_database_client::CustomerDatabaseClient;
use product_proto::product_database_client::ProductDatabaseClient;

#[derive(Clone)]
struct AppState {
    customer_db_replicas: Vec<String>,
    current_replica: Arc<Mutex<usize>>,
    product_db_peers: Vec<String>,
    product_db_leader: Arc<Mutex<String>>,
}

async fn get_customer_client(
    state: &AppState,
) -> Result<CustomerDatabaseClient<tonic::transport::Channel>, String> {
    let replicas = &state.customer_db_replicas;
    let n = replicas.len();

    let start_idx = {
        let guard = state.current_replica.lock().unwrap();
        *guard
    };

    let mut remaining: Vec<usize> = (0..n).filter(|&i| i != start_idx).collect();
    remaining.shuffle(&mut rand::thread_rng());
    let order = std::iter::once(start_idx).chain(remaining);

    for idx in order {
        let addr = format!("http://{}", replicas[idx]);
        match CustomerDatabaseClient::connect(addr).await {
            Ok(client) => {
                *state.current_replica.lock().unwrap() = idx;
                return Ok(client);
            }
            Err(_) => {
                eprintln!("[CustomerDB] Replica {} unreachable, trying another", idx);
            }
        }
    }

    Err("All customer DB replicas are unreachable".to_string())
}

async fn connect_product_db(
    addr: &str,
) -> Result<ProductDatabaseClient<tonic::transport::Channel>, String> {
    let url = if addr.starts_with("http://") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    };
    ProductDatabaseClient::connect(url)
        .await
        .map_err(|e| format!("Failed to connect to product_db {}: {}", addr, e))
}

async fn get_product_client(
    state: &AppState,
) -> Result<ProductDatabaseClient<tonic::transport::Channel>, String> {
    let leader = state.product_db_leader.lock().unwrap().clone();
    if let Ok(client) = connect_product_db(&leader).await {
        return Ok(client);
    }
    for peer in &state.product_db_peers {
        if *peer != leader {
            if let Ok(client) = connect_product_db(peer).await {
                *state.product_db_leader.lock().unwrap() = peer.clone();
                return Ok(client);
            }
        }
    }
    Err("All product_db nodes are unreachable".to_string())
}

fn try_update_product_leader(state: &AppState, status: &tonic::Status) -> Option<String> {
    if status.code() == tonic::Code::FailedPrecondition {
        if let Some(addr) = status
            .metadata()
            .get("x-raft-leader-addr")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim_start_matches("http://").to_string())
        {
            if !addr.is_empty() {
                *state.product_db_leader.lock().unwrap() = addr.clone();
                return Some(addr);
            }
        }
    }
    None
}

async fn with_product_retry<F, Fut, T>(state: &AppState, call: F) -> Result<T, String>
where
    F: Fn(ProductDatabaseClient<tonic::transport::Channel>) -> Fut,
    Fut: std::future::Future<Output = Result<tonic::Response<T>, tonic::Status>>,
{
    let client = get_product_client(state).await?;
    match call(client).await {
        Ok(resp) => Ok(resp.into_inner()),
        Err(status) => {
            if let Some(new_leader) = try_update_product_leader(state, &status) {
                let client = connect_product_db(&new_leader).await?;
                call(client)
                    .await
                    .map(|r| r.into_inner())
                    .map_err(|e| e.to_string())
            } else {
                Err(status.to_string())
            }
        }
    }
}

async fn validate_session(
    state: &AppState,
    session_id_str: &str,
    expected_type: &str,
) -> Result<customer_proto::Session, String> {
    let mut client = get_customer_client(state).await?;
    let resp = client
        .get_session(customer_proto::GetSessionRequest {
            session_id: session_id_str.to_string(),
        })
        .await
        .map_err(|e| format!("Session lookup failed: {}", e))?
        .into_inner();

    if !resp.found {
        return Err("Session not found".to_string());
    }

    let session = resp.session.ok_or("Session data missing")?;
    let now = Utc::now().timestamp();
    if session.expiration < now {
        let _ = client
            .delete_session(customer_proto::DeleteSessionRequest {
                session_id: session_id_str.to_string(),
            })
            .await;
        return Err("Session expired".to_string());
    }

    if session.user_type != expected_type {
        return Err("Invalid session type".to_string());
    }

    Ok(session)
}

// POST /seller/create-account
async fn create_account(
    State(state): State<AppState>,
    Json(req): Json<CreateAccountRequest>,
) -> (StatusCode, Json<ApiResponse<CreateAccountResponse>>) {
    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .create_seller(customer_proto::CreateSellerRequest {
            seller_name: req.name,
            password: req.password,
        })
        .await
    {
        Ok(resp) => {
            let r = resp.into_inner();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(CreateAccountResponse {
                    user_id: r.seller_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

// POST /seller/login
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> (StatusCode, Json<ApiResponse<LoginResponse>>) {
    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let resp = match client
        .get_seller_by_name(customer_proto::GetSellerByNameRequest {
            seller_name: req.name.clone(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            )
        }
    };

    if !resp.found {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Seller not found")),
        );
    }

    let seller = resp.seller.unwrap();
    if seller.password != req.password {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::error("Invalid password")),
        );
    }

    match client
        .create_session(customer_proto::CreateSessionRequest {
            user_id: seller.seller_id,
            user_type: "Seller".to_string(),
        })
        .await
    {
        Ok(resp) => {
            let r = resp.into_inner();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(LoginResponse {
                    session_id: r.session_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

// POST /seller/{session_id}/logout
async fn logout(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .delete_session(customer_proto::DeleteSessionRequest {
            session_id: session_id.clone(),
        })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

// GET /seller/{session_id}/rating
async fn get_seller_rating(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Feedback>>) {
    let session = match validate_session(&state, &session_id, "Seller").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .get_seller(customer_proto::GetSellerRequest {
            seller_id: session.user_id,
        })
        .await
    {
        Ok(resp) => {
            let r = resp.into_inner();
            if r.found {
                let seller = r.seller.unwrap();
                let fb = seller.feedback.unwrap_or(customer_proto::Feedback {
                    thumbs_up: 0,
                    thumbs_down: 0,
                });
                (
                    StatusCode::OK,
                    Json(ApiResponse::ok(Feedback {
                        thumbs_up: fb.thumbs_up,
                        thumbs_down: fb.thumbs_down,
                    })),
                )
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error("Seller not found")),
                )
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

// POST /seller/{session_id}/items
async fn register_item(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<RegisterItemRequest>,
) -> (StatusCode, Json<ApiResponse<RegisterItemResponse>>) {
    let session = match validate_session(&state, &session_id, "Seller").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let condition = match req.condition.to_lowercase().as_str() {
        "new" => "New",
        "used" => "Used",
        _ => return (StatusCode::BAD_REQUEST, Json(ApiResponse::error("Condition must be 'new' or 'used'"))),
    };

    let item = product_proto::Item {
        item_id: String::new(),
        item_name: req.item_name,
        item_category: req.item_category,
        keywords: req.keywords,
        condition: condition.to_string(),
        sale_price: req.sale_price,
        quantity: req.quantity,
        feedback: Some(product_proto::Feedback { thumbs_up: 0, thumbs_down: 0 }),
        seller_id: session.user_id,
    };
    let create_req = product_proto::CreateItemRequest { item: Some(item) };
    match with_product_retry(&state, move |mut c| {
        let r = create_req.clone();
        async move { c.create_item(r).await }
    })
    .await
    {
        Ok(r) => (StatusCode::OK, Json(ApiResponse::ok(RegisterItemResponse { item_id: r.item_id }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// PUT /seller/{session_id}/items/{item_id}/price
async fn change_item_price(
    State(state): State<AppState>,
    axum::extract::Path((session_id, item_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<ChangePriceRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Seller").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let item_resp = match client
        .get_item(product_proto::GetItemRequest { item_id: item_id.clone() })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            try_update_product_leader(&state, &e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())));
        }
    };

    if !item_resp.found {
        return (StatusCode::NOT_FOUND, Json(ApiResponse::error("Item not found")));
    }

    let mut item = item_resp.item.unwrap();
    if item.seller_id != session.user_id {
        return (StatusCode::FORBIDDEN, Json(ApiResponse::error("Not your item")));
    }

    item.sale_price = req.new_price;

    match client
        .update_item(product_proto::UpdateItemRequest { item: Some(item) })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => {
            try_update_product_leader(&state, &e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())))
        }
    }
}

// PUT /seller/{session_id}/items/{item_id}/quantity
async fn update_units(
    State(state): State<AppState>,
    axum::extract::Path((session_id, item_id)): axum::extract::Path<(String, String)>,
    Json(req): Json<UpdateUnitsRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Seller").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let item_resp = match client
        .get_item(product_proto::GetItemRequest { item_id: item_id.clone() })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            try_update_product_leader(&state, &e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())));
        }
    };

    if !item_resp.found {
        return (StatusCode::NOT_FOUND, Json(ApiResponse::error("Item not found")));
    }

    let mut item = item_resp.item.unwrap();
    if item.seller_id != session.user_id {
        return (StatusCode::FORBIDDEN, Json(ApiResponse::error("Not your item")));
    }

    item.quantity = req.quantity;

    match client
        .update_item(product_proto::UpdateItemRequest { item: Some(item) })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => {
            try_update_product_leader(&state, &e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())))
        }
    }
}

// GET /seller/{session_id}/items
async fn display_items(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<Item>>>) {
    let session = match validate_session(&state, &session_id, "Seller").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let list_req = product_proto::GetItemsBySellerRequest { seller_id: session.user_id };
    match with_product_retry(&state, move |mut c| {
        let r = list_req.clone();
        async move { c.get_items_by_seller(r).await }
    })
    .await
    {
        Ok(resp) => {
            let items: Vec<Item> = resp.items.into_iter().map(proto_item_to_common).collect();
            (StatusCode::OK, Json(ApiResponse::ok(items)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /reset - clear all data in both databases (for benchmarking)
async fn reset_all(
    State(state): State<AppState>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let mut customer_client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };
    if let Err(e) = customer_client
        .reset_all(customer_proto::ResetRequest {})
        .await
    {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Customer DB reset failed: {}", e))));
    }

    match with_product_retry(&state, |mut c| async move {
        c.reset_all(product_proto::ResetRequest {}).await
    })
    .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Product DB reset failed: {}", e)))),
    }
}

fn proto_item_to_common(item: product_proto::Item) -> Item {
    let condition = match item.condition.as_str() {
        "New" => Condition::New,
        _ => Condition::Used,
    };
    let fb = item.feedback.unwrap_or(product_proto::Feedback {
        thumbs_up: 0,
        thumbs_down: 0,
    });
    Item {
        item_id: Uuid::parse_str(&item.item_id).unwrap_or(Uuid::nil()),
        item_name: item.item_name,
        item_category: item.item_category,
        keywords: item.keywords,
        condition,
        sale_price: item.sale_price,
        quantity: item.quantity,
        feedback: Feedback {
            thumbs_up: fb.thumbs_up,
            thumbs_down: fb.thumbs_down,
        },
        seller_id: Uuid::parse_str(&item.seller_id).unwrap_or(Uuid::nil()),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr =
        std::env::var("SELLER_SERVER_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8082".to_string());
    // Comma-separated list of customer DB replica addresses, e.g.
    // "127.0.0.1:50051,127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055"
    let customer_db_replicas: Vec<String> = std::env::var("CUSTOMER_DB_ADDRS")
        .unwrap_or_else(|_| {
            "127.0.0.1:50051,127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055"
                .to_string()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let product_db_initial =
        std::env::var("PRODUCT_DB_ADDR").unwrap_or_else(|_| "127.0.0.1:50052".to_string());
    let product_db_peers: Vec<String> = std::env::var("PRODUCT_DB_PEERS")
        .unwrap_or_else(|_| {
            "127.0.0.1:50052,127.0.0.1:50053,127.0.0.1:50054,127.0.0.1:50055,127.0.0.1:50056"
                .to_string()
        })
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    println!("Customer DB replicas: {:?}", customer_db_replicas);
    println!("Product DB peers: {:?}", product_db_peers);

    let state = AppState {
        customer_db_replicas,
        current_replica: Arc::new(Mutex::new(0)),
        product_db_peers,
        product_db_leader: Arc::new(Mutex::new(product_db_initial)),
    };

    let app = Router::new()
        .route("/reset", post(reset_all))
        .route("/seller/create-account", post(create_account))
        .route("/seller/login", post(login))
        .route("/seller/:session_id/logout", post(logout))
        .route("/seller/:session_id/rating", get(get_seller_rating))
        .route("/seller/:session_id/items", post(register_item))
        .route("/seller/:session_id/items", get(display_items))
        .route(
            "/seller/:session_id/items/:item_id/price",
            put(change_item_price),
        )
        .route(
            "/seller/:session_id/items/:item_id/quantity",
            put(update_units),
        )
        .with_state(state);

    println!("Seller Server (REST) listening on {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
