//! Serde-friendly application types for Raft (`AppData` / `AppDataResponse`).

use serde::{Deserialize, Serialize};

/// Mirrors proto `Item` for replication (prost types do not implement serde).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ItemData {
    pub item_id: String,
    pub item_name: String,
    pub item_category: i32,
    pub keywords: Vec<String>,
    pub condition: String,
    pub sale_price: f64,
    pub quantity: i32,
    pub thumbs_up: i32,
    pub thumbs_down: i32,
    pub seller_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CartItemData {
    pub item_id: String,
    pub quantity: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ProductDbRequest {
    CreateItem(ItemData),
    UpdateItem(ItemData),
    AddToCart {
        buyer_id: String,
        item_id: String,
        quantity: i32,
    },
    RemoveFromCart {
        buyer_id: String,
        item_id: String,
        quantity: i32,
    },
    ClearCart(String),
    SaveCart {
        buyer_id: String,
        items: Vec<CartItemData>,
    },
    AddPurchaseHistory {
        buyer_id: String,
        item_id: String,
    },
    Reset,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductDbResponse {
    pub data: Option<String>,
}

impl async_raft::AppData for ProductDbRequest {}
impl async_raft::AppDataResponse for ProductDbResponse {}

pub fn item_proto_to_data(item: &crate::proto::Item) -> ItemData {
    let fb = item.feedback.as_ref();
    ItemData {
        item_id: item.item_id.clone(),
        item_name: item.item_name.clone(),
        item_category: item.item_category,
        keywords: item.keywords.clone(),
        condition: item.condition.clone(),
        sale_price: item.sale_price,
        quantity: item.quantity,
        thumbs_up: fb.map(|f| f.thumbs_up).unwrap_or(0),
        thumbs_down: fb.map(|f| f.thumbs_down).unwrap_or(0),
        seller_id: item.seller_id.clone(),
    }
}

#[allow(dead_code)]
pub fn item_data_to_proto(i: ItemData) -> crate::proto::Item {
    crate::proto::Item {
        item_id: i.item_id,
        item_name: i.item_name,
        item_category: i.item_category,
        keywords: i.keywords,
        condition: i.condition,
        sale_price: i.sale_price,
        quantity: i.quantity,
        feedback: Some(crate::proto::Feedback {
            thumbs_up: i.thumbs_up,
            thumbs_down: i.thumbs_down,
        }),
        seller_id: i.seller_id,
    }
}
