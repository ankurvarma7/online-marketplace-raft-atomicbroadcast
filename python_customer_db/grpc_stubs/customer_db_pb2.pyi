from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ResetRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class Feedback(_message.Message):
    __slots__ = ("thumbs_up", "thumbs_down")
    THUMBS_UP_FIELD_NUMBER: _ClassVar[int]
    THUMBS_DOWN_FIELD_NUMBER: _ClassVar[int]
    thumbs_up: int
    thumbs_down: int
    def __init__(self, thumbs_up: _Optional[int] = ..., thumbs_down: _Optional[int] = ...) -> None: ...

class Seller(_message.Message):
    __slots__ = ("seller_id", "seller_name", "feedback", "items_sold", "password")
    SELLER_ID_FIELD_NUMBER: _ClassVar[int]
    SELLER_NAME_FIELD_NUMBER: _ClassVar[int]
    FEEDBACK_FIELD_NUMBER: _ClassVar[int]
    ITEMS_SOLD_FIELD_NUMBER: _ClassVar[int]
    PASSWORD_FIELD_NUMBER: _ClassVar[int]
    seller_id: str
    seller_name: str
    feedback: Feedback
    items_sold: int
    password: str
    def __init__(self, seller_id: _Optional[str] = ..., seller_name: _Optional[str] = ..., feedback: _Optional[_Union[Feedback, _Mapping]] = ..., items_sold: _Optional[int] = ..., password: _Optional[str] = ...) -> None: ...

class Buyer(_message.Message):
    __slots__ = ("buyer_id", "buyer_name", "items_purchased", "password")
    BUYER_ID_FIELD_NUMBER: _ClassVar[int]
    BUYER_NAME_FIELD_NUMBER: _ClassVar[int]
    ITEMS_PURCHASED_FIELD_NUMBER: _ClassVar[int]
    PASSWORD_FIELD_NUMBER: _ClassVar[int]
    buyer_id: str
    buyer_name: str
    items_purchased: int
    password: str
    def __init__(self, buyer_id: _Optional[str] = ..., buyer_name: _Optional[str] = ..., items_purchased: _Optional[int] = ..., password: _Optional[str] = ...) -> None: ...

class Session(_message.Message):
    __slots__ = ("session_id", "user_id", "user_type", "expiration")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    USER_TYPE_FIELD_NUMBER: _ClassVar[int]
    EXPIRATION_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    user_id: str
    user_type: str
    expiration: int
    def __init__(self, session_id: _Optional[str] = ..., user_id: _Optional[str] = ..., user_type: _Optional[str] = ..., expiration: _Optional[int] = ...) -> None: ...

class CreateSellerRequest(_message.Message):
    __slots__ = ("seller_name", "password")
    SELLER_NAME_FIELD_NUMBER: _ClassVar[int]
    PASSWORD_FIELD_NUMBER: _ClassVar[int]
    seller_name: str
    password: str
    def __init__(self, seller_name: _Optional[str] = ..., password: _Optional[str] = ...) -> None: ...

class CreateSellerResponse(_message.Message):
    __slots__ = ("seller_id",)
    SELLER_ID_FIELD_NUMBER: _ClassVar[int]
    seller_id: str
    def __init__(self, seller_id: _Optional[str] = ...) -> None: ...

class CreateBuyerRequest(_message.Message):
    __slots__ = ("buyer_name", "password")
    BUYER_NAME_FIELD_NUMBER: _ClassVar[int]
    PASSWORD_FIELD_NUMBER: _ClassVar[int]
    buyer_name: str
    password: str
    def __init__(self, buyer_name: _Optional[str] = ..., password: _Optional[str] = ...) -> None: ...

class CreateBuyerResponse(_message.Message):
    __slots__ = ("buyer_id",)
    BUYER_ID_FIELD_NUMBER: _ClassVar[int]
    buyer_id: str
    def __init__(self, buyer_id: _Optional[str] = ...) -> None: ...

class GetSellerByNameRequest(_message.Message):
    __slots__ = ("seller_name",)
    SELLER_NAME_FIELD_NUMBER: _ClassVar[int]
    seller_name: str
    def __init__(self, seller_name: _Optional[str] = ...) -> None: ...

class GetBuyerByNameRequest(_message.Message):
    __slots__ = ("buyer_name",)
    BUYER_NAME_FIELD_NUMBER: _ClassVar[int]
    buyer_name: str
    def __init__(self, buyer_name: _Optional[str] = ...) -> None: ...

class GetSellerRequest(_message.Message):
    __slots__ = ("seller_id",)
    SELLER_ID_FIELD_NUMBER: _ClassVar[int]
    seller_id: str
    def __init__(self, seller_id: _Optional[str] = ...) -> None: ...

class GetBuyerRequest(_message.Message):
    __slots__ = ("buyer_id",)
    BUYER_ID_FIELD_NUMBER: _ClassVar[int]
    buyer_id: str
    def __init__(self, buyer_id: _Optional[str] = ...) -> None: ...

class SellerResponse(_message.Message):
    __slots__ = ("found", "seller")
    FOUND_FIELD_NUMBER: _ClassVar[int]
    SELLER_FIELD_NUMBER: _ClassVar[int]
    found: bool
    seller: Seller
    def __init__(self, found: bool = ..., seller: _Optional[_Union[Seller, _Mapping]] = ...) -> None: ...

class BuyerResponse(_message.Message):
    __slots__ = ("found", "buyer")
    FOUND_FIELD_NUMBER: _ClassVar[int]
    BUYER_FIELD_NUMBER: _ClassVar[int]
    found: bool
    buyer: Buyer
    def __init__(self, found: bool = ..., buyer: _Optional[_Union[Buyer, _Mapping]] = ...) -> None: ...

class UpdateSellerRequest(_message.Message):
    __slots__ = ("seller",)
    SELLER_FIELD_NUMBER: _ClassVar[int]
    seller: Seller
    def __init__(self, seller: _Optional[_Union[Seller, _Mapping]] = ...) -> None: ...

class UpdateBuyerRequest(_message.Message):
    __slots__ = ("buyer",)
    BUYER_FIELD_NUMBER: _ClassVar[int]
    buyer: Buyer
    def __init__(self, buyer: _Optional[_Union[Buyer, _Mapping]] = ...) -> None: ...

class CreateSessionRequest(_message.Message):
    __slots__ = ("user_id", "user_type")
    USER_ID_FIELD_NUMBER: _ClassVar[int]
    USER_TYPE_FIELD_NUMBER: _ClassVar[int]
    user_id: str
    user_type: str
    def __init__(self, user_id: _Optional[str] = ..., user_type: _Optional[str] = ...) -> None: ...

class CreateSessionResponse(_message.Message):
    __slots__ = ("session_id", "expiration")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    EXPIRATION_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    expiration: int
    def __init__(self, session_id: _Optional[str] = ..., expiration: _Optional[int] = ...) -> None: ...

class GetSessionRequest(_message.Message):
    __slots__ = ("session_id",)
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    def __init__(self, session_id: _Optional[str] = ...) -> None: ...

class SessionResponse(_message.Message):
    __slots__ = ("found", "session")
    FOUND_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    found: bool
    session: Session
    def __init__(self, found: bool = ..., session: _Optional[_Union[Session, _Mapping]] = ...) -> None: ...

class DeleteSessionRequest(_message.Message):
    __slots__ = ("session_id",)
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    def __init__(self, session_id: _Optional[str] = ...) -> None: ...

class GenericResponse(_message.Message):
    __slots__ = ("success", "message")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    success: bool
    message: str
    def __init__(self, success: bool = ..., message: _Optional[str] = ...) -> None: ...
