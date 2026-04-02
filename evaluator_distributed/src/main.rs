use clap::Parser;
use common::*;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task;

const OPS_PER_CLIENT: usize = 1000;
const NUM_RUNS: usize = 10;
const REQUEST_TIMEOUT_SECS: u64 = 10;

// ── Replica discovery ─────────────────────────────────────────────────────────

fn parse_addrs(env_key: &str, defaults: &str) -> Vec<String> {
    std::env::var(env_key)
        .unwrap_or_else(|_| defaults.to_string())
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn get_seller_replicas() -> Vec<String> {
    parse_addrs(
        "SELLER_SERVER_ADDRS",
        "http://127.0.0.1:8082,http://127.0.0.1:8088,http://127.0.0.1:8089,http://127.0.0.1:8090",
    )
}

fn get_buyer_replicas() -> Vec<String> {
    parse_addrs(
        "BUYER_SERVER_ADDRS",
        "http://127.0.0.1:8083,http://127.0.0.1:8084,http://127.0.0.1:8086,http://127.0.0.1:8087",
    )
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "evaluator_distributed")]
#[command(about = "Distributed performance evaluator for online marketplace (PA3)")]
struct Cli {
    #[arg(long, default_value_t = NUM_RUNS)]
    runs: usize,

    #[arg(long, default_value_t = OPS_PER_CLIENT)]
    ops: usize,
}

// ── Run result ────────────────────────────────────────────────────────────────

struct RunResult {
    total_duration: Duration,
    total_operations: usize,
    failed_operations: usize,
    total_response_time: Duration,
}

fn make_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .expect("failed to build reqwest client")
}

// ── Seller API helpers ────────────────────────────────────────────────────────

async fn seller_create_account(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/seller/create-account", base))
        .json(&CreateAccountRequest { name: name.to_string(), password: password.to_string() })
        .send()
        .await?
        .json::<ApiResponse<CreateAccountResponse>>()
        .await?;
    if resp.success { Ok(resp.data.unwrap().user_id) } else { Err(resp.error.unwrap_or_default().into()) }
}

async fn seller_login(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/seller/login", base))
        .json(&LoginRequest { name: name.to_string(), password: password.to_string() })
        .send()
        .await?
        .json::<ApiResponse<LoginResponse>>()
        .await?;
    if resp.success { Ok(resp.data.unwrap().session_id) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(resp.data.unwrap().item_id) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(resp.data.unwrap()) } else { Err(resp.error.unwrap_or_default().into()) }
}

