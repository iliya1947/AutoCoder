"""JSON-over-stdin entry point used by the Tauri process bridge."""

from __future__ import annotations

import json
import sys
from typing import Any

from provider import Message, OllamaProvider, ProviderError

ALLOWED_ROLES = {"system", "user", "assistant"}
OPEN_FILE_PROMPT = """The user currently has this project file open in AutoCoder.
Use its path and content as context for the user's request.
Path: {path}

<open_file>
{content}
</open_file>"""


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


def parse_request(payload: Any) -> list[Message]:
    messages = parse_messages(payload)
    context = payload.get("context")
    if context is None:
        return messages
    if not isinstance(context, dict) or not isinstance(context.get("openFile"), dict):
        raise ValueError("Context must contain an openFile object.")
    open_file = context["openFile"]
    path, content = open_file.get("path"), open_file.get("content")
    if not isinstance(path, str) or not path.strip() or not isinstance(content, str):
        raise ValueError("Open file context needs a path and text content.")
    file_message = Message(role="system", content=OPEN_FILE_PROMPT.format(path=path, content=content))
    return [file_message, *messages]


def main() -> int:
    try:
        payload = json.load(sys.stdin)
        answer = OllamaProvider().chat(parse_request(payload))
        json.dump({"message": answer.__dict__}, sys.stdout, ensure_ascii=False)
        return 0
    except (ValueError, json.JSONDecodeError, ProviderError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
