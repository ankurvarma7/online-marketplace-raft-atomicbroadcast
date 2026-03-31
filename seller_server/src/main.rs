use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use common::*;
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
    customer_db_addr: String,
    product_db_addr: String,
}

async fn get_customer_client(
    state: &AppState,
) -> Result<CustomerDatabaseClient<tonic::transport::Channel>, String> {
    CustomerDatabaseClient::connect(format!("http://{}", state.customer_db_addr))
        .await
        .map_err(|e| format!("Failed to connect to customer DB: {}", e))
}

async fn get_product_client(
    state: &AppState,
) -> Result<ProductDatabaseClient<tonic::transport::Channel>, String> {
    ProductDatabaseClient::connect(format!("http://{}", state.product_db_addr))
        .await
        .map_err(|e| format!("Failed to connect to product DB: {}", e))
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

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    let item = product_proto::Item {
        item_id: String::new(),
        item_name: req.item_name,
        item_category: req.item_category,
        keywords: req.keywords,
        condition: condition.to_string(),
        sale_price: req.sale_price,
        quantity: req.quantity,
        feedback: Some(product_proto::Feedback {
            thumbs_up: 0,
            thumbs_down: 0,
        }),
        seller_id: session.user_id,
    };

    match client
        .create_item(product_proto::CreateItemRequest { item: Some(item) })
        .await
    {
        Ok(resp) => {
            let r = resp.into_inner();
            (
                StatusCode::OK,
                Json(ApiResponse::ok(RegisterItemResponse {
                    item_id: r.item_id,
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
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
        .get_item(product_proto::GetItemRequest {
            item_id: item_id.clone(),
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

    if !item_resp.found {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Item not found")),
        );
    }

    let mut item = item_resp.item.unwrap();
    if item.seller_id != session.user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Not your item")),
        );
    }

    item.sale_price = req.new_price;

    match client
        .update_item(product_proto::UpdateItemRequest { item: Some(item) })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
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
        .get_item(product_proto::GetItemRequest {
            item_id: item_id.clone(),
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

    if !item_resp.found {
        return (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error("Item not found")),
        );
    }

    let mut item = item_resp.item.unwrap();
    if item.seller_id != session.user_id {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiResponse::error("Not your item")),
        );
    }

    item.quantity = req.quantity;

    match client
        .update_item(product_proto::UpdateItemRequest { item: Some(item) })
        .await
    {
        Ok(_) => (StatusCode::OK, Json(ApiResponse::ok(EmptyData {}))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
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

    let mut client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    match client
        .get_items_by_seller(product_proto::GetItemsBySellerRequest {
            seller_id: session.user_id,
        })
        .await
    {
        Ok(resp) => {
            let items: Vec<Item> = resp
                .into_inner()
                .items
                .into_iter()
                .map(proto_item_to_common)
                .collect();
            (StatusCode::OK, Json(ApiResponse::ok(items)))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(e.to_string())),
        ),
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
    let mut product_client = match get_product_client(&state).await {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e))),
    };

    if let Err(e) = customer_client
        .reset_all(customer_proto::ResetRequest {})
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Customer DB reset failed: {}", e))),
        );
    }

    if let Err(e) = product_client
        .reset_all(product_proto::ResetRequest {})
        .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::error(format!("Product DB reset failed: {}", e))),
        );
    }

    (StatusCode::OK, Json(ApiResponse::ok(EmptyData {})))
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
    let customer_db_addr =
        std::env::var("CUSTOMER_DB_ADDR").unwrap_or_else(|_| "127.0.0.1:50051".to_string());
    let product_db_addr =
        std::env::var("PRODUCT_DB_ADDR").unwrap_or_else(|_| "127.0.0.1:50052".to_string());

    let state = AppState {
        customer_db_addr,
        product_db_addr,
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
