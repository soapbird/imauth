from unittest.mock import MagicMock, patch

from imauth import client as client_module
from imauth.client import ImauthClient
from imauth.models import Cookie, Platform


def test_update_cookies_serializes_common_proto_cookie():
    # Given: an SDK cookie and a SessionService stub.
    with patch.object(client_module.grpc, "insecure_channel", return_value=MagicMock()):
        client = ImauthClient("test:1234")
    fake_stub = MagicMock()
    cookie = Cookie(
        name="sessionid",
        value="abc",
        domain=".instagram.com",
        path="/",
        expires=123,
        http_only=True,
        secure=True,
    )

    # When: the public update_cookies boundary serializes the SDK cookie.
    with patch.object(
        client_module.session_pb2_grpc,
        "SessionServiceStub",
        return_value=fake_stub,
    ):
        client.update_cookies(Platform.INSTAGRAM, [cookie])

    # Then: UpdateCookies receives the Cookie type owned by common.proto.
    from imauth.v1 import common_pb2

    request = fake_stub.UpdateCookies.call_args.args[0]
    assert request.platform == common_pb2.Platform.PLATFORM_INSTAGRAM
    assert len(request.cookies) == 1
    assert isinstance(request.cookies[0], common_pb2.Cookie)
    assert request.cookies[0].name == "sessionid"
