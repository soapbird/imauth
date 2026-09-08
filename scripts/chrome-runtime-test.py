# /// script
# requires-python = ">=3.9"
# dependencies = ["pytest"]
# ///
# ─── How to run ───
# uv run --with pytest pytest -q scripts/chrome-runtime-test.py

from __future__ import annotations

import importlib.util
import socket
import threading
import time
from collections.abc import Callable
from pathlib import Path
from types import ModuleType
from unittest import mock

import pytest


def load_runtime() -> ModuleType:
    path = Path(__file__).with_name("chrome-runtime.py")
    spec = importlib.util.spec_from_file_location("chrome_runtime", path)
    assert spec is not None
    assert spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wait_for(predicate: Callable[[], bool], timeout: float = 1.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.01)
    raise AssertionError("condition was not met before timeout")


def test_given_idle_upstream_when_relaying_then_socket_timeout_is_cleared() -> None:
    runtime = load_runtime()
    left, client = socket.socketpair()
    upstream, right = socket.socketpair()
    upstream.settimeout(0.05)

    relay = threading.Thread(target=runtime.relay_sockets, args=(left, upstream))
    relay.start()
    time.sleep(0.1)
    right.sendall(b"still-open")

    assert client.recv(10) == b"still-open"
    client.close()
    right.close()
    relay.join(timeout=1)


def test_given_health_probe_when_handled_then_desktop_does_not_start() -> None:
    runtime = load_runtime()
    starts = 0

    def start() -> None:
        nonlocal starts
        starts += 1

    lifecycle = runtime.BrowserLifecycle(start, lambda: None, idle_timeout=0.05)
    server = runtime.DemandRelayServer(("127.0.0.1", 0), lifecycle, ("127.0.0.1", 9))
    thread = threading.Thread(target=server.serve_forever)
    thread.start()
    try:
        with socket.create_connection(server.server_address, timeout=1) as probe:
            probe.sendall(b"GET /healthz HTTP/1.1\r\nHost: test\r\n\r\n")
            response = probe.recv(1024)
        assert response.startswith(b"HTTP/1.1 200 OK")
        assert starts == 0
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=1)
        lifecycle.close()


def test_given_active_lease_when_idle_deadline_passes_then_stop_waits_for_release() -> (
    None
):
    runtime = load_runtime()
    events: list[str] = []
    lifecycle = runtime.BrowserLifecycle(
        lambda: events.append("start"),
        lambda: events.append("stop"),
        idle_timeout=0.05,
    )
    try:
        lifecycle.acquire()
        time.sleep(0.12)
        assert events == ["start"]

        lifecycle.release()
        wait_for(lambda: events == ["start", "stop"])
    finally:
        lifecycle.close()


def test_given_concurrent_cold_leases_when_acquired_then_desktop_starts_once() -> None:
    runtime = load_runtime()
    starts = 0
    running = False
    start_gate = threading.Event()

    def start() -> None:
        nonlocal running, starts
        if running:
            return
        starts += 1
        start_gate.wait(timeout=1)
        running = True

    lifecycle = runtime.BrowserLifecycle(start, lambda: None, idle_timeout=1)
    workers = [threading.Thread(target=lifecycle.acquire) for _ in range(3)]
    try:
        for worker in workers:
            worker.start()
        time.sleep(0.05)
        start_gate.set()
        for worker in workers:
            worker.join(timeout=1)
        assert starts == 1
    finally:
        for _ in workers:
            lifecycle.release()
        lifecycle.close()


def test_given_two_relay_servers_when_created_then_handlers_are_isolated() -> None:
    runtime = load_runtime()
    first_lifecycle = runtime.BrowserLifecycle(lambda: None, lambda: None, 1)
    second_lifecycle = runtime.BrowserLifecycle(lambda: None, lambda: None, 1)
    first = runtime.DemandRelayServer(
        ("127.0.0.1", 0), first_lifecycle, ("127.0.0.1", 1)
    )
    second = runtime.DemandRelayServer(
        ("127.0.0.1", 0), second_lifecycle, ("127.0.0.1", 2)
    )
    try:
        assert first.RequestHandlerClass is not second.RequestHandlerClass
    finally:
        first.server_close()
        second.server_close()
        first_lifecycle.close()
        second_lifecycle.close()


