"""Synchronous imauth client."""

from typing import Iterator, Optional
import grpc

from imauth.models import AuthEvent, Cookie, CredentialInfo, Platform
from imauth.exceptions import ImauthConnectionError, ImauthAuthError

# Note: generated protobuf modules should be built via:
# python -m grpc_tools.protoc -I../../proto --python_out=. --grpc_python_out=. ../../proto/imauth/v1/*.proto
# For now, this module provides the high-level interface.

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
        """Start login and yield auth events.

        Usage:
            for event in client.login(Platform.INSTAGRAM, "user", "pass"):
                if event.requires_input and event.input_type == "2fa_code":
                    code = input("2FA code: ")
                    client.submit_2fa(session_id, code)
        """
        raise NotImplementedError("Generated gRPC stubs required")

    def submit_2fa(self, session_id: str, code: str) -> AuthEvent:
        """Submit 2FA code for an ongoing session."""
        raise NotImplementedError("Generated gRPC stubs required")

    def get_cookies(self, platform: Platform) -> list[Cookie]:
        """Get stored cookies for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    def export_netscape(self, platform: Platform) -> str:
        """Export cookies in Netscape format."""
        raise NotImplementedError("Generated gRPC stubs required")

    def get_connection_status(self) -> dict[str, bool]:
        """Get connection status for all platforms."""
        raise NotImplementedError("Generated gRPC stubs required")

    def save_credentials(
        self,
        platform: Platform,
        username: str,
        password: str,
        twofa_method: Optional[str] = None,
    ) -> None:
        """Save credentials for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    def get_credentials(self, platform: Platform) -> Optional[CredentialInfo]:
        """Get stored credential info for a platform."""
        raise NotImplementedError("Generated gRPC stubs required")

    def close(self) -> None:
        self._channel.close()
