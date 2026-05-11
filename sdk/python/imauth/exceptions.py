"""imauth exceptions."""

class ImauthError(Exception):
    """Base imauth error."""
    pass

class ImauthConnectionError(ImauthError):
    """Failed to connect to imauth server."""
    pass

class ImauthAuthError(ImauthError):
    """Authentication failed."""
    pass

class ImauthNotFoundError(ImauthError):
    """Resource not found."""
    pass
