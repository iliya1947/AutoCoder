"""JSON-over-stdin entry point used by the Tauri process bridge."""

from __future__ import annotations

import json
import sys
from typing import Any

from provider import Message, OllamaProvider, ProviderError

ALLOWED_ROLES = {"system", "user", "assistant"}


def parse_messages(payload: Any) -> list[Message]:
    if not isinstance(payload, dict) or not isinstance(payload.get("messages"), list):
        raise ValueError("Request must contain a messages array.")
    messages: list[Message] = []
    for item in payload["messages"]:
        if not isinstance(item, dict):
            raise ValueError("Each message must be an object.")
        role, content = item.get("role"), item.get("content")
        if role not in ALLOWED_ROLES or not isinstance(content, str) or not content.strip():
            raise ValueError("Each message needs a valid role and non-empty content.")
        messages.append(Message(role=role, content=content))
    if not messages:
        raise ValueError("At least one message is required.")
    return messages


def main() -> int:
    try:
        payload = json.load(sys.stdin)
        answer = OllamaProvider().chat(parse_messages(payload))
        json.dump({"message": answer.__dict__}, sys.stdout, ensure_ascii=False)
        return 0
    except (ValueError, json.JSONDecodeError, ProviderError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