async fn seller_change_price(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    item_id: &str,
    new_price: f64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .put(format!("{}/seller/{}/items/{}/price", base, session_id, item_id))
        .json(&ChangePriceRequest { new_price })
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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

// ── Buyer API helpers ─────────────────────────────────────────────────────────

async fn buyer_create_account(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/create-account", base))
        .json(&CreateAccountRequest { name: name.to_string(), password: password.to_string() })
        .send()
        .await?
        .json::<ApiResponse<CreateAccountResponse>>()
        .await?;
    if resp.success { Ok(resp.data.unwrap().user_id) } else { Err(resp.error.unwrap_or_default().into()) }
}

async fn buyer_login(
    client: &reqwest::Client,
    base: &str,
    name: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/login", base))
        .json(&LoginRequest { name: name.to_string(), password: password.to_string() })
        .send()
        .await?
        .json::<ApiResponse<LoginResponse>>()
        .await?;
    if resp.success { Ok(resp.data.unwrap().session_id) } else { Err(resp.error.unwrap_or_default().into()) }
}

async fn buyer_search(
    client: &reqwest::Client,
    base: &str,
    session_id: &str,
    keywords: Vec<String>,
) -> Result<Vec<Item>, Box<dyn std::error::Error + Send + Sync>> {
    let resp = client
        .post(format!("{}/buyer/{}/search", base, session_id))
        .json(&SearchRequest { category: None, keywords })
        .send()
        .await?
        .json::<ApiResponse<Vec<Item>>>()
        .await?;
    if resp.success { Ok(resp.data.unwrap()) } else { Err(resp.error.unwrap_or_default().into()) }
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
        .json(&CartOperationRequest { item_id: item_id.to_string(), quantity })
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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
        .json(&FeedbackRequest { item_id: item_id.to_string(), thumbs_up })
        .send()
        .await?
        .json::<ApiResponse<EmptyData>>()
        .await?;
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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
    if resp.success { Ok(()) } else { Err(resp.error.unwrap_or_default().into()) }
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

// ── Core workload ─────────────────────────────────────────────────────────────
//
// Each client pair is handed its assigned replica addresses. The caller is
// responsible for distributing clients across replicas (round-robin by index).

async fn run_client_workload(
    client_id: usize,
    run_id: usize,
    ops_per_client: usize,
    seller_base: String,
    buyer_base: String,
) -> (Duration, Duration, usize, usize) {   // (wall, resp_time, ok, failed)
    let client = make_client();
    let password = "password";
    let mut rng = StdRng::from_entropy();

    let seller_name = format!("eval_s_{}_{}_{}", run_id, client_id, rng.gen::<u32>());
    let buyer_name  = format!("eval_b_{}_{}_{}", run_id, client_id, rng.gen::<u32>());

    // ── Setup (not counted) ────────────────────────────────────────────────────
    macro_rules! setup {
        ($call:expr, $label:expr) => {
            match $call.await {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("  [client {}] {} failed on {}: {}", client_id, $label, seller_base, e);
                    return (Duration::ZERO, Duration::ZERO, 0, 0);
                }
            }
        };
    }

    setup!(seller_create_account(&client, &seller_base, &seller_name, password), "seller_create");
    let seller_session = setup!(seller_login(&client, &seller_base, &seller_name, password), "seller_login");
    setup!(buyer_create_account(&client, &buyer_base, &buyer_name, password), "buyer_create");
    let buyer_session = setup!(buyer_login(&client, &buyer_base, &buyer_name, password), "buyer_login");

    let mut item_ids = Vec::new();
    for j in 0..5 {
        if let Ok(id) = seller_register_item(
            &client, &seller_base, &seller_session,
            &format!("EvalItem_{}_{}", client_id, j),
            rng.gen_range(1..10), rng.gen_range(10.0..500.0), 1000,
        ).await {
            item_ids.push(id);
        }
    }

    // ── Measured operations ────────────────────────────────────────────────────
    let wall_start = Instant::now();
    let mut total_response_time = Duration::ZERO;
    let mut ops_ok: usize = 0;
    let mut ops_failed: usize = 0;

    // Op mix: 30% search, 15% register_item, 10% display_items, 10% change_price,
    //         10% add_to_cart, 5% display_cart, 5% clear_cart, 5% feedback,
    //         5% get_rating, 5% get_purchases
    for _ in 0..ops_per_client {
        let op = rng.gen_range(0..100u32);
        let t = Instant::now();

        let result: Result<(), _> = match op {
            0..30 => buyer_search(&client, &buyer_base, &buyer_session,
                        vec!["test".to_string(), "perf".to_string()])
                        .await.map(|_| ()),

            30..45 => seller_register_item(
                        &client, &seller_base, &seller_session,
                        &format!("Item_{}_{}_{}", run_id, client_id, ops_ok),
                        rng.gen_range(1..10), rng.gen_range(10.0..500.0), rng.gen_range(1..100),
                    ).await.map(|id| { item_ids.push(id); }),

            45..55 => seller_get_items(&client, &seller_base, &seller_session)
                        .await.map(|_| ()),

            55..65 => {
                if let Some(id) = item_ids.first().cloned() {
                    seller_change_price(&client, &seller_base, &seller_session,
                        &id, rng.gen_range(5.0..999.0)).await
                } else { Ok(()) }
            }

            65..75 => {
                if !item_ids.is_empty() {
                    let id = item_ids[rng.gen_range(0..item_ids.len())].clone();
                    buyer_add_to_cart(&client, &buyer_base, &buyer_session, &id, 1).await
                } else { Ok(()) }
            }

            75..80 => buyer_display_cart(&client, &buyer_base, &buyer_session).await,

            80..85 => buyer_clear_cart(&client, &buyer_base, &buyer_session).await,

            85..90 => {
                if let Some(id) = item_ids.first().cloned() {
                    buyer_provide_feedback(&client, &buyer_base, &buyer_session,
                        &id, rng.gen_bool(0.8)).await
                } else { Ok(()) }
            }

            90..95 => seller_get_rating(&client, &seller_base, &seller_session).await,

            _ => buyer_get_purchases(&client, &buyer_base, &buyer_session).await,
        };

        let elapsed = t.elapsed();
        match result {
            Ok(_) => { total_response_time += elapsed; ops_ok += 1; }
            Err(e) => {
                ops_failed += 1;
                if ops_failed <= 5 {
                    eprintln!("  [client {}] op failed (seller={}, buyer={}): {}", client_id, seller_base, buyer_base, e);
                }
            }
        }
    }

    let wall_duration = wall_start.elapsed();
    let _ = seller_logout(&client, &seller_base, &seller_session).await;
    let _ = buyer_logout(&client, &buyer_base, &buyer_session).await;

    (wall_duration, total_response_time, ops_ok, ops_failed)
}

// ── Cross-replica consistency test ────────────────────────────────────────────
//
// For each (seller_replica, buyer_replica) pair where seller ≠ buyer:
//   1. Seller registers an item on seller_replica.
//   2. Buyer searches on buyer_replica and expects to find it.
//
// This validates that product_db Raft replication and customer_db total-order
// broadcast propagate writes across all replicas before the search is issued.

async fn cross_replica_consistency_test(
    seller_replicas: &[String],
    buyer_replicas: &[String],
) {
    println!("\n=== Cross-Replica Consistency Test ===");
    println!(
        "  {} seller replica(s) × {} buyer replica(s)",
        seller_replicas.len(),
        buyer_replicas.len()
    );

    let client = make_client();
    let password = "xr_pass";
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut pair_index = 0usize;

    for seller_base in seller_replicas {
        // Create seller once per seller replica.
        let seller_name = format!("xr_seller_{}", pair_index);
        let seller_id_res = seller_create_account(&client, seller_base, &seller_name, password).await;
        let seller_login_res = seller_login(&client, seller_base, &seller_name, password).await;

        let seller_session = match (seller_id_res, seller_login_res) {
            (_, Ok(s)) => s,
            (_, Err(e)) => {
                eprintln!("  [xr] seller setup failed on {}: {}", seller_base, e);
                failed += buyer_replicas.len();
                continue;
            }
        };

        // Register a uniquely named item so the search keyword is unambiguous.
        let unique_kw = format!("xritem_{}", pair_index);
        let item_id = match seller_register_item(
            &client, seller_base, &seller_session,
            &format!("CrossReplicaItem_{}", pair_index),
            1, 42.0, 100,
        ).await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("  [xr] register_item failed on {}: {}", seller_base, e);
                failed += buyer_replicas.len();
                let _ = seller_logout(&client, seller_base, &seller_session).await;
                continue;
            }
        };

        // Register item with the unique keyword so search can find it.
        // (We re-register with the keyword since the helper hardcodes keywords;
        // instead we just search by item_id indirectly via keyword "xritem_N".)
        // Actually since RegisterItemRequest keywords are hardcoded to ["test","perf"],
        // we search by those and verify item_id appears in results.
        let _ = unique_kw; // suppress unused warning

        for buyer_base in buyer_replicas {
            // Skip same replica if only one replica exists.
            if seller_base == buyer_base && seller_replicas.len() == 1 {
                continue;
            }

            let buyer_name = format!("xr_buyer_{}", pair_index);
            let _ = buyer_create_account(&client, buyer_base, &buyer_name, password).await;
            let buyer_session = match buyer_login(&client, buyer_base, &buyer_name, password).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  [xr] buyer login failed on {}: {}", buyer_base, e);
                    failed += 1;
                    continue;
                }
            };

            // Search on the buyer replica — the item should be visible.
            let t = Instant::now();
            let search_result = buyer_search(
                &client, buyer_base, &buyer_session,
                vec!["test".to_string(), "perf".to_string()],
            ).await;
            let elapsed = t.elapsed();

            match search_result {
                Ok(items) => {
                    if items.iter().any(|i| i.item_id.to_string() == item_id) {
                        println!(
                            "  [PASS] seller={} buyer={} item found in {:.2?}",
                            seller_base, buyer_base, elapsed
                        );
                        passed += 1;
                    } else {
                        eprintln!(
                            "  [FAIL] seller={} buyer={} item {} NOT in search results ({} items returned)",
                            seller_base, buyer_base, item_id, items.len()
                        );
                        failed += 1;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "  [FAIL] seller={} buyer={} search error: {}",
                        seller_base, buyer_base, e
                    );
                    failed += 1;
                }
            }

            let _ = buyer_logout(&client, buyer_base, &buyer_session).await;
            pair_index += 1;
        }

        let _ = seller_logout(&client, seller_base, &seller_session).await;
    }

    println!(
        "\n  Cross-replica result: {} passed, {} failed",
        passed, failed
    );
    if failed == 0 {
        println!("  All writes were visible across replicas.");
    } else {
        println!("  WARNING: some writes were not visible across replicas.");
    }
}

