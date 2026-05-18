"""Async imauth client."""

from typing import AsyncIterator, Optional

import grpc
import grpc.aio

from imauth.v1 import auth_pb2, auth_pb2_grpc
from imauth.v1 import common_pb2
from imauth.v1 import credential_pb2, credential_pb2_grpc
from imauth.v1 import session_pb2, session_pb2_grpc
from imauth.models import AuthEvent, AuthStatus, Cookie, CredentialInfo, Platform
from imauth._converters import (
    api_key_metadata,
    auth_event_from_proto,
    auth_response_to_event,
    cookie_from_proto,
    cookie_to_proto,
    platform_from_proto,
    platform_to_proto,
    status_response_to_event,
)

# Backward-compat private aliases.
_platform_to_proto = platform_to_proto
_platform_from_proto = platform_from_proto
_cookie_from_proto = cookie_from_proto
_cookie_to_proto = cookie_to_proto
_auth_event_from_proto = auth_event_from_proto
_auth_response_to_event = auth_response_to_event


class AsyncImauthClient:
    """Async client for imauth gRPC server.

    Pass `api_key` when the server is started with `IMAUTH_API_KEY`/`--api-key`;
    the SDK will attach `authorization: Bearer <key>` to every call.
    """

    def __init__(
        self,
        server_address: str = "localhost:50051",
        api_key: Optional[str] = None,
    ):
        self.server_address = server_address
        self.api_key = api_key
        self._channel = grpc.aio.insecure_channel(server_address)

    def _meta(self):
        return api_key_metadata(self.api_key)

    # --- auth -------------------------------------------------------------

    async def login(
        self,
        platform: Platform,
    ) -> AsyncIterator[AuthEvent]:
        """Start login (user-driven via browser) and yield auth events."""
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.LoginRequest(
            platform=platform_to_proto(platform),
        )
        async for event in stub.Login(req, metadata=self._meta()):
            yield auth_event_from_proto(event)

    async def get_status(self, session_id: str) -> AuthEvent:
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.SubmitCaptchaRequest(session_id=session_id, solution=solution)
        resp = await stub.SubmitCaptcha(req, metadata=self._meta())
        return auth_response_to_event(resp)

    async def get_status(self, session_id: str) -> Optional[AuthEvent]:
        """Get current status of a session. Returns None if the session is unknown."""
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.StatusRequest(session_id=session_id)
        try:
            resp = await stub.GetStatus(req, metadata=self._meta())
            return status_response_to_event(resp, session_id)
        except grpc.aio.AioRpcError as e:
            if e.code() == grpc.StatusCode.NOT_FOUND:
                return None
            raise

    async def cancel(self, session_id: str) -> None:
        """Cancel an in-flight session. Idempotent — NOT_FOUND is suppressed."""
        stub = auth_pb2_grpc.AuthServiceStub(self._channel)
        req = auth_pb2.CancelRequest(session_id=session_id)
        try:
            await stub.Cancel(req, metadata=self._meta())
        except grpc.aio.AioRpcError as e:
            if e.code() != grpc.StatusCode.NOT_FOUND:
                raise

    # --- session ----------------------------------------------------------

    async def get_cookies(
        self,
        platform: Platform,
        domains: Optional[list[str]] = None,
    ) -> list[Cookie]:
        """Get stored cookies for a platform, optionally filtered by domain."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.GetCookiesRequest(
            platform=platform_to_proto(platform),
            domains=domains or [],
        )
        resp = await stub.GetCookies(req, metadata=self._meta())
        return [cookie_from_proto(c) for c in resp.cookies]

    async def update_cookies(self, platform: Platform, cookies: list[Cookie]) -> None:
        """Replace stored cookies for a platform."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.UpdateCookiesRequest(
            platform=platform_to_proto(platform),
            cookies=[cookie_to_proto(c) for c in cookies],
        )
        await stub.UpdateCookies(req, metadata=self._meta())

    async def export_netscape(self, platform: Platform) -> str:
        """Export cookies in Netscape format."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.ExportRequest(platform=platform_to_proto(platform))
        resp = await stub.ExportNetscape(req, metadata=self._meta())
        return resp.content

    async def validate_session(self, platform: Platform) -> bool:
        """Return True if a session cookie for the platform is present."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        req = session_pb2.ValidateRequest(platform=platform_to_proto(platform))
        resp = await stub.ValidateSession(req, metadata=self._meta())
        return bool(resp.valid)

    async def get_connection_status(self) -> dict[str, bool]:
        """Get connection status for all platforms."""
        stub = session_pb2_grpc.SessionServiceStub(self._channel)
        resp = await stub.GetConnectionStatus(common_pb2.Empty(), metadata=self._meta())
        return dict(resp.platforms)

    # --- credentials ------------------------------------------------------

    async def save_credentials(
        self,
        platform: Platform,
        username: str,
        password: str,
        twofa_method: Optional[str] = None,
    ) -> None:
        """Save credentials for a platform."""
        stub = credential_pb2_grpc.CredentialServiceStub(self._channel)
        req = credential_pb2.SaveCredentialRequest(
            platform=platform_to_proto(platform),
            username=username,
            password=password,
            twofa_method=twofa_method or "",
        )
        await stub.Save(req, metadata=self._meta())

    async def get_credentials(self, platform: Platform) -> Optional[CredentialInfo]:
        """Get stored credential info for a platform. Returns None on NOT_FOUND."""
        stub = credential_pb2_grpc.CredentialServiceStub(self._channel)
        req = credential_pb2.GetCredentialRequest(platform=platform_to_proto(platform))
        try:
            resp = await stub.Get(req, metadata=self._meta())
            return CredentialInfo(
                platform=Platform(platform_from_proto(resp.platform)),
                username=resp.username,
                has_password=resp.has_password,
                twofa_method=resp.twofa_method,
            )
        except grpc.aio.AioRpcError as e:
            if e.code() == grpc.StatusCode.NOT_FOUND:
                return None
            raise

    async def delete_credentials(self, platform: Platform) -> bool:
        """Delete stored credentials for a platform. Returns True if existed."""
        stub = credential_pb2_grpc.CredentialServiceStub(self._channel)
        req = credential_pb2.DeleteCredentialRequest(
            platform=platform_to_proto(platform)
        )
        try:
            await stub.Delete(req, metadata=self._meta())
            return True
        except grpc.aio.AioRpcError as e:
            if e.code() == grpc.StatusCode.NOT_FOUND:
                return False
            raise

    async def close(self) -> None:
        await self._channel.close()
