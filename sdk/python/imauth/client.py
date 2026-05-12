"""Synchronous imauth client."""

from typing import Iterator, Optional

import grpc

from imauth.v1 import auth_pb2, auth_pb2_grpc
from imauth.v1 import common_pb2
from imauth.v1 import credential_pb2, credential_pb2_grpc
from imauth.v1 import session_pb2, session_pb2_grpc
from imauth.models import AuthEvent, AuthStatus, Cookie, CredentialInfo, Platform


def _platform_to_proto(platform: Platform) -> int:
    mapping = {Platform.INSTAGRAM: 1, Platform.THREADS: 2}
    return mapping.get(platform, 0)


def _platform_from_proto(value: int) -> str:
    mapping = {1: "instagram", 2: "threads"}
    return mapping.get(value, "unknown")


def _cookie_from_proto(c) -> Cookie:
    return Cookie(
        name=c.name,
        value=c.value,
        domain=c.domain,
        path=c.path,
        expires=c.expires,
        http_only=c.http_only,
        secure=c.secure,
    )


def _auth_event_from_proto(event) -> AuthEvent:
    status_map = {
        common_pb2.AuthStatus.AUTH_STATUS_IDLE: AuthStatus.IDLE,
        common_pb2.AuthStatus.AUTH_STATUS_LOADING: AuthStatus.LOADING,
        common_pb2.AuthStatus.AUTH_STATUS_AUTHENTICATING: AuthStatus.AUTHENTICATING,
        common_pb2.AuthStatus.AUTH_STATUS_NEEDS_CREDS: AuthStatus.NEEDS_CREDS,
        common_pb2.AuthStatus.AUTH_STATUS_NEEDS_2FA: AuthStatus.NEEDS_2FA,
        common_pb2.AuthStatus.AUTH_STATUS_NEEDS_CAPTCHA: AuthStatus.NEEDS_CAPTCHA,
        common_pb2.AuthStatus.AUTH_STATUS_CONNECTED: AuthStatus.CONNECTED,
        common_pb2.AuthStatus.AUTH_STATUS_FAILED: AuthStatus.FAILED,
    }
    return AuthEvent(
        status=status_map.get(event.status, AuthStatus.IDLE),
        message=event.message,
        requires_input=event.requires_input,
        input_type=event.input_type,
        cookies=[_cookie_from_proto(c) for c in event.cookies],
        screenshot=bytes(event.screenshot),
    )


def _auth_response_to_event(resp) -> AuthEvent:
    status = AuthStatus.CONNECTED if resp.success else AuthStatus.FAILED
    return AuthEvent(
        status=status,
        message=resp.message,
        cookies=[_cookie_from_proto(c) for c in resp.cookies],
    )


class ImauthClient:
    """Synchronous client for imauth gRPC server."""

    def __init__(self, server_address: str = "localhost:50051"):
        self.server_address = server_address
        self._channel = grpc.insecure_channel(server_address)

    def login(
        self,
        platform: Platform,
        username: str,
        password: str,
    ) -> Iterator[AuthEvent]:
        """Start login and yield auth events."""
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.LoginRequest(
            platform=_platform_to_proto(platform),
            username=username,
            password=password,
        )
        for event in stub.Login(req):
            yield _auth_event_from_proto(event)

    def submit_2fa(self, session_id: str, code: str) -> AuthEvent:
        """Submit 2FA code for an ongoing session."""
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.Submit2FaRequest(session_id=session_id, code=code)
        resp = stub.Submit2FA(req)
        return _auth_response_to_event(resp)

    def get_cookies(self, platform: Platform) -> list[Cookie]:
        """Get stored cookies for a platform."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.GetCookiesRequest(
            platform=_platform_to_proto(platform),
            domains=[],
        )
        resp = stub.GetCookies(req)
        return [_cookie_from_proto(c) for c in resp.cookies]

    def export_netscape(self, platform: Platform) -> str:
        """Export cookies in Netscape format."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.ExportRequest(platform=_platform_to_proto(platform))
        resp = stub.ExportNetscape(req)
        return resp.content

    def get_connection_status(self) -> dict[str, bool]:
        """Get connection status for all platforms."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        resp = stub.GetConnectionStatus(common_pb2.Empty())
        return dict(resp.platforms)

    def save_credentials(
        self,
        platform: Platform,
        username: str,
        password: str,
        twofa_method: Optional[str] = None,
    ) -> None:
        """Save credentials for a platform."""
        stub = credential_pb2_grpc.CredentialServiceStub(self._channel)
        req = credential_pb2.SaveCredentialRequest(
            platform=_platform_to_proto(platform),
            username=username,
            password=password,
            twofa_method=twofa_method or "",
        )
        stub.Save(req)

    def get_credentials(self, platform: Platform) -> Optional[CredentialInfo]:
        """Get stored credential info for a platform."""
        stub = credential_pb2_grpc.CredentialServiceStub(self._channel)
        req = credential_pb2.GetCredentialRequest(
            platform=_platform_to_proto(platform)
        )
        try:
            resp = stub.Get(req)
            return CredentialInfo(
                platform=Platform(_platform_from_proto(resp.platform)),
                username=resp.username,
                has_password=resp.has_password,
                twofa_method=resp.twofa_method,
            )
        except grpc.RpcError as e:
            if e.code() == grpc.StatusCode.NOT_FOUND:
                return None
            raise

    def close(self) -> None:
        self._channel.close()
