import grpc
from concurrent import futures
import socket
import threading
import queue
import argparse
import os
import sys
import uuid
import time

import pymysql
import pymysql.cursors

# The generated stubs import `customer_db_pb2` by bare name.
# Add the grpc_stubs directory to sys.path so those imports resolve.
_STUBS_DIR = os.path.join(os.path.dirname(__file__), "grpc_stubs")
if _STUBS_DIR not in sys.path:
    sys.path.insert(0, _STUBS_DIR)

from customer_db_pb2_grpc import CustomerDatabaseServicer, add_CustomerDatabaseServicer_to_server
from customer_db_pb2 import (
    CreateSellerResponse, CreateBuyerResponse,
    SellerResponse, BuyerResponse, GenericResponse,
    CreateSessionResponse, SessionResponse,
    Seller, Buyer, Session, Feedback,
)
from message_struct_pb2 import (
    GeneralMessage, RequestMessage, ServiceMessage,
    RequestMetadata, ServiceMetadata,
    RetransmitRequestMessage, RetransmitServiceMessage,
)

GRPC_PORT = 50051
NUM_NODES = 5
DELIVER_TIMEOUT = 20.0   # seconds a gRPC handler waits for delivery
SESSION_TTL = 300         # seconds, matching the Rust implementation

CUSTOMER_DB_BASE_PORT = int(os.getenv("CUSTOMER_DB_BASE_PORT", 8000))
CUSTOMER_DB_HOST = os.getenv("CUSTOMER_DB_HOST", "127.0.0.1")
CUSTOMER_DB_PASSWORD = os.getenv("CUSTOMER_DB_PASSWORD", "my-secret-pw")

IP_ADD_MAP = {
    0: {'IP': '127.0.0.1', 'PORT': 50000},
    1: {'IP': '127.0.0.1', 'PORT': 50001},
    2: {'IP': '127.0.0.1', 'PORT': 50002},
    3: {'IP': '127.0.0.1', 'PORT': 50003},
    4: {'IP': '127.0.0.1', 'PORT': 50004},
}


# ─────────────────────────────────────────────────────────
# MySQL helpers
# ─────────────────────────────────────────────────────────

def create_db_connection(node_id: int):
    port = CUSTOMER_DB_BASE_PORT + node_id
    try:
        conn = pymysql.connect(
            host=CUSTOMER_DB_HOST,
            port=port,
            user="root",
            password=CUSTOMER_DB_PASSWORD,
            database="customer_db",
            cursorclass=pymysql.cursors.DictCursor,
        )
        print(f"[DB] Node {node_id} connected to customer_db on port {port}")
        return conn
    except pymysql.Error as e:
        print(f"[DB] Node {node_id} connection failed on port {port}: {e}")
        return None


def _det_uuid(sender_id: int, local_seq: int) -> str:
    """Deterministic UUID so all replicas generate the same ID for the same request."""
    return str(uuid.uuid5(uuid.NAMESPACE_OID, f"node{sender_id}:seq{local_seq}"))


# ─────────────────────────────────────────────────────────
# DB execution — called from delivery_executor thread only
# ─────────────────────────────────────────────────────────

def execute_request(req: RequestMessage, conn) -> object:
    body = req.WhichOneof("request_body")
    det_id = _det_uuid(req.sender_id, req.local_sequence_num)
    try:
        if body == "create_seller_request":
            return _exec_create_seller(req.create_seller_request, det_id, conn)
        elif body == "create_buyer_request":
            return _exec_create_buyer(req.create_buyer_request, det_id, conn)
        elif body == "update_seller_request":
            return _exec_update_seller(req.update_seller_request, conn)
        elif body == "update_buyer_request":
            return _exec_update_buyer(req.update_buyer_request, conn)
        elif body == "create_session_request":
            return _exec_create_session(req.create_session_request, det_id, conn)
        elif body == "delete_session_request":
            return _exec_delete_session(req.delete_session_request, conn)
        elif body == "reset_all_request":
            return _exec_reset_all(conn)
        else:
            print(f"[DB] Unknown request body: {body}")
            return None
    except pymysql.Error as e:
        print(f"[DB] Error executing {body}: {e}")
        conn.rollback()
        return None


