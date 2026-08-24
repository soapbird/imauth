"""imauth exceptions."""


class ImauthError(Exception):
    """Base imauth error."""


class ImauthConnectionError(ImauthError):
    """Failed to connect to imauth server."""


class ImauthAuthError(ImauthError):
    """Authentication failed."""


class ImauthNotFoundError(ImauthError):
    """Resource not found."""
