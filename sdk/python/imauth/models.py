"""Pydantic models for imauth types."""

from enum import Enum

from pydantic import BaseModel


class Platform(str, Enum):
    INSTAGRAM = "instagram"
    THREADS = "threads"
    NAVER = "naver"


class AuthStatus(str, Enum):
    IDLE = "idle"
    LOADING = "loading"
    AUTHENTICATING = "authenticating"
    WAITING_FOR_USER = "waiting_for_user"
    CONNECTED = "connected"
    FAILED = "failed"


class Cookie(BaseModel):
    name: str
    value: str
    domain: str
    path: str = "/"
    expires: int = 0
    http_only: bool = False
    secure: bool = False


class AuthEvent(BaseModel):
    status: AuthStatus
    session_id: str = ""
    message: str = ""
    requires_input: bool = False
    input_type: str = ""
    cookies: list[Cookie] = []
    screenshot: bytes = b""
    viewer_url: str = ""


class CredentialInfo(BaseModel):
    platform: Platform
    username: str
    has_password: bool
    twofa_method: str = ""
