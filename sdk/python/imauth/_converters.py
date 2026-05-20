"""Shared proto <-> model converters used by both sync and async clients."""

from imauth.v1 import common_pb2
from imauth.models import AuthEvent, AuthStatus, Cookie, Platform


_PLATFORM_TO_PROTO = {Platform.INSTAGRAM: 1, Platform.THREADS: 2, Platform.NAVER: 3}
_PROTO_TO_PLATFORM = {1: "instagram", 2: "threads", 3: "naver"}

_STATUS_MAP = {
    common_pb2.AuthStatus.AUTH_STATUS_IDLE: AuthStatus.IDLE,
    common_pb2.AuthStatus.AUTH_STATUS_LOADING: AuthStatus.LOADING,
    common_pb2.AuthStatus.AUTH_STATUS_AUTHENTICATING: AuthStatus.AUTHENTICATING,
    common_pb2.AuthStatus.AUTH_STATUS_WAITING_FOR_USER: AuthStatus.WAITING_FOR_USER,
    common_pb2.AuthStatus.AUTH_STATUS_CONNECTED: AuthStatus.CONNECTED,
    common_pb2.AuthStatus.AUTH_STATUS_FAILED: AuthStatus.FAILED,
}


def platform_to_proto(platform: Platform) -> int:
    return _PLATFORM_TO_PROTO.get(platform, 0)


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
    from imauth.v1 import session_pb2
    return session_pb2.Cookie(
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
        screenshot=bytes(event.screenshot),
        viewer_url=getattr(event, "viewer_url", ""),
    )


def auth_response_to_event(resp) -> AuthEvent:
    status = AuthStatus.CONNECTED if resp.success else AuthStatus.FAILED
    return AuthEvent(
        status=status,
        session_id=getattr(resp, "session_id", ""),
        message=resp.message,
        cookies=[cookie_from_proto(c) for c in resp.cookies],
    )


def status_response_to_event(resp, session_id: str) -> AuthEvent:
    """Convert a StatusResponse into an AuthEvent. The proto field is the same
    int enum used by AuthEvent (not a string), so reuse `_STATUS_MAP`."""
    return AuthEvent(
        status=_STATUS_MAP.get(getattr(resp, "status", 0), AuthStatus.IDLE),
        session_id=session_id,
        message=getattr(resp, "message", ""),
    )


def api_key_metadata(api_key):
    """Build gRPC call metadata for an optional API key. Returns () when None."""
    if not api_key:
        return ()
    return (("authorization", f"Bearer {api_key}"),)
