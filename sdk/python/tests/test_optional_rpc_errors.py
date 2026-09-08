from concurrent.futures import ThreadPoolExecutor
from inspect import isawaitable

import anyio
import grpc
import pytest

from imauth import AsyncImauthClient, ImauthClient
from imauth.exceptions import ImauthAuthError, ImauthConnectionError, ImauthError
from imauth.models import AuthEvent, CredentialInfo, Platform


@pytest.mark.parametrize("use_async", [False, True])
@pytest.mark.parametrize(
    "operation",
    [
        ("get_status", None),
        ("cancel", None),
        ("get_credentials", None),
        ("delete_credentials", False),
    ],
)
@pytest.mark.parametrize(
    "failure",
    [
        (grpc.StatusCode.NOT_FOUND, None),
        (grpc.StatusCode.UNAVAILABLE, ImauthConnectionError),
        (grpc.StatusCode.PERMISSION_DENIED, ImauthAuthError),
    ],
)
def test_optional_rpc_result_when_server_rejects_request(
    *,
    use_async: bool,
    operation: tuple[str, bool | None],
    failure: tuple[grpc.StatusCode, type[ImauthError] | None],
) -> None:
    # Given a real server rejecting each optional RPC with the selected status.
    method, missing_result = operation
    status, expected_error = failure

    def abort(_request: bytes, context: grpc.ServicerContext) -> None:
        context.abort(status, "optional RPC regression")

    with ThreadPoolExecutor(max_workers=1) as executor:
        server = grpc.server(executor)
        server.add_generic_rpc_handlers(
            [
                grpc.method_handlers_generic_handler(
                    f"imauth.v1.{service}",
                    {
                        name: grpc.unary_unary_rpc_method_handler(abort)
                        for name in names
                    },
                )
                for service, names in [
                    ("AuthService", ("GetStatus", "Cancel")),
                    ("CredentialService", ("Get", "Delete")),
                ]
            ]
        )
        address = f"127.0.0.1:{server.add_insecure_port('127.0.0.1:0')}"
        server.start()

        async def invoke() -> AuthEvent | CredentialInfo | bool | None:
            client = AsyncImauthClient(address) if use_async else ImauthClient(address)
            try:
                result = {
                    "get_status": lambda: client.get_status("missing-session"),
                    "cancel": lambda: client.cancel("missing-session"),
                    "get_credentials": lambda: client.get_credentials(Platform.THREADS),
                    "delete_credentials": lambda: client.delete_credentials(
                        Platform.THREADS
                    ),
                }[method]()
                return await result if isawaitable(result) else result
            finally:
                closed = client.close()
                if isawaitable(closed):
                    await closed

        try:
            # When the SDK makes the request, then only NOT_FOUND is suppressed.
            if expected_error is None:
                assert anyio.run(invoke) is missing_result
            else:
                with pytest.raises(
                    expected_error, match="optional RPC regression"
                ) as error:
                    _ = anyio.run(invoke)
                assert isinstance(error.value.__cause__, grpc.RpcError)
                assert error.value.__cause__.code() == status
        finally:
            _ = server.stop(0).wait()
