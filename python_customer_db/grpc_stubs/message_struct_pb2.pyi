import customer_db_pb2 as _customer_db_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class GeneralMessage(_message.Message):
    __slots__ = ("request_message", "service_message", "retransmit_request_message", "retransmit_service_message")
    REQUEST_MESSAGE_FIELD_NUMBER: _ClassVar[int]
    SERVICE_MESSAGE_FIELD_NUMBER: _ClassVar[int]
    RETRANSMIT_REQUEST_MESSAGE_FIELD_NUMBER: _ClassVar[int]
    RETRANSMIT_SERVICE_MESSAGE_FIELD_NUMBER: _ClassVar[int]
    request_message: RequestMessage
    service_message: ServiceMessage
    retransmit_request_message: RetransmitRequestMessage
    retransmit_service_message: RetransmitServiceMessage
    def __init__(self, request_message: _Optional[_Union[RequestMessage, _Mapping]] = ..., service_message: _Optional[_Union[ServiceMessage, _Mapping]] = ..., retransmit_request_message: _Optional[_Union[RetransmitRequestMessage, _Mapping]] = ..., retransmit_service_message: _Optional[_Union[RetransmitServiceMessage, _Mapping]] = ...) -> None: ...

class RequestMessage(_message.Message):
    __slots__ = ("sender_id", "local_sequence_num", "request_id", "create_seller_request", "metadata")
    SENDER_ID_FIELD_NUMBER: _ClassVar[int]
    LOCAL_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CREATE_SELLER_REQUEST_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    sender_id: int
    local_sequence_num: int
    request_id: int
    create_seller_request: _customer_db_pb2.CreateSellerRequest
    metadata: RequestMetadata
    def __init__(self, sender_id: _Optional[int] = ..., local_sequence_num: _Optional[int] = ..., request_id: _Optional[int] = ..., create_seller_request: _Optional[_Union[_customer_db_pb2.CreateSellerRequest, _Mapping]] = ..., metadata: _Optional[_Union[RequestMetadata, _Mapping]] = ...) -> None: ...

class RequestMetadata(_message.Message):
    __slots__ = ("global_sequence",)
    GLOBAL_SEQUENCE_FIELD_NUMBER: _ClassVar[int]
    global_sequence: int
    def __init__(self, global_sequence: _Optional[int] = ...) -> None: ...

class ServiceMessage(_message.Message):
    __slots__ = ("source_node_id", "local_seq_num", "request_id", "global_id", "sender_id", "metadata")
    SOURCE_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    LOCAL_SEQ_NUM_FIELD_NUMBER: _ClassVar[int]
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    GLOBAL_ID_FIELD_NUMBER: _ClassVar[int]
    SENDER_ID_FIELD_NUMBER: _ClassVar[int]
    METADATA_FIELD_NUMBER: _ClassVar[int]
    source_node_id: int
    local_seq_num: int
    request_id: int
    global_id: int
    sender_id: int
    metadata: ServiceMetadata
    def __init__(self, source_node_id: _Optional[int] = ..., local_seq_num: _Optional[int] = ..., request_id: _Optional[int] = ..., global_id: _Optional[int] = ..., sender_id: _Optional[int] = ..., metadata: _Optional[_Union[ServiceMetadata, _Mapping]] = ...) -> None: ...

class ServiceMetadata(_message.Message):
    __slots__ = ("last_delivered_seq",)
    LAST_DELIVERED_SEQ_FIELD_NUMBER: _ClassVar[int]
    last_delivered_seq: int
    def __init__(self, last_delivered_seq: _Optional[int] = ...) -> None: ...

class RetransmitRequestMessage(_message.Message):
    __slots__ = ("request_node_id", "source_node_id", "request_sequence_num")
    REQUEST_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    SOURCE_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    REQUEST_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    request_node_id: int
    source_node_id: int
    request_sequence_num: int
    def __init__(self, request_node_id: _Optional[int] = ..., source_node_id: _Optional[int] = ..., request_sequence_num: _Optional[int] = ...) -> None: ...

class RetransmitRequestMessageResponse(_message.Message):
    __slots__ = ("request_node_id", "source_node_id", "request_sequence_num")
    REQUEST_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    SOURCE_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    REQUEST_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    request_node_id: int
    source_node_id: int
    request_sequence_num: int
    def __init__(self, request_node_id: _Optional[int] = ..., source_node_id: _Optional[int] = ..., request_sequence_num: _Optional[int] = ...) -> None: ...

class RetransmitServiceMessage(_message.Message):
    __slots__ = ("request_node_id", "source_node_id", "request_sequence_num", "request_message_list")
    REQUEST_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    SOURCE_NODE_ID_FIELD_NUMBER: _ClassVar[int]
    REQUEST_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    REQUEST_MESSAGE_LIST_FIELD_NUMBER: _ClassVar[int]
    request_node_id: int
    source_node_id: int
    request_sequence_num: int
    request_message_list: _containers.RepeatedCompositeFieldContainer[RequestMessage]
    def __init__(self, request_node_id: _Optional[int] = ..., source_node_id: _Optional[int] = ..., request_sequence_num: _Optional[int] = ..., request_message_list: _Optional[_Iterable[_Union[RequestMessage, _Mapping]]] = ...) -> None: ...

class ProcessedRequestMessage(_message.Message):
    __slots__ = ("sender_id", "local_sequence_num", "global_sequence_num")
    SENDER_ID_FIELD_NUMBER: _ClassVar[int]
    LOCAL_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    GLOBAL_SEQUENCE_NUM_FIELD_NUMBER: _ClassVar[int]
    sender_id: int
    local_sequence_num: int
    global_sequence_num: int
    def __init__(self, sender_id: _Optional[int] = ..., local_sequence_num: _Optional[int] = ..., global_sequence_num: _Optional[int] = ...) -> None: ...
