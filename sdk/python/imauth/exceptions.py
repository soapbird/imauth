"""imauth exceptions."""

from collections.abc import Iterator
from contextlib import contextmanager

import grpc


class ImauthError(Exception):
    """Base imauth error."""


class ImauthConnectionError(ImauthError):
    """Failed to connect to imauth server."""


class ImauthAuthError(ImauthError):
    """Authentication failed."""


class ImauthNotFoundError(ImauthError):
    """Resource not found."""


def rpc_error_to_imauth(error: grpc.RpcError) -> ImauthError:
    code = error.code()
    details = error.details() if hasattr(error, "details") else None
    message = details or code.name
    if code == grpc.StatusCode.UNAVAILABLE:
        return ImauthConnectionError(message)
    if code in {grpc.StatusCode.UNAUTHENTICATED, grpc.StatusCode.PERMISSION_DENIED}:
        return ImauthAuthError(message)
    if code == grpc.StatusCode.NOT_FOUND:
        return ImauthNotFoundError(message)
    return ImauthError(message)


@contextmanager
def translate_rpc_errors() -> Iterator[None]:
    try:
        yield
    except grpc.RpcError as error:
        raise rpc_error_to_imauth(error) from error
