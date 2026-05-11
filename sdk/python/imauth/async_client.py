"""Async imauth client."""

from typing import AsyncIterator, Optional
import grpc.aio

from imauth.models import AuthEvent, Cookie, CredentialInfo, Platform
from imauth.exceptions import ImauthConnectionError, ImauthAuthError

class AsyncImauthClient:
    """Async client for imauth gRPC server."""

    def __init__(self, server_address: str = "localhost:50051"):
        self.server_address = server_address
        self._channel = grpc.aio.insecure_channel(server_address)

    async def login(
        self,
        platform: Platform,
        username: str,
        password: str,
    ) -> AsyncIterator[AuthEvent]:
        """Start login and yield auth events."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def submit_2fa(self, session_id: str, code: str) -> AuthEvent:
        """Submit 2FA code for an ongoing session."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def get_cookies(self, platform: Platform) -> list[Cookie]:
        """Get stored cookies for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def export_netscape(self, platform: Platform) -> str:
        """Export cookies in Netscape format."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def get_connection_status(self) -> dict[str, bool]:
        """Get connection status for all platforms."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def save_credentials(
        self,
        platform: Platform,
        username: str,
        password: str,
        twofa_method: Optional[str] = None,
    ) -> None:
        """Save credentials for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def get_credentials(self, platform: Platform) -> Optional[CredentialInfo]:
        """Get stored credential info for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    async def close(self) -> None:
        await self._channel.close()