def cdp_probe(body: bytes = b'{"Browser": "Chromium/139.0"}') -> mock.MagicMock:
    probe = mock.MagicMock()
    probe.__enter__.return_value = probe
    probe.recv.return_value = body
    return probe


def test_given_orphaned_desktop_when_starting_then_locks_stay_and_no_relaunch() -> (
    None
):
    runtime = load_runtime()
    desktop = runtime.DesktopProcess(1000)
    dead_supervisor = mock.Mock()
    dead_supervisor.poll.return_value = 129
    desktop._process = dead_supervisor

    with (
        mock.patch.object(
            runtime.socket, "create_connection", return_value=cdp_probe()
        ),
        mock.patch.object(runtime.DesktopProcess, "_adopt_owned_pids") as adopt,
        mock.patch.object(runtime.subprocess, "Popen") as popen,
    ):
        desktop.start()

    adopt.assert_called_once()
    popen.assert_not_called()


def test_given_half_dead_desktop_when_starting_then_stop_runs_before_relaunch() -> (
    None
):
    runtime = load_runtime()
    desktop = runtime.DesktopProcess(1000)
    stops: list[str] = []
    desktop.stop = lambda: stops.append("stop")
    launched = mock.Mock()
    launched.poll.return_value = None

    with (
        mock.patch.object(
            runtime.socket,
            "create_connection",
            side_effect=[OSError(), cdp_probe()],
        ),
        mock.patch.object(runtime.DesktopProcess, "_owned_pids", return_value=[123]),
        mock.patch.object(runtime.subprocess, "Popen", return_value=launched),
    ):
        desktop.start()

    assert stops == ["stop"]


def test_given_non_cdp_listener_when_starting_then_not_adopted() -> None:
    runtime = load_runtime()
    desktop = runtime.DesktopProcess(1000)
    launched = mock.Mock()
    launched.poll.return_value = None
    calls = 0

    def fake_connect(addr: object, timeout: float = 0) -> mock.MagicMock:
        nonlocal calls
        calls += 1
        if calls == 1:
            return cdp_probe(b"not a browser")
        return cdp_probe()

    with (
        mock.patch.object(
            runtime.socket, "create_connection", side_effect=fake_connect
        ),
        mock.patch.object(runtime.DesktopProcess, "_owned_pids", return_value=[]),
        mock.patch.object(
            runtime.subprocess, "Popen", return_value=launched
        ) as popen,
    ):
        desktop.start()

    popen.assert_called_once()


def test_given_reused_pid_when_signaling_then_unrelated_process_is_skipped() -> None:
    runtime = load_runtime()
    desktop = runtime.DesktopProcess(1000)
    desktop._known_pids = [(4321, 111)]

    with mock.patch.object(
        runtime.DesktopProcess, "_proc_start_time", return_value=222
    ):
        assert desktop._owned_pids() == []


def test_given_failed_startup_when_reacquiring_then_cooldown_fails_fast() -> None:
    runtime = load_runtime()
    starts = 0

    def start() -> None:
        nonlocal starts
        starts += 1
        raise OSError("boom")

    lifecycle = runtime.BrowserLifecycle(start, lambda: None, idle_timeout=60)
    try:
        with pytest.raises(OSError):
            lifecycle.acquire()
        with pytest.raises(OSError):
            lifecycle.acquire()
        assert starts == 1
    finally:
        lifecycle.close()


def test_given_concurrent_failed_startups_when_acquiring_then_start_runs_once() -> (
    None
):
    runtime = load_runtime()
    starts = 0
    gate = threading.Event()

    def start() -> None:
        nonlocal starts
        starts += 1
        gate.wait(timeout=2)
        raise OSError("boom")

    lifecycle = runtime.BrowserLifecycle(start, lambda: None, idle_timeout=60)
    errors: list[OSError] = []

    def worker() -> None:
        try:
            lifecycle.acquire()
        except OSError as error:
            errors.append(error)

    threads = [threading.Thread(target=worker) for _ in range(3)]
    try:
        for thread in threads:
            thread.start()
        time.sleep(0.1)
        gate.set()
        for thread in threads:
            thread.join(timeout=2)
        assert starts == 1
        assert len(errors) == 3
    finally:
        lifecycle.close()
