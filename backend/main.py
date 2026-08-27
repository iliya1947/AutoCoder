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
PROJECT_PROMPT = """This is the read-only structure of the project currently open in AutoCoder.
Use it to understand which files and directories exist. Do not assume their contents.
Project: {name}

<project_structure>
{entries}
</project_structure>"""
SELECTION_PROMPT = """The user selected this text in the currently open project file.
Give it priority when the request refers to selected code or text.
Path: {path}

<selection>
{content}
</selection>"""


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
    if not isinstance(context, dict):
        raise ValueError("Context must be an object.")

    context_messages: list[Message] = []
    project = context.get("project")
    if project is not None:
        if not isinstance(project, dict):
            raise ValueError("Project context must be an object.")
        name, entries = project.get("name"), project.get("entries")
        if (
            not isinstance(name, str)
            or not name.strip()
            or not isinstance(entries, list)
            or any(not isinstance(entry, str) or not entry.strip() for entry in entries)
        ):
            raise ValueError("Project context needs a name and text entries.")
        context_messages.append(Message(
            role="system",
            content=PROJECT_PROMPT.format(name=name, entries="\n".join(entries)),
        ))

    open_file = context.get("openFile")
    if open_file is not None:
        if not isinstance(open_file, dict):
            raise ValueError("Open file context must be an object.")
        path, content = open_file.get("path"), open_file.get("content")
        if not isinstance(path, str) or not path.strip() or not isinstance(content, str):
            raise ValueError("Open file context needs a path and text content.")
        context_messages.append(Message(
            role="system",
            content=OPEN_FILE_PROMPT.format(path=path, content=content),
        ))

    selection = context.get("selection")
    if selection is not None:
        if not isinstance(selection, dict):
            raise ValueError("Selection context must be an object.")
        path, content = selection.get("path"), selection.get("content")
        if (
            not isinstance(path, str)
            or not path.strip()
            or not isinstance(content, str)
            or not content
        ):
            raise ValueError("Selection context needs a path and non-empty text content.")
        context_messages.append(Message(
            role="system",
            content=SELECTION_PROMPT.format(path=path, content=content),
        ))

    if not context_messages:
        raise ValueError("Context must contain an openFile, selection, or project object.")
    return [*context_messages, *messages]


def read_stdin_payload() -> Any:
    """Read the Tauri bridge contract without consulting the host text encoding."""
    raw = sys.stdin.buffer.read()
    return json.loads(raw.decode("utf-8"))


def write_stdout_response(answer: Message) -> None:
    """Write the Tauri bridge contract as UTF-8 bytes, independent of locale."""
    response = json.dumps(
        {"message": answer.__dict__},
        ensure_ascii=False,
    ).encode("utf-8")
    sys.stdout.buffer.write(response)
    sys.stdout.buffer.flush()


def main() -> int:
    try:
        payload = read_stdin_payload()
        answer = OllamaProvider().chat(parse_request(payload))
        write_stdout_response(answer)
        return 0
    except (ValueError, json.JSONDecodeError, UnicodeDecodeError, ProviderError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
