use crate::grpc::ProductDbRaft;
use crate::proto::product_database_server::ProductDatabase;
use crate::proto::*;
use crate::storage::ProductDbStore;
use crate::types::{item_proto_to_data, CartItemData, ProductDbRequest, ProductDbResponse};
use async_raft::error::{ClientReadError, ClientWriteError};
use async_raft::raft::ClientWriteRequest;
use async_raft::NodeId;
use std::collections::HashMap;
use std::sync::Arc;
use tonic::metadata::MetadataValue;
use tonic::{Request, Response, Status};

/// gRPC application API: forwards writes through Raft; reads use `client_read` then local SQLite.
pub struct ProductDb {
    pub raft: Arc<ProductDbRaft>,
    pub store: Arc<ProductDbStore>,
    /// All cluster members (for leader redirect hints).
    pub leader_addrs: Arc<HashMap<NodeId, String>>,
}

impl ProductDb {
    fn redirect_status(&self, leader: Option<NodeId>) -> Status {
        let mut s = Status::failed_precondition(
            "not leader: retry against the Raft leader (see metadata x-raft-leader-addr)",
        );
        if let Some(lid) = leader {
            if let Ok(v) = MetadataValue::try_from(format!("{}", lid)) {
                s.metadata_mut().insert("x-raft-leader-id", v);
            }
            if let Some(addr) = self.leader_addrs.get(&lid) {
                if let Ok(v) = MetadataValue::try_from(addr.clone()) {
                    s.metadata_mut().insert("x-raft-leader-addr", v);
                }
            }
        }
        s
    }