def _exec_create_seller(req, seller_id: str, conn) -> CreateSellerResponse:
    try:
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO sellers (id, name, password, thumbs_up, thumbs_down, items_sold) "
                "VALUES (%s, %s, %s, 0, 0, 0)",
                (seller_id, req.seller_name, req.password)
            )
        conn.commit()
        print(f"[DB] CreateSeller: {req.seller_name} id={seller_id}")
        return CreateSellerResponse(seller_id=seller_id)
    except pymysql.IntegrityError:
        conn.rollback()
        return CreateSellerResponse(seller_id=seller_id)


def _exec_create_buyer(req, buyer_id: str, conn) -> CreateBuyerResponse:
    try:
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO buyers (id, name, password, items_purchased) VALUES (%s, %s, %s, 0)",
                (buyer_id, req.buyer_name, req.password)
            )
        conn.commit()
        print(f"[DB] CreateBuyer: {req.buyer_name} id={buyer_id}")
        return CreateBuyerResponse(buyer_id=buyer_id)
    except pymysql.IntegrityError:
        conn.rollback()
        return CreateBuyerResponse(buyer_id=buyer_id)


def _exec_update_seller(req, conn) -> GenericResponse:
    seller = req.seller
    feedback = seller.feedback if seller.HasField("feedback") else None
    thumbs_up = feedback.thumbs_up if feedback else 0
    thumbs_down = feedback.thumbs_down if feedback else 0
    with conn.cursor() as cur:
        cur.execute(
            "UPDATE sellers SET thumbs_up = %s, thumbs_down = %s, items_sold = %s WHERE id = %s",
            (thumbs_up, thumbs_down, seller.items_sold, seller.seller_id)
        )
    conn.commit()
    return GenericResponse(success=True, message="Seller updated successfully")


def _exec_update_buyer(req, conn) -> GenericResponse:
    buyer = req.buyer
    with conn.cursor() as cur:
        cur.execute(
            "UPDATE buyers SET items_purchased = %s WHERE id = %s",
            (buyer.items_purchased, buyer.buyer_id)
        )
    conn.commit()
    return GenericResponse(success=True, message="Buyer updated successfully")


def _exec_create_session(req, session_id: str, conn) -> CreateSessionResponse:
    expiration = int(time.time()) + SESSION_TTL
    try:
        with conn.cursor() as cur:
            cur.execute(
                "INSERT INTO sessions (id, user_id, user_type, expiration) VALUES (%s, %s, %s, %s)",
                (session_id, req.user_id, req.user_type, expiration)
            )
        conn.commit()
        print(f"[DB] CreateSession: user={req.user_id} session_id={session_id}")
        return CreateSessionResponse(session_id=session_id, expiration=expiration)
    except pymysql.IntegrityError:
        conn.rollback()
        with conn.cursor() as cur:
            cur.execute("SELECT expiration FROM sessions WHERE id = %s", (session_id,))
            row = cur.fetchone()
        existing_exp = row["expiration"] if row else expiration
        return CreateSessionResponse(session_id=session_id, expiration=existing_exp)


def _exec_delete_session(req, conn) -> GenericResponse:
    with conn.cursor() as cur:
        cur.execute("DELETE FROM sessions WHERE id = %s", (req.session_id,))
    conn.commit()
    return GenericResponse(success=True, message="Session deleted successfully")


def _exec_reset_all(conn) -> GenericResponse:
    with conn.cursor() as cur:
        cur.execute("DELETE FROM sellers")
        cur.execute("DELETE FROM buyers")
        cur.execute("DELETE FROM sessions")
    conn.commit()
    return GenericResponse(success=True, message="All customer data cleared")


# ─────────────────────────────────────────────────────────
# Result holder for blocking gRPC handlers
# ─────────────────────────────────────────────────────────

class ResultHolder:
    def __init__(self):
        self.response = None


# ─────────────────────────────────────────────────────────
# Rotating sequencer atomic broadcast state
# ─────────────────────────────────────────────────────────

