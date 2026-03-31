use clap::{Parser, Subcommand};
use common::*;

fn get_seller_server_addr() -> String {
    std::env::var("SELLER_SERVER_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string())
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
    let base = get_seller_server_addr();
    let client = reqwest::Client::new();

    match cli.command {
        Commands::CreateAccount { name, password } => {
            let resp = client
                .post(format!("{}/seller/create-account", base))
                .json(&CreateAccountRequest { name, password })
                .send()
                .await?
                .json::<ApiResponse<CreateAccountResponse>>()
                .await?;
            if resp.success {
                let data = resp.data.unwrap();
                println!("Account created successfully!");
                println!("Seller ID: {}", data.user_id);
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::Login { name, password } => {
            let resp = client
                .post(format!("{}/seller/login", base))
                .json(&LoginRequest { name, password })
                .send()
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
            let resp = client
                .post(format!("{}/seller/{}/logout", base, session_id))
                .send()
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
            let resp = client
                .get(format!("{}/seller/{}/rating", base, session_id))
                .send()
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

            let resp = client
                .post(format!("{}/seller/{}/items", base, session_id))
                .json(&RegisterItemRequest {
                    item_name: name,
                    item_category: category,
                    keywords,
                    condition,
                    sale_price: price,
                    quantity,
                })
                .send()
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
            let resp = client
                .put(format!(
                    "{}/seller/{}/items/{}/price",
                    base, session_id, item_id
                ))
                .json(&ChangePriceRequest { new_price })
                .send()
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
            let resp = client
                .put(format!(
                    "{}/seller/{}/items/{}/quantity",
                    base, session_id, item_id
                ))
                .json(&UpdateUnitsRequest { quantity })
                .send()
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
            let resp = client
                .get(format!("{}/seller/{}/items", base, session_id))
                .send()
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
