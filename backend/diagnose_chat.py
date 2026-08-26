"""Byte-level Windows diagnostic for the AI Chat bridge."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib import error, request

DEFAULT_OLLAMA_URL = "http://127.0.0.1:11434/api/chat"
PAYLOAD = {
    "messages": [{"role": "user", "content": "Ответь дословно только содержимым открытого файла"}],
    "context": {"openFile": {"path": "АвтоКодер_тестовый файл.txt", "content": "123 123 123"}},
}


def byte_report(data: bytes) -> dict[str, Any]:
    report: dict[str, Any] = {
        "length": len(data),
        "hex": data.hex(" "),
        "hasUtf8Bom": data.startswith(b"\xef\xbb\xbf"),
    }
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as exc:
        report["utf8DecodeError"] = str(exc)
        return report
    report["utf8Text"] = text
    report["controls"] = [
        {"index": index, "codepoint": f"U+{ord(character):04X}"}
        for index, character in enumerate(text)
        if ord(character) < 0x20
    ]
    try:
        report["json"] = json.loads(text)
    except json.JSONDecodeError as exc:
        report["jsonDecodeError"] = str(exc)
    return report


class CaptureServer(ThreadingHTTPServer):
    captured: dict[str, Any]
    target_url: str


class CaptureHandler(BaseHTTPRequestHandler):
    def do_POST(self) -> None:  # noqa: N802 - inherited API name
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        self.server.captured = {  # type: ignore[attr-defined]
            "url": f"http://{self.headers['Host']}{self.path}",
            "method": self.command,
            "headers": list(self.headers.items()),
            "contentType": self.headers.get("Content-Type"),
            "body": byte_report(body),
        }
        forwarded = request.Request(
            self.server.target_url,  # type: ignore[attr-defined]
            data=body,
            headers={"Content-Type": self.headers.get("Content-Type", "")},
            method="POST",
        )
        try:
            with request.urlopen(forwarded, timeout=120) as response:
                response_body, status = response.read(), response.status
                content_type = response.headers.get("Content-Type", "application/json")
        except error.HTTPError as exc:
            response_body, status = exc.read(), exc.code
            content_type = exc.headers.get("Content-Type", "application/json")
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(response_body)))
        self.end_headers()
        self.wfile.write(response_body)

    def log_message(self, _format: str, *_args: object) -> None:
        return


def capture_stdin() -> int:
    print(json.dumps({"stdin": byte_report(sys.stdin.buffer.read())}, ensure_ascii=False, indent=2))
    return 0


def probe(ollama_url: str) -> int:
    stdin_bytes = json.dumps(PAYLOAD, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    server = CaptureServer(("127.0.0.1", 0), CaptureHandler)
    server.captured, server.target_url = {}, ollama_url
    thread = threading.Thread(target=server.handle_request, daemon=True)
    thread.start()
    environment = os.environ.copy()
    environment["AUTOCODER_OLLAMA_URL"] = f"http://127.0.0.1:{server.server_port}/api/chat"
    completed = subprocess.run(
        [sys.executable, str(Path(__file__).with_name("main.py"))],
        input=stdin_bytes,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=environment,
        check=False,
    )
    thread.join(timeout=125)
    server.server_close()
    report = {
        "source": "Python json.dumps(..., ensure_ascii=False).encode('utf-8')",
        "ollamaTargetUrl": ollama_url,
        "backendStdin": byte_report(stdin_bytes),
        "outgoingHttpRequest": server.captured or "No request reached the capture proxy.",
        "backendExitCode": completed.returncode,
        "backendStdout": byte_report(completed.stdout),
        "backendStderr": byte_report(completed.stderr),
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if completed.returncode == 0 and server.captured else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--capture-stdin", action="store_true", help="report exact stdin bytes")
    parser.add_argument("--ollama-url", default=os.environ.get("AUTOCODER_OLLAMA_URL", DEFAULT_OLLAMA_URL))
    arguments = parser.parse_args()
    return capture_stdin() if arguments.capture_stdin else probe(arguments.ollama_url)


if __name__ == "__main__":
    raise SystemExit(main())
