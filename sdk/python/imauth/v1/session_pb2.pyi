from imauth.v1 import common_pb2 as _common_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class GetCookiesRequest(_message.Message):
    __slots__ = ("platform", "domains")
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    DOMAINS_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    domains: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ..., domains: _Optional[_Iterable[str]] = ...) -> None: ...

class CookieList(_message.Message):
    __slots__ = ("cookies",)
    COOKIES_FIELD_NUMBER: _ClassVar[int]
    cookies: _containers.RepeatedCompositeFieldContainer[_common_pb2.Cookie]
    def __init__(self, cookies: _Optional[_Iterable[_Union[_common_pb2.Cookie, _Mapping]]] = ...) -> None: ...

class UpdateCookiesRequest(_message.Message):
    __slots__ = ("platform", "cookies")
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    COOKIES_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    cookies: _containers.RepeatedCompositeFieldContainer[_common_pb2.Cookie]
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ..., cookies: _Optional[_Iterable[_Union[_common_pb2.Cookie, _Mapping]]] = ...) -> None: ...

class ExportRequest(_message.Message):
    __slots__ = ("platform",)
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ...) -> None: ...

class NetscapeExport(_message.Message):
    __slots__ = ("content",)
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    content: str
    def __init__(self, content: _Optional[str] = ...) -> None: ...

class ValidateRequest(_message.Message):
    __slots__ = ("platform",)
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ...) -> None: ...

class ValidationResult(_message.Message):
    __slots__ = ("valid", "expires_at", "session_cookie_name")
    VALID_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    SESSION_COOKIE_NAME_FIELD_NUMBER: _ClassVar[int]
    valid: bool
    expires_at: int
    session_cookie_name: str
    def __init__(self, valid: bool = ..., expires_at: _Optional[int] = ..., session_cookie_name: _Optional[str] = ...) -> None: ...

class ConnectionStatusMap(_message.Message):
    __slots__ = ("platforms",)
    class PlatformsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: bool
        def __init__(self, key: _Optional[str] = ..., value: bool = ...) -> None: ...
    PLATFORMS_FIELD_NUMBER: _ClassVar[int]
    platforms: _containers.ScalarMap[str, bool]
    def __init__(self, platforms: _Optional[_Mapping[str, bool]] = ...) -> None: ...
