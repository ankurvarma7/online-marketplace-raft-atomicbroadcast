use clap::Parser;
use common::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::time::{Duration, Instant};
use tokio::task;

const OPS_PER_CLIENT: usize = 1000;
const NUM_RUNS: usize = 10;

fn get_seller_server_addr() -> String {
    std::env::var("SELLER_SERVER_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string())
}

fn get_buyer_server_addr() -> String {
    std::env::var("BUYER_SERVER_ADDR").unwrap_or_else(|_| "http://127.0.0.1:8083".to_string())
}

#[derive(Parser)]
#[command(name = "evaluator")]
#[command(about = "Performance evaluator for online marketplace (PA2)")]
struct Cli {
    #[arg(long, default_value_t = NUM_RUNS)]
    runs: usize,

    #[arg(long, default_value_t = OPS_PER_CLIENT)]
    ops: usize,
}

struct RunResult {
    total_duration: Duration,
    total_operations: usize,
    total_response_time: Duration,
}

// ── Seller API helpers ──

async fn seller_create_account(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/seller/create-account", base))
        .json(&CreateAccountRequest {
            name: name.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?
        .json::<ApiResponse<CreateAccountResponse>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap().user_id)
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_login(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/seller/login", base))
        .json(&LoginRequest {
            name: name.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?
        .json::<ApiResponse<LoginResponse>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap().session_id)
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_register_item(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    name: &str,
    category: i32,
    price: f64,
    quantity: i32,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/seller/{}/items", base, session_id))
        .json(&RegisterItemRequest {
            item_name: name.to_string(),
            item_category: category,
            keywords: vec!["test".to_string(), "perf".to_string()],
            condition: "New".to_string(),
            sale_price: price,
            quantity,
        })
        .send()
        .await?
        .json::<ApiResponse<RegisterItemResponse>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap().item_id)
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_get_items(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .get(format!("{}/seller/{}/items", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<Vec<Item>>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_change_price(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    item_id: &str,
    new_price: f64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_get_rating(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .get(format!("{}/seller/{}/rating", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<Feedback>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn seller_logout(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = client
        .post(format!("{}/seller/{}/logout", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    Ok(())
}

// ── Buyer API helpers ──

async fn buyer_create_account(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/create-account", base))
        .json(&CreateAccountRequest {
            name: name.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?
        .json::<ApiResponse<CreateAccountResponse>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap().user_id)
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_login(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/login", base))
        .json(&LoginRequest {
            name: name.to_string(),
            password: password.to_string(),
        })
        .send()
        .await?
        .json::<ApiResponse<LoginResponse>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap().session_id)
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_search(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    keywords: Vec<String>,
) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/{}/search", base, session_id))
        .json(&SearchRequest {
            category: None,
            keywords,
        })
        .send()
        .await?
        .json::<ApiResponse<Vec<Item>>>()
        .await?;
    if resp.success {
        Ok(resp.data.unwrap())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_add_to_cart(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    item_id: &str,
    quantity: i32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/{}/cart/add", base, session_id))
        .json(&CartOperationRequest {
            item_id: item_id.to_string(),
            quantity,
        })
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_display_cart(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .get(format!("{}/buyer/{}/cart", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<Vec<CartItem>>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_clear_cart(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/{}/cart/clear", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_provide_feedback(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    item_id: &str,
    thumbs_up: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/{}/feedback", base, session_id))
        .json(&FeedbackRequest {
            item_id: item_id.to_string(),
            thumbs_up,
        })
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_get_purchases(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .get(format!("{}/buyer/{}/purchases", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<Vec<String>>>()
        .await?;
    if resp.success {
        Ok(())
    } else {
        Err(resp.error.unwrap_or_default().into())
    }
}

async fn buyer_logout(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _ = client
        .post(format!("{}/buyer/{}/logout", base, session_id))
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    Ok(())
}

// ── Core workload: each seller+buyer pair does `ops_per_client` API calls ──

async fn run_client_workload(
    client_id: usize,
    run_id: usize,
    ops_per_client: usize,
) -> (Duration, Duration, usize) {
    let client = reqwest::Client::new();
    let seller_base = get_seller_server_addr();
    let buyer_base = get_buyer_server_addr();
    let password = "password";
    let mut rng = StdRng::from_entropy();

    let seller_name = format!("eval_s_{}_{}_{}", run_id, client_id, rng.gen::<u32>());
    let buyer_name = format!("eval_b_{}_{}_{}", run_id, client_id, rng.gen::<u32>());

    // Setup: create accounts and login (not counted toward measured operations)
    let _seller_id = match seller_create_account(&client, &seller_base, &seller_name, password).await
    {
        Ok(id) => id,
        Err(e) => {
            eprintln!("  [client {}] seller create failed: {}", client_id, e);
            return (Duration::ZERO, Duration::ZERO, 0);
        }
    };
    let seller_session = match seller_login(&client, &seller_base, &seller_name, password).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  [client {}] seller login failed: {}", client_id, e);
            return (Duration::ZERO, Duration::ZERO, 0);
        }
    };
    let _buyer_id = match buyer_create_account(&client, &buyer_base, &buyer_name, password).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("  [client {}] buyer create failed: {}", client_id, e);
            return (Duration::ZERO, Duration::ZERO, 0);
        }
    };
    let buyer_session = match buyer_login(&client, &buyer_base, &buyer_name, password).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  [client {}] buyer login failed: {}", client_id, e);
            return (Duration::ZERO, Duration::ZERO, 0);
        }
    };

    // Register initial items for buyer to interact with
    let mut item_ids = Vec::new();
    for j in 0..5 {
        if let Ok(id) = seller_register_item(
            &client,
            &seller_base,
            &seller_session,
            &format!("EvalItem_{}_{}", client_id, j),
            rng.gen_range(1..10),
            rng.gen_range(10.0..500.0),
            1000,
        )
        .await
        {
            item_ids.push(id);
        }
    }

    // ── Measured operations start here ──
    let wall_start = Instant::now();
    let mut total_response_time = Duration::ZERO;
    let mut ops_completed: usize = 0;

    // Distribute ops_per_client across API types with a realistic mix:
    //   30% search, 15% register_item, 10% display_items, 10% change_price,
    //   10% add_to_cart, 5% display_cart, 5% clear_cart, 5% feedback,
    //   5% get_rating, 5% get_purchases
    for _ in 0..ops_per_client {
        let op = rng.gen_range(0..100);
        let t = Instant::now();
        let ok = match op {
            0..30 => {
                let kw = vec!["test".to_string(), "perf".to_string()];
                buyer_search(&client, &buyer_base, &buyer_session, kw)
                    .await
                    .map(|_| ())
                    .is_ok()
            }
            30..45 => seller_register_item(
                &client,
                &seller_base,
                &seller_session,
                &format!("Item_{}_{}_{}", run_id, client_id, ops_completed),
                rng.gen_range(1..10),
                rng.gen_range(10.0..500.0),
                rng.gen_range(1..100),
            )
            .await
            .map(|id| {
                item_ids.push(id);
            })
            .is_ok(),
            45..55 => seller_get_items(&client, &seller_base, &seller_session)
                .await
                .is_ok(),
            55..65 => {
                if let Some(id) = item_ids.first() {
                    seller_change_price(
                        &client,
                        &seller_base,
                        &seller_session,
                        id,
                        rng.gen_range(5.0..999.0),
                    )
                    .await
                    .is_ok()
                } else {
                    true
                }
            }
            65..75 => {
                if let Some(id) = item_ids.get(rng.gen_range(0..item_ids.len().max(1))) {
                    buyer_add_to_cart(&client, &buyer_base, &buyer_session, id, 1)
                        .await
                        .is_ok()
                } else {
                    true
                }
            }
            75..80 => buyer_display_cart(&client, &buyer_base, &buyer_session)
                .await
                .is_ok(),
            80..85 => buyer_clear_cart(&client, &buyer_base, &buyer_session)
                .await
                .is_ok(),
            85..90 => {
                if let Some(id) = item_ids.first() {
                    buyer_provide_feedback(
                        &client,
                        &buyer_base,
                        &buyer_session,
                        id,
                        rng.gen_bool(0.8),
                    )
                    .await
                    .is_ok()
                } else {
                    true
                }
            }
            90..95 => seller_get_rating(&client, &seller_base, &seller_session)
                .await
                .is_ok(),
            _ => buyer_get_purchases(&client, &buyer_base, &buyer_session)
                .await
                .is_ok(),
        };

        let elapsed = t.elapsed();
        if ok {
            total_response_time += elapsed;
            ops_completed += 1;
        }
    }

    let wall_duration = wall_start.elapsed();

    // Cleanup
    let _ = seller_logout(&client, &seller_base, &seller_session).await;
    let _ = buyer_logout(&client, &buyer_base, &buyer_session).await;

    (wall_duration, total_response_time, ops_completed)
}

async fn reset_databases(seller_server_addr: &str) {
    let client = reqwest::Client::new();
    match client
        .post(format!("{}/reset", seller_server_addr))
        .send()
        .await
    {
        Ok(resp) => {
            if resp.status().is_success() {
                println!("  Databases reset successfully.");
            } else {
                eprintln!("  Warning: reset returned status {}", resp.status());
            }
        }
        Err(e) => eprintln!("  Warning: failed to reset databases: {}", e),
    }
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

async fn run_scenario(
    scenario_name: &str,
    num_clients: usize,
    num_runs: usize,
    ops_per_client: usize,
) -> Vec<RunResult> {
    println!("\n=== {} ===", scenario_name);
    println!(
        "  {} client pair(s), {} ops/client, {} runs",
        num_clients, ops_per_client, num_runs
    );

    let mut results = Vec::new();

    for run in 0..num_runs {
        print!("  Run {} / {}...", run + 1, num_runs);

        let mut handles = Vec::new();
        for i in 0..num_clients {
            handles.push(task::spawn(run_client_workload(i, run, ops_per_client)));
        }

        let mut max_wall = Duration::ZERO;
        let mut total_ops = 0usize;
        let mut total_resp = Duration::ZERO;

        for handle in handles {
            if let Ok((wall, resp, ops)) = handle.await {
                max_wall = max_wall.max(wall);
                total_ops += ops;
                total_resp += resp;
            }
        }

        let throughput = if max_wall.as_secs_f64() > 0.0 {
            total_ops as f64 / max_wall.as_secs_f64()
        } else {
            0.0
        };

        let avg_resp = if total_ops > 0 {
            total_resp / total_ops as u32
        } else {
            Duration::ZERO
        };

        println!(
            " ops={}, wall={:.2?}, throughput={:.2} ops/s, avg_resp={:.2?}",
            total_ops, max_wall, throughput, avg_resp
        );

        results.push(RunResult {
            total_duration: max_wall,
            total_operations: total_ops,
            total_response_time: total_resp,
        });

        if run < num_runs - 1 {
            reset_databases(&get_seller_server_addr()).await;
        }
    }

    results
}

fn print_summary(name: &str, results: &[RunResult]) {
    let n = results.len() as f64;

    let avg_throughput: f64 = results
        .iter()
        .map(|r| {
            if r.total_duration.as_secs_f64() > 0.0 {
                r.total_operations as f64 / r.total_duration.as_secs_f64()
            } else {
                0.0
            }
        })
        .sum::<f64>()
        / n;

    let total_ops: usize = results.iter().map(|r| r.total_operations).sum();
    let total_resp: Duration = results.iter().map(|r| r.total_response_time).sum();
    let avg_response_time = if total_ops > 0 {
        total_resp / total_ops as u32
    } else {
        Duration::ZERO
    };

    let avg_wall: Duration =
        results.iter().map(|r| r.total_duration).sum::<Duration>() / results.len() as u32;

    println!("{}:", name);
    println!("  Average Response Time:  {:.2?}", avg_response_time);
    println!("  Average Throughput:     {:.2} ops/sec", avg_throughput);
    println!("  Average Wall Time:      {:.2?}", avg_wall);
    println!(
        "  Total Operations:       {} across {} runs",
        total_ops,
        results.len()
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║    Online Marketplace Performance Evaluator (PA2)       ║");
    println!("╠══════════════════════════════════════════════════════════╣");
    println!("║  Seller Server: {:<40} ║", get_seller_server_addr());
    println!("║  Buyer Server:  {:<40} ║", get_buyer_server_addr());
    println!("║  Runs per scenario: {:<36} ║", cli.runs);
    println!("║  Ops per client:    {:<36} ║", cli.ops);
    println!("╚══════════════════════════════════════════════════════════╝");

    reset_databases(&get_seller_server_addr()).await;
    let s1 = run_scenario(
        "Scenario 1: 1 seller, 1 buyer",
        1,
        cli.runs,
        cli.ops,
    )
    .await;

    reset_databases(&get_seller_server_addr()).await;
    let s2 = run_scenario(
        "Scenario 2: 10 sellers, 10 buyers",
        10,
        cli.runs,
        cli.ops,
    )
    .await;

    reset_databases(&get_seller_server_addr()).await;
    let s3 = run_scenario(
        "Scenario 3: 100 sellers, 100 buyers",
        100,
        cli.runs,
        cli.ops,
    )
    .await;

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                   Results Summary                       ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    print_summary("Scenario 1 (1x1)", &s1);
    println!();
    print_summary("Scenario 2 (10x10)", &s2);
    println!();
    print_summary("Scenario 3 (100x100)", &s3);

    println!("\n=== Performance Analysis ===");
    println!("1. Scenario 1 shows baseline REST→gRPC performance with no contention.");
    println!("2. Scenario 2 tests moderate concurrency; HTTP/2 multiplexing in gRPC helps backend.");
    println!("3. Scenario 3 stress-tests with 100 concurrent client pairs (200 total users).");
    println!("\nComparison with PA1 (TCP):");
    println!("- REST adds HTTP overhead (headers, JSON serialization) vs raw TCP.");
    println!("- gRPC (HTTP/2) enables multiplexed backend calls, reducing connection overhead.");
    println!("- Cloud deployment adds network latency between VMs vs single-machine PA1.");

    Ok(())
}
