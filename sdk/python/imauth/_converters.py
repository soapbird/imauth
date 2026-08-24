"""Shared proto <-> model converters used by both sync and async clients."""

from imauth.models import AuthEvent, AuthStatus, Cookie, Platform
from imauth.v1 import common_pb2

_PLATFORM_TO_PROTO = {
    Platform.INSTAGRAM: common_pb2.Platform.PLATFORM_INSTAGRAM,
    Platform.THREADS: common_pb2.Platform.PLATFORM_THREADS,
    Platform.NAVER: common_pb2.Platform.PLATFORM_NAVER,
    Platform.NOVELPIA: common_pb2.Platform.PLATFORM_NOVELPIA,
    Platform.MUNPIA: common_pb2.Platform.PLATFORM_MUNPIA,
}
_PROTO_TO_PLATFORM = {
    1: "instagram",
    2: "threads",
    3: "naver",
    4: "novelpia",
    5: "munpia",
}

_STATUS_MAP: dict[int, AuthStatus] = {
    common_pb2.AuthStatus.AUTH_STATUS_IDLE: AuthStatus.IDLE,
    common_pb2.AuthStatus.AUTH_STATUS_LOADING: AuthStatus.LOADING,
    common_pb2.AuthStatus.AUTH_STATUS_AUTHENTICATING: AuthStatus.AUTHENTICATING,
    common_pb2.AuthStatus.AUTH_STATUS_WAITING_FOR_USER: AuthStatus.WAITING_FOR_USER,
    common_pb2.AuthStatus.AUTH_STATUS_CONNECTED: AuthStatus.CONNECTED,
    common_pb2.AuthStatus.AUTH_STATUS_FAILED: AuthStatus.FAILED,
}


def platform_to_proto(platform: Platform) -> common_pb2.Platform:
    return _PLATFORM_TO_PROTO.get(
        platform,
        common_pb2.Platform.PLATFORM_UNSPECIFIED,
    )


def platform_from_proto(value: int) -> str:
    return _PROTO_TO_PLATFORM.get(value, "unknown")


def cookie_from_proto(c) -> Cookie:
    return Cookie(
        name=c.name,
        value=c.value,
        domain=c.domain,
        path=c.path,
        expires=c.expires,
        http_only=c.http_only,
        secure=c.secure,
    )


def cookie_to_proto(c: Cookie):
    return common_pb2.Cookie(
        name=c.name,
        value=c.value,
        domain=c.domain,
        path=c.path,
        expires=c.expires,
        http_only=c.http_only,
        secure=c.secure,
    )


def auth_event_from_proto(event) -> AuthEvent:
    return AuthEvent(
        status=_STATUS_MAP.get(event.status, AuthStatus.IDLE),
        session_id=getattr(event, "session_id", ""),
        message=event.message,
        requires_input=event.requires_input,
        input_type=event.input_type,
        cookies=[cookie_from_proto(c) for c in event.cookies],
        viewer_url=getattr(event, "viewer_url", ""),
    )


def status_response_to_event(resp, session_id: str) -> AuthEvent:
    """Convert a StatusResponse into an AuthEvent. The proto field is the same
    int enum used by AuthEvent (not a string), so reuse `_STATUS_MAP`."""
    return AuthEvent(
        status=_STATUS_MAP.get(getattr(resp, "status", 0), AuthStatus.IDLE),
        session_id=session_id,
        message=getattr(resp, "message", ""),
        requires_input=getattr(resp, "requires_input", False),
        input_type=getattr(resp, "input_type", ""),
    )


def api_key_metadata(api_key):
    """Build gRPC call metadata for an optional API key. Returns () when None."""
    if not api_key:
        return ()
    return (("authorization", f"Bearer {api_key}"),)
