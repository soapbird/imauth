from imauth.v1 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class LoginRequest(_message.Message):
    __slots__ = ("platform",)
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ...) -> None: ...

class AuthEvent(_message.Message):
    __slots__ = ("session_id", "status", "message", "requires_input", "input_type", "cookies", "viewer_url")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    REQUIRES_INPUT_FIELD_NUMBER: _ClassVar[int]
    INPUT_TYPE_FIELD_NUMBER: _ClassVar[int]
    COOKIES_FIELD_NUMBER: _ClassVar[int]
    VIEWER_URL_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    status: _common_pb2.AuthStatus
    message: str
    requires_input: bool
    input_type: str
    cookies: _containers.RepeatedCompositeFieldContainer[_common_pb2.Cookie]
    viewer_url: str
    def __init__(self, session_id: _Optional[str] = ..., status: _Optional[_Union[_common_pb2.AuthStatus, str]] = ..., message: _Optional[str] = ..., requires_input: bool = ..., input_type: _Optional[str] = ..., cookies: _Optional[_Iterable[_Union[_common_pb2.Cookie, _Mapping]]] = ..., viewer_url: _Optional[str] = ...) -> None: ...

class AuthResponse(_message.Message):
    __slots__ = ("success", "session_id", "message", "cookies")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    COOKIES_FIELD_NUMBER: _ClassVar[int]
    success: bool
    session_id: str
    message: str
    cookies: _containers.RepeatedCompositeFieldContainer[_common_pb2.Cookie]
    def __init__(self, success: bool = ..., session_id: _Optional[str] = ..., message: _Optional[str] = ..., cookies: _Optional[_Iterable[_Union[_common_pb2.Cookie, _Mapping]]] = ...) -> None: ...

class StatusRequest(_message.Message):
    __slots__ = ("session_id",)
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    def __init__(self, session_id: _Optional[str] = ...) -> None: ...

class AuthStatusResponse(_message.Message):
    __slots__ = ("session_id", "status", "message", "requires_input", "input_type")
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    REQUIRES_INPUT_FIELD_NUMBER: _ClassVar[int]
    INPUT_TYPE_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    status: _common_pb2.AuthStatus
    message: str
    requires_input: bool
    input_type: str
    def __init__(self, session_id: _Optional[str] = ..., status: _Optional[_Union[_common_pb2.AuthStatus, str]] = ..., message: _Optional[str] = ..., requires_input: bool = ..., input_type: _Optional[str] = ...) -> None: ...

class CancelRequest(_message.Message):
    __slots__ = ("session_id",)
    SESSION_ID_FIELD_NUMBER: _ClassVar[int]
    session_id: str
    def __init__(self, session_id: _Optional[str] = ...) -> None: ...
