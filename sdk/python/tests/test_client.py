"""Unit tests for imauth Python SDK."""

import pytest
from imauth.models import Platform, AuthStatus, Cookie


def test_platform_enum():
    assert Platform.INSTAGRAM.value == "instagram"
    assert Platform.THREADS.value == "threads"
    assert Platform.NAVER.value == "naver"


def test_auth_status_enum():
    assert AuthStatus.WAITING_FOR_USER.value == "waiting_for_user"
    assert AuthStatus.CONNECTED.value == "connected"
    assert AuthStatus.FAILED.value == "failed"


def test_cookie_model():
    cookie = Cookie(name="sessionid", value="abc123", domain=".instagram.com")
    assert cookie.name == "sessionid"
    assert cookie.path == "/"
    assert not cookie.http_only
