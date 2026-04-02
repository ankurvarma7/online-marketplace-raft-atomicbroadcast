use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
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
    financial_tx_addr: String,
}

async fn get_customer_client(
    state: &AppState,
) -> Result<CustomerDatabaseClient<tonic::transport::Channel>, String> {
    let replicas = &state.customer_db_replicas;
    let n = replicas.len();

    // Determine starting index under the lock, then release it before doing I/O.
    let start_idx = {
        let guard = state.current_replica.lock().unwrap();
        *guard
    };

    // Try current replica first, then the rest in random order.
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
    // Cached leader unreachable — try all peers.
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

/// If a tonic Status is a Raft leader-redirect, update the leader cache and
/// return the new leader address so the caller can retry.
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

/// Call one product_db RPC, retrying once against the true leader if the first
/// attempt returns a ForwardToLeader redirect.
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
                // Retry once against the actual leader.
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

// POST /buyer/create-account
async fn create_account(
    State(state): State<AppState>,
    Json(req): Json<CreateAccountRequest>,
) -> (StatusCode, Json<ApiResponse<CreateAccountResponse>>) {
    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .create_buyer(customer_proto::CreateBuyerRequest {
            buyer_name: req.name,
            password: req.password,
        })
        .await
    {
        Ok(resp) => {
            let r = resp.into_inner();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(CreateAccountResponse {
                    user_id: r.buyer_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
    }
}

// POST /buyer/login
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> (StatusCode, Json<ApiResponse<LoginResponse>>) {
    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let resp = match client
        .get_buyer_by_name(customer_proto::GetBuyerByNameRequest {
            buyer_name: req.name.clone(),
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
            Json(ApiResponse::error("Buyer not found")),
        );
    }

    let buyer = resp.buyer.unwrap();
    if buyer.password != req.password {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::error("Invalid password")),
        );
    }

    match client
        .create_session(customer_proto::CreateSessionRequest {
            user_id: buyer.buyer_id,
            user_type: "Buyer".to_string(),
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

// POST /buyer/{session_id}/logout
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

// POST /buyer/{session_id}/search
async fn search_items(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<SearchRequest>,
) -> (StatusCode, Json<ApiResponse<Vec<Item>>>) {
    if let Err(e) = validate_session(&state, &session_id, "Buyer").await {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e)));
    }

    let search_req = product_proto::SearchItemsRequest {
        category: req.category.unwrap_or(0),
        has_category: req.category.is_some(),
        keywords: req.keywords,
    };
    match with_product_retry(&state, move |mut c| {
        let r = search_req.clone();
        async move { c.search_items(r).await }
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

// GET /buyer/{session_id}/items/{item_id}
async fn get_item(
    State(state): State<AppState>,
    axum::extract::Path((session_id, item_id)): axum::extract::Path<(String, String)>,
) -> (StatusCode, Json<ApiResponse<Option<Item>>>) {
    if let Err(e) = validate_session(&state, &session_id, "Buyer").await {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e)));
    }

    let get_req = product_proto::GetItemRequest { item_id: item_id.clone() };
    match with_product_retry(&state, move |mut c| {
        let r = get_req.clone();
        async move { c.get_item(r).await }
    })
    .await
    {
        Ok(resp) => {
            if resp.found {
                (StatusCode::OK, Json(ApiResponse::ok(Some(proto_item_to_common(resp.item.unwrap())))))
            } else {
                (StatusCode::OK, Json(ApiResponse::ok(None)))
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /buyer/{session_id}/cart/add
async fn add_to_cart(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<CartOperationRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let cart_req = product_proto::CartOperationRequest {
        buyer_id: session.user_id,
        item_id: req.item_id,
        quantity: req.quantity,
    };
    match with_product_retry(&state, move |mut c| {
        let r = cart_req.clone();
        async move { c.add_to_cart(r).await }
    })
    .await
    {
        Ok(r) => {
            if r.success {
                (StatusCode::OK, Json(ApiResponse::ok(EmptyData {})))
            } else {
                (StatusCode::BAD_REQUEST, Json(ApiResponse::error(r.message)))
            }
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /buyer/{session_id}/cart/remove
async fn remove_from_cart(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<CartOperationRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let cart_req = product_proto::CartOperationRequest {
        buyer_id: session.user_id,
        item_id: req.item_id,
        quantity: req.quantity,
    };
    match with_product_retry(&state, move |mut c| {
        let r = cart_req.clone();
        async move { c.remove_from_cart(r).await }
    })
    .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /buyer/{session_id}/cart/save
async fn save_cart(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let cart_resp = match client
        .get_cart(product_proto::GetCartRequest {
            buyer_id: session.user_id.clone(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            try_update_product_leader(&state, &e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())));
        }
    };

    match client
        .save_cart(product_proto::SaveCartRequest {
            buyer_id: session.user_id,
            items: cart_resp.items,
        })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => {
            try_update_product_leader(&state, &e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())))
        }
    }
}

// POST /buyer/{session_id}/cart/clear
async fn clear_cart(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let clear_req = product_proto::ClearCartRequest { buyer_id: session.user_id };
    match with_product_retry(&state, move |mut c| {
        let r = clear_req.clone();
        async move { c.clear_cart(r).await }
    })
    .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// GET /buyer/{session_id}/cart
async fn display_cart(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<CartItem>>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let cart_req = product_proto::GetCartRequest { buyer_id: session.user_id };
    match with_product_retry(&state, move |mut c| {
        let r = cart_req.clone();
        async move { c.get_cart(r).await }
    })
    .await
    {
        Ok(resp) => {
            let items: Vec<CartItem> = resp
                .items
                .into_iter()
                .map(|ci| CartItem {
                    item_id: Uuid::parse_str(&ci.item_id).unwrap_or(Uuid::nil()),
                    quantity: ci.quantity,
                })
                .collect();
            (StatusCode::OK, Json(ApiResponse::ok(items)))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /buyer/{session_id}/feedback
async fn provide_feedback(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<FeedbackRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    if let Err(e) = validate_session(&state, &session_id, "Buyer").await {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e)));
    }

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let item_resp = match client
        .get_item(product_proto::GetItemRequest {
            item_id: req.item_id.clone(),
        })
        .await
    {
        Ok(r) => r.into_inner(),
        Err(e) => {
            try_update_product_leader(&state, &e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string())));
        }
    };

    if !item_resp.found {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Item not found")),
        );
    }

    let mut item = item_resp.item.unwrap();
    let mut fb = item.feedback.unwrap_or(product_proto::Feedback {
        thumbs_up: 0,
        thumbs_down: 0,
    });
    if req.thumbs_up {
        fb.thumbs_up += 1;
    } else {
        fb.thumbs_down += 1;
    }
    item.feedback = Some(fb);

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

// GET /buyer/{session_id}/seller-rating/{seller_id}
async fn get_seller_rating(
    State(state): State<AppState>,
    axum::extract::Path((session_id, seller_id)): axum::extract::Path<(String, String)>,
) -> (StatusCode, Json<ApiResponse<Feedback>>) {
    if let Err(e) = validate_session(&state, &session_id, "Buyer").await {
        return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e)));
    }

    let mut client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .get_seller(customer_proto::GetSellerRequest {
            seller_id: seller_id.clone(),
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

// GET /buyer/{session_id}/purchases
async fn get_purchases(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
) -> (StatusCode, Json<ApiResponse<Vec<String>>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let hist_req = product_proto::GetPurchaseHistoryRequest { buyer_id: session.user_id };
    match with_product_retry(&state, move |mut c| {
        let r = hist_req.clone();
        async move { c.get_purchase_history(r).await }
    })
    .await
    {
        Ok(resp) => (StatusCode::OK, Json(ApiResponse::ok(resp.item_ids))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    }
}

// POST /buyer/{session_id}/purchase - MakePurchase (new for PA2)
async fn make_purchase(
    State(state): State<AppState>,
    axum::extract::Path(session_id): axum::extract::Path<String>,
    Json(req): Json<MakePurchaseRequest>,
) -> (StatusCode, Json<ApiResponse<EmptyData>>) {
    let session = match validate_session(&state, &session_id, "Buyer").await {
        Ok(s) => s,
        Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::error(e))),
    };

    let mut product_client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    // Get the cart
    let cart_resp = match product_client
        .get_cart(product_proto::GetCartRequest {
            buyer_id: session.user_id.clone(),
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

    if cart_resp.items.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error("Cart is empty")),
        );
    }

    // Call Financial Transactions SOAP service
    let soap_approved =
        call_financial_transactions(&state.financial_tx_addr, &req).await;

    match soap_approved {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::PAYMENT_REQUIRED,
                Json(ApiResponse::error("Transaction declined")),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!(
                    "Financial transaction service error: {}",
                    e
                ))),
            );
        }
    }

    // Process the purchase: deduct quantities, record purchase history, update seller stats
    for cart_item in &cart_resp.items {
        let item_resp = match product_client
            .get_item(product_proto::GetItemRequest {
                item_id: cart_item.item_id.clone(),
            })
            .await
        {
            Ok(r) => r.into_inner(),
            Err(_) => continue,
        };

        if !item_resp.found {
            continue;
        }

        let mut item = item_resp.item.unwrap();

        if item.quantity < cart_item.quantity {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!(
                    "Insufficient quantity for item {}",
                    item.item_name
                ))),
            );
        }

        item.quantity -= cart_item.quantity;
        let _ = product_client
            .update_item(product_proto::UpdateItemRequest {
                item: Some(item.clone()),
            })
            .await;

        let _ = product_client
            .add_purchase_history(product_proto::AddPurchaseHistoryRequest {
                buyer_id: session.user_id.clone(),
                item_id: cart_item.item_id.clone(),
            })
            .await;

        // Update seller items_sold count
        let mut customer_client = match get_customer_client(&state).await {
            Ok(c) => c,
            Err(_) => continue,
        };

        if let Ok(seller_resp) = customer_client
            .get_seller(customer_proto::GetSellerRequest {
                seller_id: item.seller_id.clone(),
            })
            .await
        {
            let sr = seller_resp.into_inner();
            if sr.found {
                let mut seller = sr.seller.unwrap();
                seller.items_sold += cart_item.quantity;
                let _ = customer_client
                    .update_seller(customer_proto::UpdateSellerRequest {
                        seller: Some(seller),
                    })
                    .await;
            }
        }
    }

    // Update buyer items_purchased count
    let total_items: i32 = cart_resp.items.iter().map(|ci| ci.quantity).sum();
    let mut customer_client = match get_customer_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    if let Ok(buyer_resp) = customer_client
        .get_buyer(customer_proto::GetBuyerRequest {
            buyer_id: session.user_id.clone(),
        })
        .await
    {
        let br = buyer_resp.into_inner();
        if br.found {
            let mut buyer = br.buyer.unwrap();
            buyer.items_purchased += total_items;
            let _ = customer_client
                .update_buyer(customer_proto::UpdateBuyerRequest {
                    buyer: Some(buyer),
                })
                .await;
        }
    }

    // Clear the cart after purchase
    let _ = product_client
        .clear_cart(product_proto::ClearCartRequest {
            buyer_id: session.user_id,
        })
        .await;

    (StatusCode::OK, Json(ApiResponse::ok(EmptyData {})))
}

