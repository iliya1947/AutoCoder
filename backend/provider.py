"""Model-provider contract and the initial local Ollama implementation."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from typing import Protocol
from urllib import error, request


class ProviderError(RuntimeError):
    """A user-actionable model provider failure."""


@dataclass(frozen=True)
class Message:
    role: str
    content: str


class ModelProvider(Protocol):
    def chat(self, messages: list[Message]) -> Message: ...


class OllamaProvider:
    def __init__(self, url: str | None = None, model: str | None = None) -> None:
        self.url = url or os.environ.get("AUTOCODER_OLLAMA_URL", "http://127.0.0.1:11434/api/chat")
        self.model = model or os.environ.get("AUTOCODER_OLLAMA_MODEL", "qwen2.5-coder:7b")

    def chat(self, messages: list[Message]) -> Message:
        body = json.dumps(
            {
                "model": self.model,
                "messages": [message.__dict__ for message in messages],
                "stream": False,
            }
        ).encode("utf-8")
        http_request = request.Request(
            self.url, data=body, headers={"Content-Type": "application/json"}, method="POST"
        )
        try:
            with request.urlopen(http_request, timeout=120) as response:
                result = json.load(response)
        except error.HTTPError as exc:
            raise ProviderError(f"Ollama returned HTTP {exc.code}.") from exc
        except (error.URLError, TimeoutError) as exc:
            raise ProviderError("Cannot connect to Ollama. Start Ollama and verify its address.") from exc
        except (json.JSONDecodeError, UnicodeDecodeError) as exc:
            raise ProviderError("Ollama returned invalid JSON.") from exc

        message = result.get("message")
        if not isinstance(message, dict) or not isinstance(message.get("content"), str):
            raise ProviderError("Ollama response does not contain an assistant message.")
        return Message(role="assistant", content=message["content"])
