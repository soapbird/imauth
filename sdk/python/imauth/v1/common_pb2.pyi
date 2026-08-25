from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional

DESCRIPTOR: _descriptor.FileDescriptor

class Platform(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    PLATFORM_UNSPECIFIED: _ClassVar[Platform]
    PLATFORM_INSTAGRAM: _ClassVar[Platform]
    PLATFORM_THREADS: _ClassVar[Platform]
    PLATFORM_NAVER: _ClassVar[Platform]
    PLATFORM_NOVELPIA: _ClassVar[Platform]
    PLATFORM_MUNPIA: _ClassVar[Platform]

class AuthStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    AUTH_STATUS_UNSPECIFIED: _ClassVar[AuthStatus]
    AUTH_STATUS_IDLE: _ClassVar[AuthStatus]
    AUTH_STATUS_LOADING: _ClassVar[AuthStatus]
    AUTH_STATUS_AUTHENTICATING: _ClassVar[AuthStatus]
    AUTH_STATUS_WAITING_FOR_USER: _ClassVar[AuthStatus]
    AUTH_STATUS_CONNECTED: _ClassVar[AuthStatus]
    AUTH_STATUS_FAILED: _ClassVar[AuthStatus]
PLATFORM_UNSPECIFIED: Platform
PLATFORM_INSTAGRAM: Platform
PLATFORM_THREADS: Platform
PLATFORM_NAVER: Platform
PLATFORM_NOVELPIA: Platform
PLATFORM_MUNPIA: Platform
AUTH_STATUS_UNSPECIFIED: AuthStatus
AUTH_STATUS_IDLE: AuthStatus
AUTH_STATUS_LOADING: AuthStatus
AUTH_STATUS_AUTHENTICATING: AuthStatus
AUTH_STATUS_WAITING_FOR_USER: AuthStatus
AUTH_STATUS_CONNECTED: AuthStatus
AUTH_STATUS_FAILED: AuthStatus

class Cookie(_message.Message):
    __slots__ = ("name", "value", "domain", "path", "expires", "http_only", "secure")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    DOMAIN_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_FIELD_NUMBER: _ClassVar[int]
    HTTP_ONLY_FIELD_NUMBER: _ClassVar[int]
    SECURE_FIELD_NUMBER: _ClassVar[int]
    name: str
    value: str
    domain: str
    path: str
    expires: int
    http_only: bool
    secure: bool
    def __init__(self, name: _Optional[str] = ..., value: _Optional[str] = ..., domain: _Optional[str] = ..., path: _Optional[str] = ..., expires: _Optional[int] = ..., http_only: bool = ..., secure: bool = ...) -> None: ...

class Empty(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...
