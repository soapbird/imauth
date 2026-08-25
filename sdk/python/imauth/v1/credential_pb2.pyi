from imauth.v1 import common_pb2 as _common_pb2
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class SaveCredentialRequest(_message.Message):
    __slots__ = ("platform", "username", "password", "twofa_method")
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    USERNAME_FIELD_NUMBER: _ClassVar[int]
    PASSWORD_FIELD_NUMBER: _ClassVar[int]
    TWOFA_METHOD_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    username: str
    password: str
    twofa_method: str
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ..., username: _Optional[str] = ..., password: _Optional[str] = ..., twofa_method: _Optional[str] = ...) -> None: ...

class GetCredentialRequest(_message.Message):
    __slots__ = ("platform",)
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ...) -> None: ...

class CredentialInfo(_message.Message):
    __slots__ = ("platform", "username", "has_password", "twofa_method")
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    USERNAME_FIELD_NUMBER: _ClassVar[int]
    HAS_PASSWORD_FIELD_NUMBER: _ClassVar[int]
    TWOFA_METHOD_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    username: str
    has_password: bool
    twofa_method: str
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ..., username: _Optional[str] = ..., has_password: bool = ..., twofa_method: _Optional[str] = ...) -> None: ...

class DeleteCredentialRequest(_message.Message):
    __slots__ = ("platform",)
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    platform: _common_pb2.Platform
    def __init__(self, platform: _Optional[_Union[_common_pb2.Platform, str]] = ...) -> None: ...

class CredentialResponse(_message.Message):
    __slots__ = ("success", "platform", "username")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    USERNAME_FIELD_NUMBER: _ClassVar[int]
    success: bool
    platform: _common_pb2.Platform
    username: str
    def __init__(self, success: bool = ..., platform: _Optional[_Union[_common_pb2.Platform, str]] = ..., username: _Optional[str] = ...) -> None: ...
