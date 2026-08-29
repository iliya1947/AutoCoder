"""Model-provider contract and lifecycle management for a local Ollama install."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Protocol
from urllib import error, parse, request


class ProviderError(RuntimeError):
    """A user-actionable model provider failure."""


@dataclass(frozen=True)
class Message:
    role: str
    content: str


class ModelProvider(Protocol):
    def chat(self, messages: list[Message]) -> Message: ...


class OllamaRuntime:
    """Ensure a loopback Ollama API is ready, without owning existing processes."""

    def __init__(
        self,
        api_root: str,
        *,
        timeout: float = 20.0,
        opener: Callable[..., object] | None = None,
        executable_finder: Callable[[], Path | None] | None = None,
        process_launcher: Callable[[Path], subprocess.Popen[bytes]] | None = None,
        monotonic: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], None] = time.sleep,
    ) -> None:
        self.api_root = api_root.rstrip("/")
        self.timeout = timeout
        self.opener = opener or request.urlopen
        self.executable_finder = executable_finder or self.find_executable
        self.process_launcher = process_launcher or self.launch
        self.monotonic = monotonic
        self.sleep = sleep
        self.last_readiness_error: str | None = None

    @staticmethod
    def find_executable() -> Path | None:
        """Use Ollama's documented Windows install directory, then the user PATH."""
        if sys.platform == "win32":
            local_app_data = os.environ.get("LOCALAPPDATA")
            if local_app_data:
                installed = Path(local_app_data) / "Programs" / "Ollama" / "ollama.exe"
                if installed.is_file():
                    return installed.resolve()
        discovered = shutil.which("ollama")
        return Path(discovered).resolve() if discovered else None

    @staticmethod
    def launch(executable: Path) -> subprocess.Popen[bytes]:
        creationflags = subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0
        return subprocess.Popen(
            [str(executable), "serve"],
            shell=False,
            cwd=str(executable.parent),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=creationflags,
        )

    def _get_json(self, path: str, timeout: float) -> object:
        with self.opener(f"{self.api_root}{path}", timeout=timeout) as response:
            return json.load(response)  # type: ignore[arg-type]

    def is_ready(self) -> bool:
        self.last_readiness_error = None
        try:
            result = self._get_json("/api/version", min(1.0, self.timeout))
            return isinstance(result, dict) and isinstance(result.get("version"), str)
        except error.HTTPError as exc:
            self.last_readiness_error = (
                f"Ollama readiness endpoint at {self.api_root}/api/version returned HTTP {exc.code}."
            )
            return False
        except (error.URLError, TimeoutError, OSError, json.JSONDecodeError):
            return False

    def ensure_ready(self) -> None:
        if self.is_ready():
            return
        if self.last_readiness_error:
            raise ProviderError(self.last_readiness_error)
        if os.environ.get("AUTOCODER_OLLAMA_MANAGED") == "1":
            raise ProviderError(
                f"AutoCoder could not connect to its managed Ollama service at {self.api_root}."
            )
        executable = self.executable_finder()
        if executable is None:
            raise ProviderError(
                "Local Ollama was not found. Install Ollama for Windows; expected "
                r"%LOCALAPPDATA%\Programs\Ollama\ollama.exe."
            )
        try:
            process = self.process_launcher(executable)
        except OSError as exc:
            raise ProviderError(f"Failed to start Ollama at '{executable}': {exc}") from exc

        deadline = self.monotonic() + self.timeout
        while self.monotonic() < deadline:
            if self.is_ready():
                return
            exit_code = process.poll()
            if exit_code is not None:
                raise ProviderError(
                    f"Ollama at '{executable}' exited before its API was ready (exit code {exit_code})."
                )
            self.sleep(0.25)
        raise ProviderError(
            f"Timed out after {self.timeout:g} seconds waiting for Ollama at '{executable}' "
            f"to become ready at {self.api_root}."
        )

    def ensure_model(self, model: str) -> None:
        try:
            result = self._get_json("/api/tags", min(5.0, self.timeout))
        except (error.URLError, error.HTTPError, TimeoutError, OSError) as exc:
            raise ProviderError(f"Could not list local Ollama models at {self.api_root}: {exc}") from exc
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise ProviderError("Ollama returned invalid JSON while listing local models.") from exc
        models = result.get("models") if isinstance(result, dict) else None
        names = {
            item.get("name")
            for item in models
            if isinstance(item, dict) and isinstance(item.get("name"), str)
        } if isinstance(models, list) else set()
        if model not in names:
            raise ProviderError(
                f"Required Ollama model '{model}' is not installed. Install it before using AutoCoder; "
                "automatic model downloads are disabled."
            )


class OllamaProvider:
    def __init__(
        self,
        url: str | None = None,
        model: str | None = None,
        runtime: OllamaRuntime | None = None,
        opener: Callable[..., object] | None = None,
    ) -> None:
        self.url = url or os.environ.get("AUTOCODER_OLLAMA_URL", "http://127.0.0.1:11434/api/chat")
        self.model = model or os.environ.get("AUTOCODER_OLLAMA_MODEL", "qwen2.5-coder:7b")
        self.opener = opener or request.urlopen
        parsed = parse.urlsplit(self.url)
        self.is_local = parsed.scheme in {"http", "https"} and parsed.hostname in {
            "127.0.0.1", "localhost", "::1"
        }
        api_root = parse.urlunsplit((parsed.scheme, parsed.netloc, "", "", ""))
        self.runtime = runtime or OllamaRuntime(api_root, opener=self.opener)

    def chat(self, messages: list[Message]) -> Message:
        # Explicitly configured remote providers remain untouched: process and
        # model lifecycle management applies only to a loopback Ollama endpoint.
        if self.is_local:
            self.runtime.ensure_ready()
            self.runtime.ensure_model(self.model)

        body = json.dumps(
            {"model": self.model, "messages": [message.__dict__ for message in messages], "stream": False},
            ensure_ascii=False,
        ).encode("utf-8")
        http_request = request.Request(
            self.url, data=body, headers={"Content-Type": "application/json"}, method="POST"
        )
        try:
            with self.opener(http_request, timeout=120) as response:
                result = json.load(response)  # type: ignore[arg-type]
        except error.HTTPError as exc:
            details = exc.read().decode("utf-8", errors="replace").strip()
            suffix = f" Response: {details}" if details else ""
            raise ProviderError(f"Ollama returned HTTP {exc.code}.{suffix}") from exc
        except (error.URLError, TimeoutError) as exc:
            raise ProviderError(f"Cannot connect to Ollama at {self.url}: {exc}") from exc
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise ProviderError("Ollama returned invalid JSON.") from exc

        message = result.get("message")
        if not isinstance(message, dict) or not isinstance(message.get("content"), str):
            raise ProviderError("Ollama response does not contain an assistant message.")
        return Message(role="assistant", content=message["content"])
