╔══════════════════════════════════════════════════════════════╗
║                      Results Summary                         ║
╚══════════════════════════════════════════════════════════════╝

Scenario 1 (1×1):
  Average Response Time:  4.72ms
  Average Throughput:     0.64 ops/sec
  Average Wall Time:      15.51s
  Total Operations:       83 ok / 100 total (17.0% failures) across 10 runs

Scenario 2 (10×10):
  Average Response Time:  9.20ms
  Average Throughput:     86.61 ops/sec
  Average Wall Time:      496.01ms
  Total Operations:       288 ok / 1000 total (71.2% failures) across 10 runs

Scenario 3 (100×100):
  Average Response Time:  111.37ms
  Average Throughput:     66.64 ops/sec
  Average Wall Time:      4.54s
  Total Operations:       3007 ok / 10000 total (69.9% failures) across 10 runs

=== Notes ===
- Clients are distributed round-robin across replicas.
- Sessions persist across replicas via customer_db total-order broadcast.
- Item visibility across replicas is guaranteed by product_db Raft replication.
- Failure rate > 0 indicates a replica is down or replication lag exceeded the 10s timeout.