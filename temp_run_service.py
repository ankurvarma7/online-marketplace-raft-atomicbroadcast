from python_customer_db.customer_db_service import start_server, NUM_NODES
import multiprocessing


def run_node(node_id: int):
    class Args:
        id = node_id

    try:
        start_server(Args())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    processes = []

    for i in range(NUM_NODES):
        p = multiprocessing.Process(target=run_node, args=(i,), name=f"node-{i}")
        p.start()
        processes.append(p)
        print(f"[Main] Started node {i} (gRPC port {50051 + i}, UDP port {50000 + i})")

    try:
        for p in processes:
            p.join()
    except KeyboardInterrupt:
        print("[Main] Shutting down all nodes...")
        for p in processes:
            p.terminate()
        for p in processes:
            p.join()
        print("[Main] All nodes stopped.")