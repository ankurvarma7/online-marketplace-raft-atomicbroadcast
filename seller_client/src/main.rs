use clap::{Parser, Subcommand};
use common::*;
use rand::seq::SliceRandom;

fn get_seller_server_replicas() -> Vec<String> {
    let addrs = std::env::var("SELLER_SERVER_ADDRS").unwrap_or_else(|_| {
        "http://127.0.0.1:8082,http://127.0.0.1:8088,http://127.0.0.1:8089,http://127.0.0.1:8090"
            .to_string()
    });
    let mut replicas: Vec<String> = addrs
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    replicas.shuffle(&mut rand::thread_rng());
    replicas
}

async fn send_with_failover(
    replicas: &[String],
    build: impl Fn(&str) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let mut last_err: Option<reqwest::Error> = None;
    for base in replicas {
        match build(base).send().await {
            Ok(resp) => return Ok(resp),
            Err(e) if e.is_connect() || e.is_timeout() => {
                eprintln!("Replica {} unavailable, trying next...", base);
                last_err = Some(e);
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(last_err
        .map(|e| Box::new(e) as Box<dyn std::error::Error>)
        .unwrap_or_else(|| "All replicas failed".to_string().into()))
}

#[derive(Parser)]
#[command(name = "seller_client")]
#[command(about = "Online Marketplace Seller Client (REST)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    CreateAccount {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        password: String,
    },
    Login {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        password: String,
    },
    Logout {
        #[arg(short, long)]
        session_id: String,
    },
    GetRating {
        #[arg(short, long)]
        session_id: String,
    },
    RegisterItem {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        category: i32,
        #[arg(short, long, num_args = 1..=5, value_delimiter = ',')]
        keywords: Vec<String>,
        #[arg(long)]
        condition: String,
        #[arg(long)]
        price: f64,
        #[arg(short, long)]
        quantity: i32,
    },
    ChangePrice {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
        #[arg(short, long)]
        new_price: f64,
    },
    UpdateUnits {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
        #[arg(short, long)]
        quantity: i32,
    },
    DisplayItems {
        #[arg(short, long)]
        session_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let replicas = get_seller_server_replicas();
    let client = reqwest::Client::new();

    // stderr so this always shows next to API errors (some terminals hide stdout)
    eprintln!("[seller_client] SELLER_SERVER_ADDRS replicas: {:?}", replicas);

    match cli.command {
        Commands::CreateAccount { name, password } => {
            let body = CreateAccountRequest { name, password };
            let resp = send_with_failover(&replicas, |base| {
                client
                    .post(format!("{}/seller/create-account", base))
                    .json(&body)
            })
            .await?
            .json::<ApiResponse<CreateAccountResponse>>()
            .await?;
            if resp.success {
                let data = resp.data.unwrap();
                println!("Account created successfully!");
                println!("Seller ID: {}", data.user_id);
            } else {
                let err = resp.error.unwrap_or_default();
                eprintln!("Error: {}", err);
                if err.contains("Unimplemented") {
                    eprintln!(
                        "Hint: seller_server called customer_db gRPC CreateSeller and got UNIMPLEMENTED. \
                         Usually wrong host:port in seller's CUSTOMER_DB_ADDRS (e.g. pointing at product_db \
                         :50052 instead of customer_db :50051), or customer_db not running / wrong proto."
                    );
                }
            }
        }
        Commands::Login { name, password } => {
            let body = LoginRequest { name, password };
            let resp = send_with_failover(&replicas, |base| {
                client.post(format!("{}/seller/login", base)).json(&body)
            })
            .await?
            .json::<ApiResponse<LoginResponse>>()
            .await?;
            if resp.success {
                let data = resp.data.unwrap();
                println!("Login successful!");
                println!("Session ID: {}", data.session_id);
                println!("Session expires in 5 minutes");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::Logout { session_id } => {
            let resp = send_with_failover(&replicas, |base| {
                client.post(format!("{}/seller/{}/logout", base, session_id))
            })
            .await?
            .json::<ApiResponse<EmptyData>>()
            .await?;
            if resp.success {
                println!("Logout successful!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::GetRating { session_id } => {
            let resp = send_with_failover(&replicas, |base| {
                client.get(format!("{}/seller/{}/rating", base, session_id))
            })
            .await?
            .json::<ApiResponse<Feedback>>()
            .await?;
            if resp.success {
                let fb = resp.data.unwrap();
                println!("Seller Rating:");
                println!("  Thumbs Up: {}", fb.thumbs_up);
                println!("  Thumbs Down: {}", fb.thumbs_down);
                let total = fb.thumbs_up + fb.thumbs_down;
                if total > 0 {
                    let rating = (fb.thumbs_up as f64 / total as f64) * 100.0;
                    println!("  Rating: {:.1}%", rating);
                }
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::RegisterItem {
            session_id,
            name,
            category,
            keywords,
            condition,
            price,
            quantity,
        } => {
            let keywords: Vec<String> = keywords
                .into_iter()
                .map(|k| {
                    let k = k.trim().to_string();
                    if k.len() > 8 {
                        k[..8].to_string()
                    } else {
                        k
                    }
                })
                .take(5)
                .collect();
            let body = RegisterItemRequest {
                item_name: name,
                item_category: category,
                keywords,
                condition,
                sale_price: price,
                quantity,
            };
            let resp = send_with_failover(&replicas, |base| {
                client
                    .post(format!("{}/seller/{}/items", base, session_id))
                    .json(&body)
            })
            .await?
            .json::<ApiResponse<RegisterItemResponse>>()
            .await?;
            if resp.success {
                let data = resp.data.unwrap();
                println!("Item registered successfully!");
                println!("Item ID: {}", data.item_id);
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::ChangePrice {
            session_id,
            item_id,
            new_price,
        } => {
            let body = ChangePriceRequest { new_price };
            let resp = send_with_failover(&replicas, |base| {
                client
                    .put(format!(
                        "{}/seller/{}/items/{}/price",
                        base, session_id, item_id
                    ))
                    .json(&body)
            })
            .await?
            .json::<ApiResponse<EmptyData>>()
            .await?;
            if resp.success {
                println!("Price changed successfully!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::UpdateUnits {
            session_id,
            item_id,
            quantity,
        } => {
            let body = UpdateUnitsRequest { quantity };
            let resp = send_with_failover(&replicas, |base| {
                client
                    .put(format!(
                        "{}/seller/{}/items/{}/quantity",
                        base, session_id, item_id
                    ))
                    .json(&body)
            })
            .await?
            .json::<ApiResponse<EmptyData>>()
            .await?;
            if resp.success {
                println!("Units updated successfully!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::DisplayItems { session_id } => {
            let resp = send_with_failover(&replicas, |base| {
                client.get(format!("{}/seller/{}/items", base, session_id))
            })
            .await?
            .json::<ApiResponse<Vec<Item>>>()
            .await?;
            if resp.success {
                let items = resp.data.unwrap();
                if items.is_empty() {
                    println!("No items for sale.");
                } else {
                    println!("Your Items for Sale:");
                    println!("{:-<80}", "");
                    for item in items {
                        println!("Item ID: {}", item.item_id);
                        println!("  Name: {}", item.item_name);
                        println!("  Category: {}", item.item_category);
                        println!("  Keywords: {}", item.keywords.join(", "));
                        println!("  Condition: {:?}", item.condition);
                        println!("  Price: ${:.2}", item.sale_price);
                        println!("  Quantity: {}", item.quantity);
                        println!(
                            "  Feedback: ↑{} ↓{}",
                            item.feedback.thumbs_up, item.feedback.thumbs_down
                        );
                        println!("{:-<80}", "");
                    }
                }
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
    }

    Ok(())
}
