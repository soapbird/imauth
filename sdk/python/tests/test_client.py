"""Unit tests for imauth Python SDK."""

from imauth.models import AuthEvent, AuthStatus, Cookie, Platform


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


def test_auth_event_public_shape_matches_contract():
    event = AuthEvent(status=AuthStatus.IDLE)

    assert set(event.model_dump()) == {
        "cookies",
        "input_type",
        "message",
        "requires_input",
        "session_id",
        "status",
        "viewer_url",
    }