class NodeState:
    def __init__(self, node_id: int, n: int, udp_sock: socket.socket):
        self.node_id = node_id
        self.n = n
        self.udp_sock = udp_sock
        self.lock = threading.Lock()

        self.local_counter = 0
        self.pending_requests: dict = {}       # (sender_id, local_seq) -> RequestMessage
        self.all_requests: dict = {}           # same, never deleted
        self.received_service_msgs: dict = {}  # global_seq -> ServiceMessage
        self.seq_to_request: dict = {}         # global_seq -> (sender_id, local_seq)
        self.request_to_seq: dict = {}         # (sender_id, local_seq) -> global_seq
        self.received_from_sender: dict = {}   # sender_id -> set of local_seqs
        self.last_delivered_seq: int = -1
        self.node_last_delivered: dict = {i: -1 for i in range(n)}
        self.next_seq_to_assign: int = node_id
        self.pending_responses: dict = {}      # (sender_id, local_seq) -> (Event, ResultHolder)
        self.delivery_queue: queue.Queue = queue.Queue()

    def next_local_seq(self) -> int:
        with self.lock:
            self.local_counter += 1
            return self.local_counter

    def broadcast_request(self, req_msg: RequestMessage):
        self._broadcast_request(req_msg)

    def register_pending_response(self, local_seq: int, event: threading.Event, holder: ResultHolder):
        with self.lock:
            self.pending_responses[(self.node_id, local_seq)] = (event, holder)

    def on_request_message(self, req: RequestMessage):
        with self.lock:
            key = (req.sender_id, req.local_sequence_num)
            if key in self.all_requests:
                return

            self.all_requests[key] = req
            self.pending_requests[key] = req

            if req.sender_id not in self.received_from_sender:
                self.received_from_sender[req.sender_id] = set()
            self.received_from_sender[req.sender_id].add(req.local_sequence_num)

            if req.HasField('metadata'):
                self.node_last_delivered[req.sender_id] = max(
                    self.node_last_delivered.get(req.sender_id, -1),
                    req.metadata.global_sequence
                )

            for gs, req_key in self.seq_to_request.items():
                if req_key == key:
                    self.request_to_seq[key] = gs
                    break

            self._detect_missing_requests(req.sender_id, req.local_sequence_num)
            self._try_sequence()
            self._try_deliver()

    def on_service_message(self, svc: ServiceMessage):
        with self.lock:
            global_seq = svc.global_id
            if global_seq in self.received_service_msgs:
                return

            self.received_service_msgs[global_seq] = svc
            req_key = (svc.source_node_id, svc.local_seq_num)
            self.seq_to_request[global_seq] = req_key

            if req_key in self.all_requests:
                self.request_to_seq[req_key] = global_seq

            if svc.HasField('metadata'):
                self.node_last_delivered[svc.sender_id] = max(
                    self.node_last_delivered.get(svc.sender_id, -1),
                    svc.metadata.last_delivered_seq
                )

            self._detect_missing_seqs(global_seq)
            self._try_sequence()
            self._try_deliver()

    def on_retransmit_request(self, source_node_id: int, request_sequence_num: int):
        with self.lock:
            key = (source_node_id, request_sequence_num)
            if key not in self.all_requests:
                return
            req = self.all_requests[key]
        self._broadcast_request(req)

    def on_retransmit_seq(self, global_seq: int):
        with self.lock:
            if global_seq not in self.received_service_msgs:
                return
            svc = self.received_service_msgs[global_seq]
        self._broadcast_service(svc)

    def _try_sequence(self):
        k = self.next_seq_to_assign
        if k % self.n != self.node_id:
            return

        for j in range(k):
            if j not in self.received_service_msgs:
                return

        for j in range(k):
            if j in self.seq_to_request:
                if self.seq_to_request[j] not in self.all_requests:
                    return

        chosen_key = self._choose_request()
        if chosen_key is None:
            return

        sender_id, local_seq = chosen_key
        self.seq_to_request[k] = chosen_key
        self.request_to_seq[chosen_key] = k
        self.next_seq_to_assign += self.n

        svc = ServiceMessage(
            source_node_id=sender_id,
            local_seq_num=local_seq,
            global_id=k,
            sender_id=self.node_id,
            metadata=ServiceMetadata(last_delivered_seq=self.last_delivered_seq),
        )
        self.received_service_msgs[k] = svc
        self._broadcast_service(svc)
        print(f"[SEQ] node={self.node_id} assigned global_seq={k} to ({sender_id},{local_seq})")
        self._try_deliver()

    def _choose_request(self):
        for key in self.pending_requests:
            if key in self.request_to_seq:
                continue
            sender_id, local_seq = key
            prior_all_assigned = all(
                (sender_id, ls) in self.request_to_seq
                for ls in range(1, local_seq)
            )
            if prior_all_assigned:
                return key
        return None

    def _try_deliver(self):
        s = self.last_delivered_seq + 1
        if s not in self.received_service_msgs:
            return
        if s not in self.seq_to_request:
            return
        req_key = self.seq_to_request[s]
        if req_key not in self.pending_requests:
            return
        if not self._majority_confirmed(s):
            return

        req = self.pending_requests.pop(req_key)
        self.last_delivered_seq = s
        self.node_last_delivered[self.node_id] = s

        body = req.WhichOneof("request_body")
        print(f"[DELIVER] global_seq={s} sender={req.sender_id} local_seq={req.local_sequence_num} body={body}")

        pending = self.pending_responses.pop(req_key, None)
        self.delivery_queue.put((req, pending))
        self._try_deliver()

    def _majority_confirmed(self, global_seq: int) -> bool:
        majority = self.n // 2 + 1
        return sum(1 for v in self.node_last_delivered.values() if v >= global_seq - 1) >= majority

    def _detect_missing_requests(self, sender_id: int, received_local_seq: int):
        received = self.received_from_sender.get(sender_id, set())
        for ls in range(1, received_local_seq):
            if ls not in received and (sender_id, ls) not in self.request_to_seq:
                self._send_retransmit_request_message(sender_id, ls)

    def _detect_missing_seqs(self, received_global_seq: int):
        for gs in range(max(0, self.last_delivered_seq + 1), received_global_seq):
            if gs not in self.received_service_msgs:
                self._send_retransmit_service_message(gs % self.n, gs)

    def _send_retransmit_request_message(self, target_node: int, local_sequence: int):
        ip = IP_ADD_MAP[target_node]['IP']
        port = IP_ADD_MAP[target_node]['PORT']
        retransmit = RetransmitRequestMessage(
            request_node_id=self.node_id,
            source_node_id=target_node,
            request_sequence_num=local_sequence
        )
        packet = GeneralMessage()
        packet.retransmit_request_message.CopyFrom(retransmit)
        try:
            self.udp_sock.sendto(packet.SerializeToString(), (ip, port))
        except Exception as e:
            print(f"[NACK] Failed to send retransmit request: {e}")

    def _send_retransmit_service_message(self, sequencer_id: int, global_seq: int):
        ip = IP_ADD_MAP[sequencer_id]['IP']
        port = IP_ADD_MAP[sequencer_id]['PORT']
        retransmit = RetransmitServiceMessage(
            request_node_id=self.node_id,
            source_node_id=sequencer_id,
            request_sequence_num=global_seq
        )
        packet = GeneralMessage()
        packet.retransmit_service_message.CopyFrom(retransmit)
        try:
            self.udp_sock.sendto(packet.SerializeToString(), (ip, port))
        except Exception as e:
            print(f"[NACK] Failed to send retransmit service: {e}")

    def _broadcast_request(self, req_msg: RequestMessage):
        packet = GeneralMessage()
        packet.request_message.CopyFrom(req_msg)
        self._broadcast(packet.SerializeToString())

    def _broadcast_service(self, svc_msg: ServiceMessage):
        packet = GeneralMessage()
        packet.service_message.CopyFrom(svc_msg)
        self._broadcast(packet.SerializeToString())

    def _broadcast(self, data: bytes):
        for node_id, addr in IP_ADD_MAP.items():
            try:
                self.udp_sock.sendto(data, (addr['IP'], addr['PORT']))
            except Exception as e:
                print(f"[UDP] Failed to send to node {node_id}: {e}")