// ── Reset all seller replicas ─────────────────────────────────────────────────

async fn reset_databases(seller_replicas: &[String]) {
    let client = make_client();
    for base in seller_replicas {
        match client.post(format!("{}/reset", base)).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    eprintln!("  Warning: reset on {} returned {}", base, resp.status());
                }
            }
            Err(e) => eprintln!("  Warning: reset on {} failed: {}", base, e),
        }
    }
    // Give the databases a moment to settle after reset.
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

// ── Scenario runner ───────────────────────────────────────────────────────────

async fn run_scenario(
    scenario_name: &str,
    num_clients: usize,
    num_runs: usize,
    ops_per_client: usize,
    seller_replicas: Arc<Vec<String>>,
    buyer_replicas: Arc<Vec<String>>,
) -> Vec<RunResult> {
    println!("\n=== {} ===", scenario_name);
    println!(
        "  {} client pair(s), {} ops/client, {} runs",
        num_clients, ops_per_client, num_runs
    );
    println!(
        "  Distributing across {} seller replica(s), {} buyer replica(s)",
        seller_replicas.len(), buyer_replicas.len()
    );

    let mut results = Vec::new();

    for run in 0..num_runs {
        print!("  Run {} / {}...", run + 1, num_runs);

        let mut handles = Vec::new();
        for i in 0..num_clients {
            // Round-robin each client to a different replica.
            let seller_base = seller_replicas[i % seller_replicas.len()].clone();
            let buyer_base  = buyer_replicas[i % buyer_replicas.len()].clone();
            handles.push(task::spawn(run_client_workload(
                i, run, ops_per_client, seller_base, buyer_base,
            )));
        }

        let mut max_wall = Duration::ZERO;
        let mut total_ops_ok = 0usize;
        let mut total_ops_failed = 0usize;
        let mut total_resp = Duration::ZERO;

        for handle in handles {
            if let Ok((wall, resp, ok, failed)) = handle.await {
                max_wall = max_wall.max(wall);
                total_ops_ok += ok;
                total_ops_failed += failed;
                total_resp += resp;
            }
        }

        let total_ops = total_ops_ok + total_ops_failed;
        let throughput = if max_wall.as_secs_f64() > 0.0 {
            total_ops_ok as f64 / max_wall.as_secs_f64()
        } else {
            0.0
        };
        let avg_resp = if total_ops_ok > 0 {
            total_resp / total_ops_ok as u32
        } else {
            Duration::ZERO
        };
        let fail_pct = if total_ops > 0 {
            100.0 * total_ops_failed as f64 / total_ops as f64
        } else {
            0.0
        };

        println!(
            " ok={}, failed={} ({:.1}%), wall={:.2?}, throughput={:.2} ops/s, avg_resp={:.2?}",
            total_ops_ok, total_ops_failed, fail_pct, max_wall, throughput, avg_resp
        );

        results.push(RunResult {
            total_duration: max_wall,
            total_operations: total_ops_ok,
            failed_operations: total_ops_failed,
            total_response_time: total_resp,
        });

        if run < num_runs - 1 {
            reset_databases(&seller_replicas).await;
        }
    }

    results
}

