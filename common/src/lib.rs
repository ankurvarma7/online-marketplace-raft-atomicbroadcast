use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Item {
    pub item_id: Uuid,
    pub item_name: String,
    pub item_category: i32,
    pub keywords: Vec<String>,
    pub condition: Condition,
    pub sale_price: f64,
    pub quantity: i32,
    pub feedback: Feedback,
    pub seller_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Condition {
    New,
    Used,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Feedback {
    pub thumbs_up: i32,
    pub thumbs_down: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Seller {
    pub seller_id: Uuid,
    pub seller_name: String,
    pub feedback: Feedback,
    pub items_sold: i32,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Buyer {
    pub buyer_id: Uuid,
    pub buyer_name: String,
    pub items_purchased: i32,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub user_type: UserType,
    pub expiration: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UserType {
    Buyer,
    Seller,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CartItem {
    pub item_id: Uuid,
    pub quantity: i32,
}

// REST API request/response types

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountRequest {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccountResponse {
    pub user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub session_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterItemRequest {
    pub item_name: String,
    pub item_category: i32,
    pub keywords: Vec<String>,
    pub condition: String,
    pub sale_price: f64,
    pub quantity: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterItemResponse {
    pub item_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChangePriceRequest {
    pub new_price: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUnitsRequest {
    pub quantity: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub category: Option<i32>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CartOperationRequest {
    pub item_id: String,
    pub quantity: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FeedbackRequest {
    pub item_id: String,
    pub thumbs_up: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetSellerRatingRequest {
    pub seller_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MakePurchaseRequest {
    pub credit_card_name: String,
    pub credit_card_number: String,
    pub expiration_date: String,
    pub security_code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        ApiResponse {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EmptyData {}
