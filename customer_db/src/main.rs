use rusqlite::{params, Connection, Result};
use std::sync::{Arc, Mutex};
use tonic::{transport::Server, Request, Response, Status};
use uuid::Uuid;
use chrono::Utc;

pub mod proto {
    tonic::include_proto!("customer_db");
}

use proto::customer_database_server::{CustomerDatabase, CustomerDatabaseServer};
use proto::*;

struct CustomerDb {
    conn: Arc<Mutex<Connection>>,
}

#[tonic::async_trait]
impl CustomerDatabase for CustomerDb {
    async fn create_seller(
        &self,
        request: Request<CreateSellerRequest>,
    ) -> Result<Response<CreateSellerResponse>, Status> {
        let req = request.into_inner();
        let seller_id = Uuid::new_v4().to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sellers (id, name, password, thumbs_up, thumbs_down, items_sold) VALUES (?1, ?2, ?3, 0, 0, 0)",
            &[&seller_id, &req.seller_name, &req.password],
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateSellerResponse { seller_id }))
    }

    async fn create_buyer(
        &self,
        request: Request<CreateBuyerRequest>,
    ) -> Result<Response<CreateBuyerResponse>, Status> {
        let req = request.into_inner();
        let buyer_id = Uuid::new_v4().to_string();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO buyers (id, name, password, items_purchased) VALUES (?1, ?2, ?3, 0)",
            &[&buyer_id, &req.buyer_name, &req.password],
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateBuyerResponse { buyer_id }))
    }

    async fn get_seller_by_name(
        &self,
        request: Request<GetSellerByNameRequest>,
    ) -> Result<Response<SellerResponse>, Status> {
        let req = request.into_inner();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, password, thumbs_up, thumbs_down, items_sold FROM sellers WHERE name = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;

        let sellers: Vec<Seller> = {
            let seller_iter = stmt
                .query_map(&[&req.seller_name], |row| {
                    Ok(Seller {
                        seller_id: row.get(0)?,
                        seller_name: row.get(1)?,
                        password: row.get(2)?,
                        feedback: Some(Feedback {
                            thumbs_up: row.get(3)?,
                            thumbs_down: row.get(4)?,
                        }),
                        items_sold: row.get(5)?,
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            seller_iter.filter_map(Result::ok).collect()
        };

        if let Some(seller) = sellers.into_iter().next() {
            Ok(Response::new(SellerResponse {
                found: true,
                seller: Some(seller),
            }))
        } else {
            Ok(Response::new(SellerResponse {
                found: false,
                seller: None,
            }))
        }
    }

    async fn get_buyer_by_name(
        &self,
        request: Request<GetBuyerByNameRequest>,
    ) -> Result<Response<BuyerResponse>, Status> {
        let req = request.into_inner();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, password, items_purchased FROM buyers WHERE name = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;

        let buyers: Vec<Buyer> = {
            let buyer_iter = stmt
                .query_map(&[&req.buyer_name], |row| {
                    Ok(Buyer {
                        buyer_id: row.get(0)?,
                        buyer_name: row.get(1)?,
                        password: row.get(2)?,
                        items_purchased: row.get(3)?,
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            buyer_iter.filter_map(Result::ok).collect()
        };

        if let Some(buyer) = buyers.into_iter().next() {
            Ok(Response::new(BuyerResponse {
                found: true,
                buyer: Some(buyer),
            }))
        } else {
            Ok(Response::new(BuyerResponse {
                found: false,
                buyer: None,
            }))
        }
    }

    async fn get_seller(
        &self,
        request: Request<GetSellerRequest>,
    ) -> Result<Response<SellerResponse>, Status> {
        let req = request.into_inner();
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, password, thumbs_up, thumbs_down, items_sold FROM sellers WHERE id = ?1")
            .map_err(|e| Status::internal(e.to_string()))?;

        let sellers: Vec<Seller> = {
            let seller_iter = stmt
                .query_map(&[&req.seller_id], |row| {
                    Ok(Seller {
                        seller_id: row.get(0)?,
                        seller_name: row.get(1)?,
                        password: row.get(2)?,
                        feedback: Some(Feedback {
                            thumbs_up: row.get(3)?,
                            thumbs_down: row.get(4)?,
                        }),
                        items_sold: row.get(5)?,
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            seller_iter.filter_map(Result::ok).collect()
        };

        if let Some(seller) = sellers.into_iter().next() {
            Ok(Response::new(SellerResponse {
                found: true,
                seller: Some(seller),
            }))
        } else {
            Ok(Response::new(SellerResponse {
                found: false,
                seller: None,
            }))
        }
    }

    async fn update_seller(
        &self,
        request: Request<UpdateSellerRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        let seller = req
            .seller
            .ok_or_else(|| Status::invalid_argument("Seller data required"))?;
        let feedback = seller.feedback.unwrap_or_default();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sellers SET thumbs_up = ?1, thumbs_down = ?2, items_sold = ?3 WHERE id = ?4",
            params![
                feedback.thumbs_up,
                feedback.thumbs_down,
                seller.items_sold,
                seller.seller_id
            ],
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GenericResponse {
            success: true,
            message: "Seller updated successfully".to_string(),
        }))
    }

    async fn update_buyer(
        &self,
        request: Request<UpdateBuyerRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        let buyer = req
            .buyer
            .ok_or_else(|| Status::invalid_argument("Buyer data required"))?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE buyers SET items_purchased = ?1 WHERE id = ?2",
            params![buyer.items_purchased, buyer.buyer_id],
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GenericResponse {
            success: true,
            message: "Buyer updated successfully".to_string(),
        }))
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let req = request.into_inner();
        let session_id = Uuid::new_v4().to_string();
        let expiration = Utc::now().timestamp() + 300; // 5 minutes
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (id, user_id, user_type, expiration) VALUES (?1, ?2, ?3, ?4)",
            params![session_id, req.user_id, req.user_type, expiration],
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateSessionResponse {
            session_id,
            expiration,
        }))
    }

    async fn get_buyer(
        &self,
        request: Request<GetBuyerRequest>,
    ) -> Result<Response<BuyerResponse>, Status> {
        let req = request.into_inner();
        let buyers: Vec<Buyer> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, name, password, items_purchased FROM buyers WHERE id = ?1")
                .map_err(|e| Status::internal(e.to_string()))?;
            let buyer_iter = stmt
                .query_map(&[&req.buyer_id], |row| {
                    Ok(Buyer {
                        buyer_id: row.get(0)?,
                        buyer_name: row.get(1)?,
                        password: row.get(2)?,
                        items_purchased: row.get(3)?,
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            buyer_iter.filter_map(Result::ok).collect()
        };

        if let Some(buyer) = buyers.into_iter().next() {
            Ok(Response::new(BuyerResponse {
                found: true,
                buyer: Some(buyer),
            }))
        } else {
            Ok(Response::new(BuyerResponse {
                found: false,
                buyer: None,
            }))
        }
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<SessionResponse>, Status> {
        let req = request.into_inner();
        let sessions: Vec<Session> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT id, user_id, user_type, expiration FROM sessions WHERE id = ?1")
                .map_err(|e| Status::internal(e.to_string()))?;
            let session_iter = stmt
                .query_map(&[&req.session_id], |row| {
                    Ok(Session {
                        session_id: row.get(0)?,
                        user_id: row.get(1)?,
                        user_type: row.get(2)?,
                        expiration: row.get(3)?,
                    })
                })
                .map_err(|e| Status::internal(e.to_string()))?;
            session_iter.filter_map(Result::ok).collect()
        };

        if let Some(session) = sessions.into_iter().next() {
            if session.expiration < Utc::now().timestamp() {
                let conn = self.conn.lock().unwrap();
                conn.execute("DELETE FROM sessions WHERE id = ?1", &[&req.session_id])
                    .map_err(|e| Status::internal(e.to_string()))?;
                Ok(Response::new(SessionResponse {
                    found: false,
                    session: None,
                }))
            } else {
                Ok(Response::new(SessionResponse {
                    found: true,
                    session: Some(session),
                }))
            }
        } else {
            Ok(Response::new(SessionResponse {
                found: false,
                session: None,
            }))
        }
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let req = request.into_inner();
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE id = ?1", &[&req.session_id])
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GenericResponse {
            success: true,
            message: "Session deleted successfully".to_string(),
        }))
    }

    async fn reset_all(
        &self,
        _request: Request<ResetRequest>,
    ) -> Result<Response<GenericResponse>, Status> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sellers", [])
            .map_err(|e| Status::internal(e.to_string()))?;
        conn.execute("DELETE FROM buyers", [])
            .map_err(|e| Status::internal(e.to_string()))?;
        conn.execute("DELETE FROM sessions", [])
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(GenericResponse {
            success: true,
            message: "All customer data cleared".to_string(),
        }))
    }
}

fn init_db(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sellers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            thumbs_up INTEGER NOT NULL,
            thumbs_down INTEGER NOT NULL,
            items_sold INTEGER NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS buyers (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            items_purchased INTEGER NOT NULL
        )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            user_type TEXT NOT NULL,
            expiration INTEGER NOT NULL
        )",
        [],
    )?;
    Ok(())
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50051".parse()?;
    let conn = Connection::open_in_memory()?;
    init_db(&conn)?;
    let db = CustomerDb {
        conn: Arc::new(Mutex::new(conn)),
    };

    println!("CustomerDB server listening on {}", addr);

    Server::builder()
        .add_service(CustomerDatabaseServer::new(db))
        .serve(addr)
        .await?;

    Ok(())
}
