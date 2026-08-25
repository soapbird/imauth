"""TLS channel and CLI configuration tests."""

from __future__ import annotations

from unittest.mock import MagicMock, patch

from imauth import __main__ as cli
from imauth import async_client as async_client_module
from imauth import client as client_module
from imauth.async_client import AsyncImauthClient
from imauth.client import ImauthClient


def test_sync_client_uses_insecure_channel_by_default() -> None:
    # Given: no TLS options.
    channel = MagicMock()

    # When: the synchronous client is constructed.
    with patch.object(
        client_module.grpc,
        "insecure_channel",
        return_value=channel,
    ) as insecure:
        client = ImauthClient("test:1234")

    # Then: the backwards-compatible insecure channel is used.
    assert client._channel is channel
    insecure.assert_called_once_with("test:1234")


def test_sync_client_uses_tls_ca_and_server_name() -> None:
    # Given: an explicit CA bundle and TLS server name.
    channel = MagicMock()
    credentials = MagicMock()
    ca_certificates = b"test-ca"

    # When: the synchronous client is constructed.
    with (
        patch.object(
            client_module.grpc,
            "ssl_channel_credentials",
            return_value=credentials,
        ) as create_credentials,
        patch.object(
            client_module.grpc,
            "secure_channel",
            return_value=channel,
        ) as secure,
    ):
        client = ImauthClient(
            "test:1234",
            root_certificates=ca_certificates,
            server_name="auth.example.test",
        )

    # Then: gRPC uses the provided trust root and TLS target-name override.
    assert client._channel is channel
    create_credentials.assert_called_once_with(root_certificates=ca_certificates)
    secure.assert_called_once_with(
        "test:1234",
        credentials,
        options=(("grpc.ssl_target_name_override", "auth.example.test"),),
    )


def test_async_client_uses_insecure_channel_by_default() -> None:
    # Given: no TLS options.
    channel = MagicMock()

    # When: the asynchronous client is constructed.
    with patch.object(
        async_client_module.grpc.aio, "insecure_channel", return_value=channel
    ) as insecure:
        client = AsyncImauthClient("test:1234")

    # Then: the backwards-compatible insecure channel is used.
    assert client._channel is channel
    insecure.assert_called_once_with("test:1234")


def test_async_client_uses_tls_ca_and_server_name() -> None:
    # Given: an explicit CA bundle and TLS server name.
    channel = MagicMock()
    credentials = MagicMock()
    ca_certificates = b"test-ca"

    # When: the asynchronous client is constructed.
    with (
        patch.object(
            async_client_module.grpc,
            "ssl_channel_credentials",
            return_value=credentials,
        ) as create_credentials,
        patch.object(
            async_client_module.grpc.aio, "secure_channel", return_value=channel
        ) as secure,
    ):
        client = AsyncImauthClient(
            "test:1234",
            root_certificates=ca_certificates,
            server_name="auth.example.test",
        )

    # Then: gRPC uses the provided trust root and TLS target-name override.
    assert client._channel is channel
    create_credentials.assert_called_once_with(root_certificates=ca_certificates)
    secure.assert_called_once_with(
        "test:1234",
        credentials,
        options=(("grpc.ssl_target_name_override", "auth.example.test"),),
    )


def test_cli_tls_flags_read_ca_and_forward_tls_configuration(tmp_path) -> None:
    # Given: a CLI CA file and explicit TLS flags.
    ca_path = tmp_path / "ca.pem"
    ca_path.write_bytes(b"test-ca")
    args = cli._build_parser().parse_args(
        [
            "--tls-ca-cert",
            str(ca_path),
            "--tls-server-name",
            "auth.example.test",
            "connections",
        ]
    )

    # When: the CLI constructs its SDK client.
    with patch.object(cli, "ImauthClient") as client_class:
        cli._client(args)

    # Then: it forwards CA contents rather than a filesystem path.
    client_class.assert_called_once_with(
        server_address="localhost:6100",
        api_key=None,
        root_certificates=b"test-ca",
        server_name="auth.example.test",
    )


def test_cli_tls_environment_defaults_read_ca_and_forward_configuration(
    monkeypatch, tmp_path
) -> None:
    # Given: TLS configuration supplied entirely through environment variables.
    ca_path = tmp_path / "ca.pem"
    ca_path.write_bytes(b"environment-ca")
    monkeypatch.setenv("IMAUTH_TLS_CA_CERT", str(ca_path))
    monkeypatch.setenv("IMAUTH_TLS_SERVER_NAME", "env.example.test")
    args = cli._build_parser().parse_args(["connections"])

    # When: the CLI constructs its SDK client.
    with patch.object(cli, "ImauthClient") as client_class:
        cli._client(args)

    # Then: environment defaults opt into TLS with the parsed CA contents.
    client_class.assert_called_once_with(
        server_address="localhost:6100",
        api_key=None,
        root_certificates=b"environment-ca",
        server_name="env.example.test",
    )
