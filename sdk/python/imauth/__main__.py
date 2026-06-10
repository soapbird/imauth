"""Command-line interface for the imauth Python SDK.

Thin wrapper over :class:`imauth.ImauthClient` so the gRPC surface is reachable
from a shell — and, since the package ships a console script, runnable with
``uvx``::

    uvx --from <wheel-url> imauth --server localhost:6100 login --platform naver

Connection defaults come from the environment so they don't have to be repeated:
``IMAUTH_URL`` (default ``localhost:6100``) and ``IMAUTH_API_KEY``.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Optional

from imauth.client import ImauthClient
from imauth.exceptions import ImauthError
from imauth.models import Cookie, Platform


def _emit(obj) -> None:
    """Print a pydantic model / dict as one JSON line (screenshot bytes dropped)."""
    if hasattr(obj, "model_dump"):
        obj = obj.model_dump(exclude={"screenshot"})
    print(json.dumps(obj, ensure_ascii=False, default=str))


def _client(args: argparse.Namespace) -> ImauthClient:
    return ImauthClient(server_address=args.server, api_key=args.api_key)


def _cmd_login(client: ImauthClient, args: argparse.Namespace) -> int:
    for event in client.login(Platform(args.platform)):
        _emit(event)
    return 0


def _cmd_status(client: ImauthClient, args: argparse.Namespace) -> int:
    event = client.get_status(args.session_id)
    if event is None:
        print(f"session not found: {args.session_id}", file=sys.stderr)
        return 1
    _emit(event)
    return 0


def _cmd_cancel(client: ImauthClient, args: argparse.Namespace) -> int:
    client.cancel(args.session_id)
    return 0


def _cmd_cookies(client: ImauthClient, args: argparse.Namespace) -> int:
    cookies = client.get_cookies(Platform(args.platform), domains=args.domain or None)
    _emit([c.model_dump() for c in cookies])
    return 0


def _cmd_validate(client: ImauthClient, args: argparse.Namespace) -> int:
    valid = client.validate_session(Platform(args.platform))
    _emit({"platform": args.platform, "valid": valid})
    return 0 if valid else 1


def _cmd_connections(client: ImauthClient, args: argparse.Namespace) -> int:
    _emit(client.get_connection_status())
    return 0


def _cmd_export(client: ImauthClient, args: argparse.Namespace) -> int:
    print(client.export_netscape(Platform(args.platform)), end="")
    return 0


def _cmd_creds_save(client: ImauthClient, args: argparse.Namespace) -> int:
    password = args.password or os.environ.get("IMAUTH_CRED_PASSWORD", "")
    client.save_credentials(
        Platform(args.platform), args.username, password, twofa_method=args.twofa
    )
    return 0


def _cmd_creds_get(client: ImauthClient, args: argparse.Namespace) -> int:
    info = client.get_credentials(Platform(args.platform))
    if info is None:
        print(f"no credentials for {args.platform}", file=sys.stderr)
        return 1
    _emit(info)
    return 0


def _cmd_creds_delete(client: ImauthClient, args: argparse.Namespace) -> int:
    existed = client.delete_credentials(Platform(args.platform))
    _emit({"platform": args.platform, "deleted": existed})
    return 0


def _build_parser() -> argparse.ArgumentParser:
    platforms = [p.value for p in Platform]
    parser = argparse.ArgumentParser(prog="imauth", description="imauth gRPC client")
    parser.add_argument(
        "--server",
        default=os.environ.get("IMAUTH_URL", "localhost:6100"),
        help="gRPC server address (env IMAUTH_URL, default localhost:6100)",
    )
    parser.add_argument(
        "--api-key",
        default=os.environ.get("IMAUTH_API_KEY"),
        help="bearer API key (env IMAUTH_API_KEY)",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    def _platform_arg(p: argparse.ArgumentParser) -> None:
        p.add_argument("--platform", required=True, choices=platforms)

    p = sub.add_parser("login", help="start a user-driven login and stream events")
    _platform_arg(p)
    p.set_defaults(func=_cmd_login)

    p = sub.add_parser("status", help="get current status of a session")
    p.add_argument("--session-id", required=True)
    p.set_defaults(func=_cmd_status)

    p = sub.add_parser("cancel", help="cancel an in-flight session")
    p.add_argument("--session-id", required=True)
    p.set_defaults(func=_cmd_cancel)

    p = sub.add_parser("cookies", help="list stored cookies for a platform")
    _platform_arg(p)
    p.add_argument("--domain", action="append", help="filter by domain (repeatable)")
    p.set_defaults(func=_cmd_cookies)

    p = sub.add_parser("validate", help="check whether a platform session is present")
    _platform_arg(p)
    p.set_defaults(func=_cmd_validate)

    p = sub.add_parser("connections", help="connection status for all platforms")
    p.set_defaults(func=_cmd_connections)

    p = sub.add_parser("export-netscape", help="export cookies in Netscape format")
    _platform_arg(p)
    p.set_defaults(func=_cmd_export)

    p = sub.add_parser("creds-save", help="save credentials for a platform")
    _platform_arg(p)
    p.add_argument("--username", required=True)
    p.add_argument(
        "--password",
        help="password (or set IMAUTH_CRED_PASSWORD to avoid leaking it to argv)",
    )
    p.add_argument("--twofa", default=None, help="2FA method")
    p.set_defaults(func=_cmd_creds_save)

    p = sub.add_parser("creds-get", help="show stored credential info for a platform")
    _platform_arg(p)
    p.set_defaults(func=_cmd_creds_get)

    p = sub.add_parser("creds-delete", help="delete stored credentials for a platform")
    _platform_arg(p)
    p.set_defaults(func=_cmd_creds_delete)

    return parser


def main(argv: Optional[list[str]] = None) -> int:
    args = _build_parser().parse_args(argv)
    client = _client(args)
    try:
        return args.func(client, args)
    except ImauthError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        return 130
    finally:
        client.close()


if __name__ == "__main__":
    raise SystemExit(main())
