"""Mock-based tests for the synchronous imauth Python client.

Verifies that the client constructs the correct request payloads, dispatches them
through the right gRPC stub, and maps response payloads back into SDK models.
No real gRPC server is started; we monkeypatch the stub classes used inside
each client method.
"""

from __future__ import annotations

import types
from typing import Final
from unittest.mock import MagicMock, patch

import grpc
import pytest

from imauth import client as client_module
from imauth.client import ImauthClient
from imauth.models import AuthStatus, Cookie, CredentialInfo, Platform

CONNECTED_STATUS: Final = 7

# ---- helpers ----------------------------------------------------------------


def _make_client() -> ImauthClient:
    """Construct a client without touching real grpc state.

    ``grpc.insecure_channel`` is cheap (lazy), but we patch it to keep tests
    deterministic and avoid any DNS resolution attempts on import.
    """
    with patch.object(client_module.grpc, "insecure_channel", return_value=MagicMock()):
        return ImauthClient("test:1234")


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


def _stub_auth_event(status: int, message: str = "ok") -> types.SimpleNamespace:
    return types.SimpleNamespace(
        status=status,
        message=message,
        requires_input=False,
        input_type="",
        cookies=[_stub_cookie()],
        screenshot=b"",
    )


class _RpcError(grpc.RpcError):
    def __init__(self, code: grpc.StatusCode) -> None:
        super().__init__()
        self._code = code

    def code(self) -> grpc.StatusCode:
        return self._code


# ---- login ------------------------------------------------------------------


def test_login_streams_events_and_serializes_request():
    c = _make_client()
    fake_stub = MagicMock()

    fake_stub.Login.return_value = iter(
        [_stub_auth_event(CONNECTED_STATUS, "connected!")]
    )

    with patch.object(
        client_module.auth_pb2_grpc, "AuthServiceStub", return_value=fake_stub
    ):
        events = list(c.login(Platform.INSTAGRAM))

    assert len(events) == 1
    event = events[0]
    assert event.status == AuthStatus.CONNECTED
    assert event.message == "connected!"
    assert len(event.cookies) == 1
    assert event.cookies[0].name == "sessionid"

    # Verify the request that was dispatched.
    args, _ = fake_stub.Login.call_args
    req = args[0]
    assert req.platform == 1  # Platform.INSTAGRAM -> 1


def test_login_threads_platform_serializes_to_two():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Login.return_value = iter([])

    with patch.object(
        client_module.auth_pb2_grpc, "AuthServiceStub", return_value=fake_stub
    ):
        list(c.login(Platform.THREADS))

    args, _ = fake_stub.Login.call_args
    assert args[0].platform == 2


# ---- cookies / session ------------------------------------------------------


def test_get_cookies_maps_response():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetCookies.return_value = types.SimpleNamespace(
        cookies=[
            _stub_cookie(name="csrftoken", value="xyz"),
            _stub_cookie(name="sessionid"),
        ]
    )

    with patch.object(
        client_module.session_pb2_grpc,
        "SessionServiceStub",
        return_value=fake_stub,
    ):
        cookies = c.get_cookies(Platform.INSTAGRAM)

    assert [ck.name for ck in cookies] == ["csrftoken", "sessionid"]
    assert all(isinstance(ck, Cookie) for ck in cookies)

    args, _ = fake_stub.GetCookies.call_args
    assert args[0].platform == 1
    assert list(args[0].domains) == []


def test_get_cookies_empty_list():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetCookies.return_value = types.SimpleNamespace(cookies=[])

    with patch.object(
        client_module.session_pb2_grpc,
        "SessionServiceStub",
        return_value=fake_stub,
    ):
        cookies = c.get_cookies(Platform.INSTAGRAM)

    assert cookies == []


def test_export_netscape_returns_content_string():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.ExportNetscape.return_value = types.SimpleNamespace(
        content=(
            "# Netscape HTTP Cookie File\n"
            ".instagram.com\tTRUE\t/\tTRUE\t0\tsessionid\tabc\n"
        )
    )

    with patch.object(
        client_module.session_pb2_grpc,
        "SessionServiceStub",
        return_value=fake_stub,
    ):
        content = c.export_netscape(Platform.INSTAGRAM)

    assert "Netscape HTTP Cookie File" in content
    assert "sessionid" in content
    args, _ = fake_stub.ExportNetscape.call_args
    assert args[0].platform == 1