    async fn ensure_leader_read(&self) -> Result<(), Status> {
        match self.raft.client_read().await {
            Ok(()) => Ok(()),
            Err(ClientReadError::ForwardToLeader(leader)) => Err(self.redirect_status(leader)),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    async fn write(&self, payload: ProductDbRequest) -> Result<ProductDbResponse, Status> {
        let req = ClientWriteRequest::new(payload);
        match self.raft.client_write(req).await {
            Ok(resp) => Ok(resp.data),
            Err(ClientWriteError::ForwardToLeader(_, leader)) => Err(self.redirect_status(leader)),
            Err(ClientWriteError::RaftError(e)) => Err(Status::internal(e.to_string())),
        }
    }
}

#[tonic::async_trait]
impl ProductDatabase for ProductDb {
    async fn create_item(
        &self,
        request: Request<CreateItemRequest>,
    ) -> Result<Response<CreateItemResponse>, Status> {
        let req = request.into_inner();
        let item = req
            .item
            .ok_or_else(|| Status::invalid_argument("Item data required"))?;
        let data = self
            .write(ProductDbRequest::CreateItem(item_proto_to_data(&item)))
            .await?;
        let item_id = data.data.ok_or_else(|| Status::internal("missing item id"))?;
        Ok(Response::new(CreateItemResponse { item_id }))
    }

    async fn update_item(
        &self,
        request: Request<UpdateItemRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        let item = req
            .item
            .ok_or_else(|| Status::invalid_argument("Item data required"))?;
        self.write(ProductDbRequest::UpdateItem(item_proto_to_data(&item)))
            .await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Item updated".to_string(),
        }))
    }

    async fn add_to_cart(
        &self,
        request: Request<CartOperationRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        self.write(ProductDbRequest::AddToCart {
            buyer_id: req.buyer_id,
            item_id: req.item_id,
            quantity: req.quantity,
        })
        .await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Added to cart".to_string(),
        }))
    }

    async fn remove_from_cart(
        &self,
        request: Request<CartOperationRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        self.write(ProductDbRequest::RemoveFromCart {
            buyer_id: req.buyer_id,
            item_id: req.item_id,
            quantity: req.quantity,
        })
        .await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Removed from cart".to_string(),
        }))
    }

    async fn clear_cart(
        &self,
        request: Request<ClearCartRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        self.write(ProductDbRequest::ClearCart(req.buyer_id)).await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Cart cleared".to_string(),
        }))
    }

    async fn save_cart(
        &self,
        request: Request<SaveCartRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        let items: Vec<CartItemData> = req
            .items
            .into_iter()
            .map(|c| CartItemData {
                item_id: c.item_id,
                quantity: c.quantity,
            })
            .collect();
        self.write(ProductDbRequest::SaveCart {
            buyer_id: req.buyer_id,
            items,
        })
        .await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Cart saved".to_string(),
        }))
    }

    async fn add_purchase_history(
        &self,
        request: Request<AddPurchaseHistoryRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        self.write(ProductDbRequest::AddPurchaseHistory {
            buyer_id: req.buyer_id,
            item_id: req.item_id,
        })
        .await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "Purchase recorded".to_string(),
        }))
    }

    async fn reset_all(
        &self,
        _request: Request<ResetRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        self.write(ProductDbRequest::Reset).await?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "All product data cleared".to_string(),
        }))
    }

    async fn get_item(&self, request: Request<GetItemRequest>) -> Result<Response<ItemResponse>, Status> {
        self.ensure_leader_read().await?;
        let req = request.into_inner();
        let rows: Vec<Item> = {
            let conn = self.store.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, name, category, keywords, condition, price, quantity, seller_id, thumbs_up, thumbs_down FROM items WHERE id = ?1")
                .map_err(|e| Status::internal(e.to_string()))?;
            let item_iter = stmt
                .query_map([&req.item_id], |row| {
                    let keywords: String = row.get(3)?;
                    Ok(Item {
                        item_id: row.get(0)?,
                        item_name: row.get(1)?,
                        item_category: row.get(2)?,
                        keywords: keywords.split(',').map(String::from).collect(),
                        condition: row.get(4)?,
                        sale_price: row.get(5)?,
                        quantity: row.get(6)?,
                        seller_id: row.get(7)?,
                        feedback: Some(Feedback {
                            thumbs_up: row.get(8)?,
                            thumbs_down: row.get(9)?,
                        }),
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            item_iter.filter_map(Result::ok).collect()
        };

        if let Some(item) = rows.into_iter().next() {
            Ok(Response::new(ItemResponse {
                found: true,
                item: Some(item),
            }))
        } else {
            Ok(Response::new(ItemResponse {
                found: false,
                item: None,
            }))
        }
    }

    async fn get_items_by_seller(
        &self,
        request: Request<GetItemsBySellerRequest>,
    ) -> Result<Response<ItemsResponse>, Status> {
        self.ensure_leader_read().await?;
        let req = request.into_inner();
        let conn = self.store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, category, keywords, condition, price, quantity, seller_id, thumbs_up, thumbs_down FROM items WHERE seller_id = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;
        let item_iter = stmt
            .query_map([&req.seller_id], |row| {
                let keywords: String = row.get(3)?;
                Ok(Item {
                    item_id: row.get(0)?,
                    item_name: row.get(1)?,
                    item_category: row.get(2)?,
                    keywords: keywords.split(',').map(String::from).collect(),
                    condition: row.get(4)?,
                    sale_price: row.get(5)?,
                    quantity: row.get(6)?,
                    seller_id: row.get(7)?,
                    feedback: Some(Feedback {
                        thumbs_up: row.get(8)?,
                        thumbs_down: row.get(9)?,
                    }),
                })
            })
            .map_err(|e| Status::internal(e.to_string()))?;
        let items: Vec<Item> = item_iter.filter_map(Result::ok).collect();
        Ok(Response::new(ItemsResponse { items }))
    }

    async fn search_items(
        &self,
        request: Request<SearchItemsRequest>,
    ) -> Result<Response<ItemsResponse>, Status> {
        self.ensure_leader_read().await?;
        let req = request.into_inner();
        let conn = self.store.conn.lock().unwrap();
        let mut query =
            "SELECT id, name, category, keywords, condition, price, quantity, seller_id, thumbs_up, thumbs_down FROM items WHERE 1=1".to_string();
        if req.has_category {
            query.push_str(&format!(" AND category = {}", req.category));
        }
        let mut stmt = conn.prepare(&query).map_err(|e| Status::internal(e.to_string()))?;
        let item_iter = stmt
            .query_map([], |row| {
                let keywords: String = row.get(3)?;
                Ok(Item {
                    item_id: row.get(0)?,
                    item_name: row.get(1)?,
                    item_category: row.get(2)?,
                    keywords: keywords.split(',').map(String::from).collect(),
                    condition: row.get(4)?,
                    sale_price: row.get(5)?,
                    quantity: row.get(6)?,
                    seller_id: row.get(7)?,
                    feedback: Some(Feedback {
                        thumbs_up: row.get(8)?,
                        thumbs_down: row.get(9)?,
                    }),
                })
            })
            .map_err(|e| Status::internal(e.to_string()))?;
        let mut items: Vec<Item> = item_iter
            .filter_map(Result::ok)
            .filter(|item| {
                req.keywords.is_empty() || req.keywords.iter().all(|kw| item.keywords.contains(kw))
            })
            .collect();
        items.sort_by(|a, b| {
            let a_matches = req.keywords.iter().filter(|kw| a.keywords.contains(kw)).count();
            let b_matches = req.keywords.iter().filter(|kw| b.keywords.contains(kw)).count();
            b_matches.cmp(&a_matches)
        });
        Ok(Response::new(ItemsResponse { items }))
    }

    async fn get_cart(&self, request: Request<GetCartRequest>) -> Result<Response<CartResponse>, Status> {
        self.ensure_leader_read().await?;
        let req = request.into_inner();
        let conn = self.store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT item_id, quantity FROM carts WHERE buyer_id = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;
        let cart_iter = stmt
            .query_map([&req.buyer_id], |row| {
                Ok(CartItem {
                    item_id: row.get(0)?,
                    quantity: row.get(1)?,
                })
            })
            .map_err(|e| Status::internal(e.to_string()))?;
        let items: Vec<CartItem> = cart_iter.filter_map(Result::ok).collect();
        Ok(Response::new(CartResponse { items }))
    }

    async fn get_purchase_history(
        &self,
        request: Request<GetPurchaseHistoryRequest>,
    ) -> Result<Response<PurchaseHistoryResponse>, Status> {
        self.ensure_leader_read().await?;
        let req = request.into_inner();
        let conn = self.store.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT item_id FROM purchase_history WHERE buyer_id = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;
        let history_iter = stmt
            .query_map([&req.buyer_id], |row| row.get(0))
            .map_err(|e| Status::internal(e.to_string()))?;
        let item_ids: Vec<String> = history_iter.filter_map(Result::ok).collect();
        Ok(Response::new(PurchaseHistoryResponse { item_ids }))
    }
}