# ─────────────────────────────────────────────────────────
# Delivery executor thread
# ─────────────────────────────────────────────────────────

def delivery_executor(state: NodeState):
    conn = create_db_connection(state.node_id)
    while True:
        req, pending = state.delivery_queue.get()
        response = execute_request(req, conn)
        if pending is not None:
            event, holder = pending
            holder.response = response
            event.set()


# ─────────────────────────────────────────────────────────
# gRPC service
# ─────────────────────────────────────────────────────────

class CustomerDatabaseService(CustomerDatabaseServicer):
    def __init__(self, state: NodeState, db_conn):
        super().__init__()
        self.state = state
        self.db_conn = db_conn  # for read-only RPCs

    def _broadcast_write(self, seq: int, set_body_fn):
        req_msg = RequestMessage(
            sender_id=self.state.node_id,
            local_sequence_num=seq,
            metadata=RequestMetadata(global_sequence=self.state.last_delivered_seq),
        )
        set_body_fn(req_msg)
        event = threading.Event()
        holder = ResultHolder()
        self.state.register_pending_response(seq, event, holder)
        self.state.broadcast_request(req_msg)
        timed_out = not event.wait(timeout=DELIVER_TIMEOUT)
        return timed_out, holder.response

    # ── Write RPCs ─────────────────────────────────────────

    def CreateSeller(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.create_seller_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return CreateSellerResponse()
        return resp or CreateSellerResponse()

    def CreateBuyer(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.create_buyer_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return CreateBuyerResponse()
        return resp or CreateBuyerResponse()

    def UpdateSeller(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.update_seller_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return GenericResponse(success=False, message="Timeout")
        return resp or GenericResponse(success=False, message="DB execution failed")

    def UpdateBuyer(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.update_buyer_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return GenericResponse(success=False, message="Timeout")
        return resp or GenericResponse(success=False, message="DB execution failed")

    def CreateSession(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.create_session_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return CreateSessionResponse()
        return resp or CreateSessionResponse()

    def DeleteSession(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.delete_session_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return GenericResponse(success=False, message="Timeout")
        return resp or GenericResponse(success=False, message="DB execution failed")

    def ResetAll(self, request, context):
        seq = self.state.next_local_seq()
        timed_out, resp = self._broadcast_write(seq, lambda m: m.reset_all_request.CopyFrom(request))
        if timed_out:
            context.set_code(grpc.StatusCode.DEADLINE_EXCEEDED)
            context.set_details("Timeout: request not delivered")
            return GenericResponse(success=False, message="Timeout")
        return resp or GenericResponse(success=False, message="DB execution failed")

    # ── Read RPCs ──────────────────────────────────────────

    def GetSellerByName(self, request, context):
        with self.db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, name, password, thumbs_up, thumbs_down, items_sold "
                "FROM sellers WHERE name = %s",
                (request.seller_name,)
            )
            row = cur.fetchone()
        if row is None:
            return SellerResponse(found=False)
        return SellerResponse(
            found=True,
            seller=Seller(
                seller_id=row["id"], seller_name=row["name"], password=row["password"],
                feedback=Feedback(thumbs_up=row["thumbs_up"], thumbs_down=row["thumbs_down"]),
                items_sold=row["items_sold"],
            )
        )

    def GetBuyerByName(self, request, context):
        with self.db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, name, password, items_purchased FROM buyers WHERE name = %s",
                (request.buyer_name,)
            )
            row = cur.fetchone()
        if row is None:
            return BuyerResponse(found=False)
        return BuyerResponse(
            found=True,
            buyer=Buyer(
                buyer_id=row["id"], buyer_name=row["name"],
                password=row["password"], items_purchased=row["items_purchased"],
            )
        )

    def GetSeller(self, request, context):
        with self.db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, name, password, thumbs_up, thumbs_down, items_sold "
                "FROM sellers WHERE id = %s",
                (request.seller_id,)
            )
            row = cur.fetchone()
        if row is None:
            return SellerResponse(found=False)
        return SellerResponse(
            found=True,
            seller=Seller(
                seller_id=row["id"], seller_name=row["name"], password=row["password"],
                feedback=Feedback(thumbs_up=row["thumbs_up"], thumbs_down=row["thumbs_down"]),
                items_sold=row["items_sold"],
            )
        )

    def GetBuyer(self, request, context):
        with self.db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, name, password, items_purchased FROM buyers WHERE id = %s",
                (request.buyer_id,)
            )
            row = cur.fetchone()
        if row is None:
            return BuyerResponse(found=False)
        return BuyerResponse(
            found=True,
            buyer=Buyer(
                buyer_id=row["id"], buyer_name=row["name"],
                password=row["password"], items_purchased=row["items_purchased"],
            )
        )

    def GetSession(self, request, context):
        with self.db_conn.cursor() as cur:
            cur.execute(
                "SELECT id, user_id, user_type, expiration FROM sessions WHERE id = %s",
                (request.session_id,)
            )
            row = cur.fetchone()
        if row is None:
            return SessionResponse(found=False)
        if row["expiration"] < int(time.time()):
            with self.db_conn.cursor() as cur:
                cur.execute("DELETE FROM sessions WHERE id = %s", (request.session_id,))
            self.db_conn.commit()
            return SessionResponse(found=False)
        return SessionResponse(
            found=True,
            session=Session(
                session_id=row["id"], user_id=row["user_id"],
                user_type=row["user_type"], expiration=row["expiration"],
            )
        )


# ─────────────────────────────────────────────────────────
# Thread functions
# ─────────────────────────────────────────────────────────

def grpc_serve(state: NodeState, db_conn):
    server = grpc.server(futures.ThreadPoolExecutor(max_workers=10))
    add_CustomerDatabaseServicer_to_server(CustomerDatabaseService(state, db_conn), server)
    port = GRPC_PORT + state.node_id
    server.add_insecure_port(f"0.0.0.0:{port}")
    server.start()
    print(f"[gRPC] Node {state.node_id} listening on 0.0.0.0:{port}")
    server.wait_for_termination()


def udp_receive(state: NodeState, msg_queue: queue.Queue):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    bind_addr = IP_ADD_MAP[state.node_id]
    sock.bind((bind_addr['IP'], bind_addr['PORT']))
    print(f"[UDP] Node {state.node_id} listening on {bind_addr['IP']}:{bind_addr['PORT']}")
    while True:
        data, _ = sock.recvfrom(65535)
        msg_queue.put(data)


def process_messages(state: NodeState, msg_queue: queue.Queue):
    while True:
        data = msg_queue.get()
        try:
            general_msg = GeneralMessage()
            general_msg.ParseFromString(data)
        except Exception as e:
            print(f"[MSG] Parse error: {e}")
            continue

        field = general_msg.WhichOneof("message_type")
        if field == "request_message":
            state.on_request_message(general_msg.request_message)
        elif field == "service_message":
            state.on_service_message(general_msg.service_message)
        elif field == "retransmit_request_message":
            msg = general_msg.retransmit_request_message
            state.on_retransmit_request(msg.source_node_id, msg.request_sequence_num)
        elif field == "retransmit_service_message":
            msg = general_msg.retransmit_service_message
            state.on_retransmit_seq(msg.request_sequence_num)
        else:
            print(f"[MSG] Unknown field: {field}")


# ─────────────────────────────────────────────────────────
# Entry point
# ─────────────────────────────────────────────────────────

def start_server(args):
    udp_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    state = NodeState(node_id=args.id, n=NUM_NODES, udp_sock=udp_sock)
    msg_queue: queue.Queue = queue.Queue(maxsize=1000)

    # Shared DB connection for gRPC read-only RPCs
    db_conn = create_db_connection(args.id)

    threads = [
        threading.Thread(target=grpc_serve, args=(state, db_conn), daemon=True),
        threading.Thread(target=udp_receive, args=(state, msg_queue), daemon=True),
        threading.Thread(target=process_messages, args=(state, msg_queue), daemon=True),
        threading.Thread(target=delivery_executor, args=(state,), daemon=True),
    ]
    for t in threads:
        t.start()

    print(f"[Main] Node {args.id} started — gRPC port {GRPC_PORT + args.id}, UDP port {IP_ADD_MAP[args.id]['PORT']}")

    for t in threads:
        t.join()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="CustomerDB replica with total order broadcast")
    parser.add_argument("--id", type=int, required=True, help="Node ID (0 to NUM_NODES-1)")
    args = parser.parse_args()
    try:
        start_server(args)
    except KeyboardInterrupt:
        print("Server stopped.")
