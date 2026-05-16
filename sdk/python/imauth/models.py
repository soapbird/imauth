"""Pydantic models for imauth types."""

from enum import Enum
from typing import Optional
from pydantic import BaseModel

class Platform(str, Enum):
    INSTAGRAM = "instagram"
    THREADS = "threads"

class AuthStatus(str, Enum):
    IDLE = "idle"
    LOADING = "loading"
    AUTHENTICATING = "authenticating"
    NEEDS_CREDS = "needs_creds"
    NEEDS_2FA = "needs_2fa"
    NEEDS_CAPTCHA = "needs_captcha"
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

class CredentialInfo(BaseModel):
    platform: Platform
    username: str
    has_password: bool
    twofa_method: str = ""
