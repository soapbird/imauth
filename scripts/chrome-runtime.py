# /// script
# requires-python = ">=3.9"
# dependencies = []
# ///
# ─── How to run ───
# python3 /scripts/chrome-runtime.py

from __future__ import annotations

import os
import signal
import socket
import socketserver
import subprocess
import threading
import time
from collections.abc import Callable
from pathlib import Path
from types import FrameType
from typing import ClassVar, Final

HEADER_LIMIT, HEADER_TIMEOUT_SECS = 65_536, 2.0
UPSTREAM_CONNECT_TIMEOUT_SECS, DESKTOP_STOP_TIMEOUT_SECS = 10.0, 5.0
DESKTOP_STARTUP_TIMEOUT_SECS: Final = 25.0
UPSTREAM_PROBE_TIMEOUT_SECS, PROCESS_POLL_SECS, PROCESS_WAIT_SECS = 0.2, 0.1, 1.0
HEALTH_RESPONSE: Final = b"HTTP/1.1 200 OK\r\nContent-Length:2\r\n\r\nOK"


class BrowserLifecycle:
    def __init__(
        self,
        start: Callable[[], None],
        stop: Callable[[], None],
        idle_timeout: float,
    ) -> None:
        self._start = start
        self._stop = stop
        self._idle_timeout = idle_timeout
        self._condition = threading.Condition()
        self._active_leases = 0
        self._running = False
        self._idle_since = time.monotonic()
        self._closing = False
        self._monitor = threading.Thread(target=self._monitor_idle, daemon=True)
        self._monitor.start()

    def acquire(self) -> None:
        with self._condition:
            self._active_leases += 1
            try:
                self._start()
            except OSError:
                self._active_leases -= 1
                self._running = False
                raise
            self._running = True

    def release(self) -> None:
        with self._condition:
            self._active_leases -= 1
            if self._active_leases == 0:
                self._idle_since = time.monotonic()
                self._condition.notify_all()

    def close(self) -> None:
        with self._condition:
            self._closing = True
            self._condition.notify_all()
        self._monitor.join()
        with self._condition:
            if self._running:
                self._stop()
                self._running = False

    def _monitor_idle(self) -> None:
        with self._condition:
            while not self._closing:
                if self._running and self._active_leases == 0:
                    remaining = self._idle_timeout - (
                        time.monotonic() - self._idle_since
                    )
                    if remaining <= 0:
                        self._stop()
                        self._running = False
                        continue
                    self._condition.wait(timeout=remaining)
                    continue
                self._condition.wait()


def relay_sockets(client: socket.socket, upstream: socket.socket) -> None:
    client.settimeout(None)
    upstream.settimeout(None)

    def pump(source: socket.socket, destination: socket.socket) -> None:
        try:
            while chunk := source.recv(65_536):
                destination.sendall(chunk)
        except OSError:
            return
        finally:
            for peer in (source, destination):
                try:
                    peer.shutdown(socket.SHUT_RDWR)
                except OSError:
                    continue

    threads = (
        threading.Thread(target=pump, args=(client, upstream), daemon=True),
        threading.Thread(target=pump, args=(upstream, client), daemon=True),
    )
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()


class DemandRelayHandler(socketserver.BaseRequestHandler):
    lifecycle: ClassVar[BrowserLifecycle]
    upstream_address: ClassVar[tuple[str, int]]

    def handle(self) -> None:
        request = self._read_request()
        if request is None:
            return
        if request.split(b" ", 2)[:2] == [b"GET", b"/healthz"]:
            self.request.sendall(HEALTH_RESPONSE)
            return
        try:
            self.lifecycle.acquire()
        except OSError:
            return
        try:
            upstream = socket.create_connection(
                self.upstream_address,
                timeout=UPSTREAM_CONNECT_TIMEOUT_SECS,
            )
            with upstream:
                upstream.settimeout(None)
                upstream.sendall(request)
                relay_sockets(self.request, upstream)
        except OSError:
            return
        finally:
            self.lifecycle.release()

    def _read_request(self) -> bytes | None:
        self.request.settimeout(HEADER_TIMEOUT_SECS)
        chunks = bytearray()
        try:
            while b"\r\n\r\n" not in chunks:
                chunk = self.request.recv(min(4096, HEADER_LIMIT - len(chunks)))
                if not chunk:
                    return None
                chunks.extend(chunk)
                if len(chunks) == HEADER_LIMIT and b"\r\n\r\n" not in chunks:
                    return None
        except (OSError, TimeoutError):
            return None
        return bytes(chunks)


class DemandRelayServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def __init__(
        self,
        address: tuple[str, int],
        lifecycle: BrowserLifecycle,
        upstream_address: tuple[str, int],
    ) -> None:
        handler_type = type(
            "ScopedDemandRelayHandler",
            (DemandRelayHandler,),
            {"lifecycle": lifecycle, "upstream_address": upstream_address},
        )
        super().__init__(address, handler_type)


class DesktopProcess:
    def __init__(
        self, user_uid: int, startup_timeout: float = DESKTOP_STARTUP_TIMEOUT_SECS
    ) -> None:
        self._user_uid = user_uid
        self._startup_timeout = startup_timeout
        self._process: subprocess.Popen[bytes] | None = None

    def start(self) -> None:
        if self._process is not None and self._process.poll() is None:
            try:
                with socket.create_connection(
                    ("127.0.0.1", 9222), timeout=UPSTREAM_PROBE_TIMEOUT_SECS
                ):
                    return
            except OSError:
                self.stop()
        for lock_path in Path("/home/kasm-user/.config/chromium").glob("Singleton*"):
            try:
                lock_path.unlink()
            except FileNotFoundError:
                continue
        command = "runuser -u kasm-user -- /dockerstartup/kasm_default_profile.sh /dockerstartup/vnc_startup.sh /dockerstartup/kasm_startup.sh --wait"
        self._process = subprocess.Popen(command.split())
        deadline = time.monotonic() + self._startup_timeout
        while time.monotonic() < deadline:
            if self._process.poll() is not None:
                self.stop()
                raise OSError("desktop startup exited before CDP became ready")
            try:
                with socket.create_connection(
                    ("127.0.0.1", 9222), timeout=UPSTREAM_PROBE_TIMEOUT_SECS
                ):
                    return
            except OSError:
                time.sleep(PROCESS_POLL_SECS)
        self.stop()
        raise OSError("desktop CDP did not become ready before timeout")

    def stop(self) -> None:
        if self._process is not None and self._process.poll() is None:
            self._process.terminate()
        self._signal_owned(signal.SIGTERM)
        deadline = time.monotonic() + DESKTOP_STOP_TIMEOUT_SECS
        while self._owned_pids() and time.monotonic() < deadline:
            time.sleep(PROCESS_POLL_SECS)
        self._signal_owned(signal.SIGKILL)
        if self._process is not None:
            try:
                self._process.wait(timeout=PROCESS_WAIT_SECS)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
        self._process = None

    def _signal_owned(self, signal_number: signal.Signals) -> None:
        for pid in self._owned_pids():
            try:
                os.kill(pid, signal_number)
            except ProcessLookupError:
                continue

    def _owned_pids(self) -> list[int]:
        pids: list[int] = []
        for status_path in Path("/proc").glob("[0-9]*/status"):
            try:
                status_lines = status_path.read_text().splitlines()
                owner_uid = status_path.parent.stat().st_uid
                state_line = next(
                    line for line in status_lines if line.startswith("State:")
                )
            except (OSError, StopIteration):
                continue
            if owner_uid == self._user_uid and "Z" not in state_line.split()[1]:
                pids.append(int(status_path.parent.name))
        return pids


def reap_children(_signum: int, _frame: FrameType | None) -> None:
    try:
        while os.waitpid(-1, os.WNOHANG)[0] != 0:
            pass
    except ChildProcessError:
        return


def main() -> None:
    idle_timeout = float(os.environ.get("IMAUTH_BROWSER_IDLE_TIMEOUT_SECS", "60"))
    bind_address = os.environ.get("CDP_RELAY_BIND_ADDR") or socket.gethostbyname(
        socket.gethostname()
    )
    desktop = DesktopProcess(user_uid=int(os.environ.get("KASM_USER_UID", "1000")))
    lifecycle = BrowserLifecycle(desktop.start, desktop.stop, idle_timeout)
    signal.signal(signal.SIGCHLD, reap_children)
    with DemandRelayServer(
        (bind_address, 9223), lifecycle, ("127.0.0.1", 9222)
    ) as server:

        def stop_server(_signum: int, _frame: FrameType | None) -> None:
            threading.Thread(target=server.shutdown, daemon=True).start()

        signal.signal(signal.SIGTERM, stop_server)
        try:
            server.serve_forever()
        finally:
            lifecycle.close()


if __name__ == "__main__":
    main()
