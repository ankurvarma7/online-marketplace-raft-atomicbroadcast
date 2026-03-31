use clap::{Parser, Subcommand};
use common::*;

fn get_buyer_server_addr() -> String {
    std::env::var("BUYER_SERVER_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8083".to_string())
}

#[derive(Parser)]
#[command(name = "buyer_client")]
#[command(about = "Online Marketplace Buyer Client (REST)")]
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
    Search {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        category: Option<i32>,
        #[arg(short, long, num_args = 0..=5, value_delimiter = ',')]
        keywords: Vec<String>,
    },
    GetItem {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
    },
    AddToCart {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
        #[arg(short, long)]
        quantity: i32,
    },
    RemoveFromCart {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
        #[arg(short, long)]
        quantity: i32,
    },
    SaveCart {
        #[arg(short, long)]
        session_id: String,
    },
    ClearCart {
        #[arg(short, long)]
        session_id: String,
    },
    DisplayCart {
        #[arg(short, long)]
        session_id: String,
    },
    Feedback {
        #[arg(short, long)]
        session_id: String,
        #[arg(short, long)]
        item_id: String,
        #[arg(short, long)]
        thumbs_up: bool,
    },
    GetSellerRating {
        #[arg(short, long)]
        session_id: String,
        #[arg(long)]
        seller_id: String,
    },
    GetPurchases {
        #[arg(short, long)]
        session_id: String,
    },
    MakePurchase {
        #[arg(short, long)]
        session_id: String,
        #[arg(long)]
        card_name: String,
        #[arg(long)]
        card_number: String,
        #[arg(long)]
        expiration_date: String,
        #[arg(long)]
        security_code: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let base = get_buyer_server_addr();
    let client = reqwest::Client::new();

    match cli.command {
        Commands::CreateAccount { name, password } => {
            let resp = client
                .post(format!("{}/buyer/create-account", base))
                .json(&CreateAccountRequest { name, password })
                .send()
                .await?
                .json::<ApiResponse<CreateAccountResponse>>()
                .await?;
            if resp.success {
                let data = resp.data.unwrap();
                println!("Account created successfully!");
                println!("Buyer ID: {}", data.user_id);
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::Login { name, password } => {
            let resp = client
                .post(format!("{}/buyer/login", base))
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
                .post(format!("{}/buyer/{}/logout", base, session_id))
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
        Commands::Search {
            session_id,
            category,
            keywords,
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
                .post(format!("{}/buyer/{}/search", base, session_id))
                .json(&SearchRequest { category, keywords })
                .send()
                .await?
                .json::<ApiResponse<Vec<Item>>>()
                .await?;
            if resp.success {
                let items = resp.data.unwrap();
                if items.is_empty() {
                    println!("No items found.");
                } else {
                    println!("Search Results ({} items):", items.len());
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
        Commands::GetItem {
            session_id,
            item_id,
        } => {
            let resp = client
                .get(format!(
                    "{}/buyer/{}/items/{}",
                    base, session_id, item_id
                ))
                .send()
                .await?
                .json::<ApiResponse<Option<Item>>>()
                .await?;
            if resp.success {
                match resp.data.unwrap() {
                    Some(item) => {
                        println!("Item Details:");
                        println!("  ID: {}", item.item_id);
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
                    }
                    None => println!("Item not found."),
                }
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::AddToCart {
            session_id,
            item_id,
            quantity,
        } => {
            let resp = client
                .post(format!("{}/buyer/{}/cart/add", base, session_id))
                .json(&CartOperationRequest { item_id, quantity })
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Item added to cart!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::RemoveFromCart {
            session_id,
            item_id,
            quantity,
        } => {
            let resp = client
                .post(format!("{}/buyer/{}/cart/remove", base, session_id))
                .json(&CartOperationRequest { item_id, quantity })
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Item removed from cart!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::SaveCart { session_id } => {
            let resp = client
                .post(format!("{}/buyer/{}/cart/save", base, session_id))
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Cart saved!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::ClearCart { session_id } => {
            let resp = client
                .post(format!("{}/buyer/{}/cart/clear", base, session_id))
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Cart cleared!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::DisplayCart { session_id } => {
            let resp = client
                .get(format!("{}/buyer/{}/cart", base, session_id))
                .send()
                .await?
                .json::<ApiResponse<Vec<CartItem>>>()
                .await?;
            if resp.success {
                let cart = resp.data.unwrap();
                if cart.is_empty() {
                    println!("Cart is empty.");
                } else {
                    println!("Shopping Cart:");
                    println!("{:-<80}", "");
                    for item in &cart {
                        println!("Item ID: {}", item.item_id);
                        println!("  Quantity: {}", item.quantity);
                        println!("{:-<80}", "");
                    }
                    println!("Total items: {}", cart.len());
                    let total_qty: i32 = cart.iter().map(|i| i.quantity).sum();
                    println!("Total quantity: {}", total_qty);
                }
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::Feedback {
            session_id,
            item_id,
            thumbs_up,
        } => {
            let resp = client
                .post(format!("{}/buyer/{}/feedback", base, session_id))
                .json(&FeedbackRequest { item_id, thumbs_up })
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Feedback submitted!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::GetSellerRating {
            session_id,
            seller_id,
        } => {
            let resp = client
                .get(format!(
                    "{}/buyer/{}/seller-rating/{}",
                    base, session_id, seller_id
                ))
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
        Commands::GetPurchases { session_id } => {
            let resp = client
                .get(format!("{}/buyer/{}/purchases", base, session_id))
                .send()
                .await?
                .json::<ApiResponse<Vec<String>>>()
                .await?;
            if resp.success {
                let history = resp.data.unwrap();
                if history.is_empty() {
                    println!("No purchase history.");
                } else {
                    println!("Purchase History ({} items):", history.len());
                    for (i, item_id) in history.iter().enumerate() {
                        println!("{}. Item ID: {}", i + 1, item_id);
                    }
                }
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
        Commands::MakePurchase {
            session_id,
            card_name,
            card_number,
            expiration_date,
            security_code,
        } => {
            let resp = client
                .post(format!("{}/buyer/{}/purchase", base, session_id))
                .json(&MakePurchaseRequest {
                    credit_card_name: card_name,
                    credit_card_number: card_number,
                    expiration_date,
                    security_code,
                })
                .send()
                .await?
                .json::<ApiResponse<EmptyData>>()
                .await?;
            if resp.success {
                println!("Purchase completed successfully!");
            } else {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
            }
        }
    }

    Ok(())
}