async fn call_financial_transactions(
    addr: &str,
    req: &MakePurchaseRequest,
) -> Result<bool, String> {
    let soap_body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/"
               xmlns:ft="http://marketplace.example.com/financial">
  <soap:Body>
    <ft:ProcessTransaction>
      <ft:UserName>{}</ft:UserName>
      <ft:CreditCardNumber>{}</ft:CreditCardNumber>
      <ft:ExpirationDate>{}</ft:ExpirationDate>
      <ft:SecurityCode>{}</ft:SecurityCode>
    </ft:ProcessTransaction>
  </soap:Body>
</soap:Envelope>"#,
        req.credit_card_name, req.credit_card_number, req.expiration_date, req.security_code
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/financial/transaction", addr))
        .header("Content-Type", "text/xml; charset=utf-8")
        .header(
            "SOAPAction",
            "http://marketplace.example.com/financial/ProcessTransaction",
        )
        .body(soap_body)
        .send()
        .await
        .map_err(|e| format!("SOAP request failed: {}", e))?;

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read SOAP response: {}", e))?;

    Ok(body.contains("<Approved>true</Approved>"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bind_addr =
        std::env::var("BUYER_SERVER_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8083".to_string());
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
    let financial_tx_addr = std::env::var("FINANCIAL_TX_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8085".to_string());

    println!("Customer DB replicas: {:?}", customer_db_replicas);
    println!("Product DB peers: {:?}", product_db_peers);

    let state = AppState {
        customer_db_replicas,
        current_replica: Arc::new(Mutex::new(0)),
        product_db_peers,
        product_db_leader: Arc::new(Mutex::new(product_db_initial)),
        financial_tx_addr,
    };

    let app = Router::new()
        .route("/buyer/create-account", post(create_account))
        .route("/buyer/login", post(login))
        .route("/buyer/:session_id/logout", post(logout))
        .route("/buyer/:session_id/search", post(search_items))
        .route(
            "/buyer/:session_id/items/:item_id",
            get(get_item),
        )
        .route("/buyer/:session_id/cart/add", post(add_to_cart))
        .route("/buyer/:session_id/cart/remove", post(remove_from_cart))
        .route("/buyer/:session_id/cart/save", post(save_cart))
        .route("/buyer/:session_id/cart/clear", post(clear_cart))
        .route("/buyer/:session_id/cart", get(display_cart))
        .route("/buyer/:session_id/feedback", post(provide_feedback))
        .route(
            "/buyer/:session_id/seller-rating/:seller_id",
            get(get_seller_rating),
        )
        .route("/buyer/:session_id/purchases", get(get_purchases))
        .route("/buyer/:session_id/purchase", post(make_purchase))
        .with_state(state);

    println!("Buyer Server (REST) listening on {}", bind_addr);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