// ── Summary printer ───────────────────────────────────────────────────────────

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

    let total_ok: usize = results.iter().map(|r| r.total_operations).sum();
    let total_failed: usize = results.iter().map(|r| r.failed_operations).sum();
    let total_ops = total_ok + total_failed;
    let total_resp: Duration = results.iter().map(|r| r.total_response_time).sum();
    let avg_response_time = if total_ok > 0 { total_resp / total_ok as u32 } else { Duration::ZERO };
    let avg_wall: Duration =
        results.iter().map(|r| r.total_duration).sum::<Duration>() / results.len() as u32;
    let fail_pct = if total_ops > 0 { 100.0 * total_failed as f64 / total_ops as f64 } else { 0.0 };

    println!("{}:", name);
    println!("  Average Response Time:  {:.2?}", avg_response_time);
    println!("  Average Throughput:     {:.2} ops/sec", avg_throughput);
    println!("  Average Wall Time:      {:.2?}", avg_wall);
    println!(
        "  Total Operations:       {} ok / {} total ({:.1}% failures) across {} runs",
        total_ok, total_ops, fail_pct, results.len()
    );
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    let seller_replicas = Arc::new(get_seller_replicas());
    let buyer_replicas  = Arc::new(get_buyer_replicas());

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║   Online Marketplace Distributed Performance Evaluator (PA3) ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Seller replicas ({}):", seller_replicas.len());
    for addr in seller_replicas.iter() { println!("║    {}", addr); }
    println!("║  Buyer replicas ({}):", buyer_replicas.len());
    for addr in buyer_replicas.iter() { println!("║    {}", addr); }
    println!("║  Request timeout: {}s", REQUEST_TIMEOUT_SECS);
    println!("║  Runs per scenario: {}", cli.runs);
    println!("║  Ops per client:    {}", cli.ops);
    println!("╚══════════════════════════════════════════════════════════════╝");

    // ── Cross-replica consistency ─────────────────────────────────────────────
    reset_databases(&seller_replicas).await;
    cross_replica_consistency_test(&seller_replicas, &buyer_replicas).await;

    // ── Throughput scenarios ──────────────────────────────────────────────────
    reset_databases(&seller_replicas).await;
    let s1 = run_scenario(
        "Scenario 1: 1 seller, 1 buyer",
        1, cli.runs, cli.ops,
        seller_replicas.clone(), buyer_replicas.clone(),
    ).await;

    reset_databases(&seller_replicas).await;
    let s2 = run_scenario(
        "Scenario 2: 10 sellers, 10 buyers",
        10, cli.runs, cli.ops,
        seller_replicas.clone(), buyer_replicas.clone(),
    ).await;

    reset_databases(&seller_replicas).await;
    let s3 = run_scenario(
        "Scenario 3: 100 sellers, 100 buyers",
        100, cli.runs, cli.ops,
        seller_replicas.clone(), buyer_replicas.clone(),
    ).await;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                      Results Summary                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    print_summary("Scenario 1 (1×1)", &s1);
    println!();
    print_summary("Scenario 2 (10×10)", &s2);
    println!();
    print_summary("Scenario 3 (100×100)", &s3);

    println!("\n=== Notes ===");
    println!("- Clients are distributed round-robin across replicas.");
    println!("- Sessions persist across replicas via customer_db total-order broadcast.");
    println!("- Item visibility across replicas is guaranteed by product_db Raft replication.");
    println!("- Failure rate > 0 indicates a replica is down or replication lag exceeded the {}s timeout.", REQUEST_TIMEOUT_SECS);

    Ok(())
}
