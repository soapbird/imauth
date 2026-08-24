"""Mock-based tests for the asynchronous imauth Python client.

We don't add pytest-asyncio as a dependency; instead we drive each coroutine
through ``anyio.run`` and mock the gRPC stubs with ``AsyncMock``.
"""

from __future__ import annotations

import types
from typing import Final
from unittest.mock import AsyncMock, MagicMock, patch

import anyio
import grpc
import pytest

from imauth import async_client as ac
from imauth.async_client import AsyncImauthClient
from imauth.exceptions import ImauthConnectionError
from imauth.models import AuthStatus, CredentialInfo, Platform

CONNECTED_STATUS: Final = 7


def _make_client() -> AsyncImauthClient:
    with patch.object(ac.grpc.aio, "insecure_channel", return_value=MagicMock()):
        return AsyncImauthClient("test:1234")


def _stub_cookie(**overrides) -> types.SimpleNamespace:
    base = {
        "name": "sessionid",
        "value": "abc",
        "domain": ".instagram.com",
        "path": "/",
        "expires": 0,
        "http_only": True,
        "secure": True,
    }
    base.update(overrides)
    return types.SimpleNamespace(**base)


class _AsyncIter:
    """Helper async iterator for streaming RPC mocks."""

    def __init__(self, items):
        self._items = list(items)

    def __aiter__(self):
        return self

    async def __anext__(self):
        if not self._items:
            raise StopAsyncIteration
        return self._items.pop(0)


def test_async_login_streams_events():
    c = _make_client()
    fake_stub = MagicMock()

    event = types.SimpleNamespace(
        status=CONNECTED_STATUS,
        message="ok",
        requires_input=False,
        input_type="",
        cookies=[_stub_cookie()],
        screenshot=b"",
    )
    fake_stub.Login = MagicMock(return_value=_AsyncIter([event]))

    async def run():
        with patch.object(ac.auth_pb2_grpc, "AuthServiceStub", return_value=fake_stub):
            return [evt async for evt in c.login(Platform.INSTAGRAM)]

    events = anyio.run(run)
    assert len(events) == 1
    assert events[0].status == AuthStatus.CONNECTED
    args, _ = fake_stub.Login.call_args
    assert args[0].platform == 1


def test_async_get_cookies():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetCookies = AsyncMock(
        return_value=types.SimpleNamespace(cookies=[_stub_cookie(name="csrftoken")])
    )

    async def run():
        with patch.object(
            ac.session_pb2_grpc, "SessionServiceStub", return_value=fake_stub
        ):
            return await c.get_cookies(Platform.INSTAGRAM)

    cookies = anyio.run(run)
    assert [ck.name for ck in cookies] == ["csrftoken"]


def test_async_validate_session_details():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.ValidateSession = AsyncMock(
        return_value=types.SimpleNamespace(
            valid=True,
            expires_at=1_900_000_000,
            session_cookie_name="sessionid",
        )
    )

    async def run():
        with patch.object(
            ac.session_pb2_grpc, "SessionServiceStub", return_value=fake_stub
        ):
            return await c.validate_session_details(Platform.INSTAGRAM)

    details = anyio.run(run)
    assert details.expires_at == 1_900_000_000
    assert details.session_cookie_name == "sessionid"


def test_async_get_cookies_maps_unavailable_to_connection_error():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetCookies = AsyncMock(
        side_effect=grpc.aio.AioRpcError(
            grpc.StatusCode.UNAVAILABLE,
            grpc.aio.Metadata(),
            grpc.aio.Metadata(),
        )
    )

    async def run():
        with patch.object(
            ac.session_pb2_grpc, "SessionServiceStub", return_value=fake_stub
        ):
            await c.get_cookies(Platform.INSTAGRAM)

    with pytest.raises(ImauthConnectionError):
        anyio.run(run)


def test_async_export_netscape():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.ExportNetscape = AsyncMock(
        return_value=types.SimpleNamespace(content="# Netscape\n")
    )

    async def run():
        with patch.object(
            ac.session_pb2_grpc, "SessionServiceStub", return_value=fake_stub
        ):
            return await c.export_netscape(Platform.THREADS)

    content = anyio.run(run)
    assert content.startswith("# Netscape")


def test_async_get_connection_status():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetConnectionStatus = AsyncMock(
        return_value=types.SimpleNamespace(platforms={"instagram": True})
    )

    async def run():
        with patch.object(
            ac.session_pb2_grpc, "SessionServiceStub", return_value=fake_stub
        ):
            return await c.get_connection_status()

    status = anyio.run(run)
    assert status == {"instagram": True}


def test_async_save_credentials_serializes_request():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Save = AsyncMock(return_value=types.SimpleNamespace())

    async def run():
        with patch.object(
            ac.credential_pb2_grpc, "CredentialServiceStub", return_value=fake_stub
        ):
            await c.save_credentials(Platform.INSTAGRAM, "alice", "pw", "totp")

    anyio.run(run)
    args, _ = fake_stub.Save.call_args
    assert args[0].username == "alice"
    assert args[0].twofa_method == "totp"


def test_async_get_credentials_happy_path():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Get = AsyncMock(
        return_value=types.SimpleNamespace(
            platform=2, username="bob", has_password=True, twofa_method=""
        )
    )

    async def run():
        with patch.object(
            ac.credential_pb2_grpc, "CredentialServiceStub", return_value=fake_stub
        ):
            return await c.get_credentials(Platform.THREADS)

    info = anyio.run(run)
    assert isinstance(info, CredentialInfo)
    assert info.platform == Platform.THREADS
    assert info.username == "bob"


def test_async_get_credentials_returns_none_on_not_found():
    c = _make_client()
    fake_stub = MagicMock()

    fake_stub.Get = AsyncMock(
        side_effect=grpc.aio.AioRpcError(
            grpc.StatusCode.NOT_FOUND,
            grpc.aio.Metadata(),
            grpc.aio.Metadata(),
        )
    )

    async def run():
        with patch.object(
            ac.credential_pb2_grpc, "CredentialServiceStub", return_value=fake_stub
        ):
            return await c.get_credentials(Platform.INSTAGRAM)

    assert anyio.run(run) is None


def test_async_close():
    c = _make_client()
    c._channel.close = AsyncMock()

    async def run():
        await c.close()

    anyio.run(run)
    c._channel.close.assert_awaited_once()


def test_async_helpers_match_sync():
    assert ac._platform_to_proto(Platform.INSTAGRAM) == 1
    assert ac._platform_from_proto(2) == "threads"
    assert ac._platform_from_proto(99) == "unknown"