def test_get_connection_status_returns_dict():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.GetConnectionStatus.return_value = types.SimpleNamespace(
        platforms={"instagram": True, "threads": False}
    )

    with patch.object(
        client_module.session_pb2_grpc,
        "SessionServiceStub",
        return_value=fake_stub,
    ):
        status = c.get_connection_status()

    assert status == {"instagram": True, "threads": False}


# ---- credentials ------------------------------------------------------------


def test_save_credentials_serializes_all_fields():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Save.return_value = types.SimpleNamespace()

    with patch.object(
        client_module.credential_pb2_grpc,
        "CredentialServiceStub",
        return_value=fake_stub,
    ):
        c.save_credentials(Platform.INSTAGRAM, "alice", "hunter2", twofa_method="totp")

    args, _ = fake_stub.Save.call_args
    req = args[0]
    assert req.platform == 1
    assert req.username == "alice"
    assert req.password == "hunter2"
    assert req.twofa_method == "totp"


def test_save_credentials_defaults_twofa_to_empty_string():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Save.return_value = types.SimpleNamespace()

    with patch.object(
        client_module.credential_pb2_grpc,
        "CredentialServiceStub",
        return_value=fake_stub,
    ):
        c.save_credentials(Platform.THREADS, "bob", "pw")

    args, _ = fake_stub.Save.call_args
    assert args[0].twofa_method == ""


def test_get_credentials_returns_credential_info():
    c = _make_client()
    fake_stub = MagicMock()
    fake_stub.Get.return_value = types.SimpleNamespace(
        platform=1,
        username="alice",
        has_password=True,
        twofa_method="totp",
    )

    with patch.object(
        client_module.credential_pb2_grpc,
        "CredentialServiceStub",
        return_value=fake_stub,
    ):
        info = c.get_credentials(Platform.INSTAGRAM)

    assert isinstance(info, CredentialInfo)
    assert info.platform == Platform.INSTAGRAM
    assert info.username == "alice"
    assert info.has_password is True
    assert info.twofa_method == "totp"


def test_get_credentials_returns_none_on_not_found():
    c = _make_client()
    fake_stub = MagicMock()

    err = _RpcError(grpc.StatusCode.NOT_FOUND)
    fake_stub.Get.side_effect = err

    with patch.object(
        client_module.credential_pb2_grpc,
        "CredentialServiceStub",
        return_value=fake_stub,
    ):
        info = c.get_credentials(Platform.INSTAGRAM)

    assert info is None


def test_get_credentials_reraises_non_not_found():
    c = _make_client()
    fake_stub = MagicMock()

    err = _RpcError(grpc.StatusCode.INTERNAL)
    fake_stub.Get.side_effect = err

    with (
        patch.object(
            client_module.credential_pb2_grpc,
            "CredentialServiceStub",
            return_value=fake_stub,
        ),
        pytest.raises(grpc.RpcError),
    ):
        c.get_credentials(Platform.INSTAGRAM)


# ---- close ------------------------------------------------------------------


def test_close_closes_channel():
    close_channel = MagicMock()
    channel = MagicMock()
    channel.close = close_channel
    with patch.object(client_module.grpc, "insecure_channel", return_value=channel):
        c = ImauthClient("test:1234")

    c.close()
    close_channel.assert_called_once()


# ---- helper exercises -------------------------------------------------------


def test_platform_to_proto_helper():
    assert client_module._platform_to_proto(Platform.INSTAGRAM) == 1
    assert client_module._platform_to_proto(Platform.THREADS) == 2


def test_platform_from_proto_helper():
    assert client_module._platform_from_proto(1) == "instagram"
    assert client_module._platform_from_proto(2) == "threads"
    assert client_module._platform_from_proto(99) == "unknown"


def test_auth_event_from_proto_unknown_status_falls_back_to_idle():
    evt = client_module._auth_event_from_proto(_stub_auth_event(status=9999))
    assert evt.status == AuthStatus.IDLE


def test_auth_response_to_event_success_maps_to_connected():
    from imauth._converters import auth_response_to_event

    resp = types.SimpleNamespace(success=True, message="hi", cookies=[])
    evt = auth_response_to_event(resp)
    assert evt.status == AuthStatus.CONNECTED
    assert evt.message == "hi"


def test_auth_response_to_event_failure_maps_to_failed():
    from imauth._converters import auth_response_to_event

    resp = types.SimpleNamespace(success=False, message="nope", cookies=[])
    evt = auth_response_to_event(resp)
    assert evt.status == AuthStatus.FAILED
