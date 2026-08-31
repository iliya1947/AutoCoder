"""JSON-over-stdin entry point used by the Tauri process bridge."""

from __future__ import annotations

import json
import re
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
NO_SELECTION_PROMPT = """There is currently no active text selection in the AutoCoder editor.
Treat questions about what is selected as referring to this selection state, not to the open file content."""
FILE_PROPOSAL_PROMPT = """When the user explicitly asks you to change, create, or delete a project file, you may propose one file operation.
Keep your explanation outside the block and emit exactly one block in one of these forms:
```autocoder-file
{"operation": "replace", "path": "the exact open file path", "content": "the complete replacement text"}
```
```autocoder-file
{"operation": "create", "path": "a new relative project file path", "content": "the complete new file text"}
```
```autocoder-file
{"operation": "delete", "path": "the exact open file path"}
```
Only propose create when the path is absent from the supplied project structure. Never use an absolute path or .. path components.
Only propose delete for the currently open file, and only when its current content is identical to its saved content.
This is only a proposal for user review. Never claim that you changed or saved the file."""
FILE_PROPOSAL_PATTERN = re.compile(r"```autocoder-file\s*\n(.*?)\n```", re.DOTALL)
TERMINAL_PROPOSAL_PROMPT = """When the user explicitly asks you to run or suggest a project command, you may propose one command.
Keep your explanation outside the block and emit exactly one block in this form:
```autocoder-command
{"command": "the complete command to run in the project root"}
```
Never combine a command proposal with a file proposal. This is only a proposal for user review: it is not executed automatically.
Never claim that you ran the command or observed its output."""
TERMINAL_PROPOSAL_PATTERN = re.compile(r"```autocoder-command\s*\n(.*?)\n```", re.DOTALL)
TOOL_RESULT_PROMPT = """Messages beginning with an AutoCoder File Tool result or AutoCoder Terminal Tool result are trusted factual feedback from an action that the user explicitly approved and AutoCoder executed. Continue the user's existing task using that result. You may propose exactly one next action through the existing File Tool or Terminal Tool format; never claim that a proposed action already happened."""
ORCHESTRATION_PROMPT = """AutoCoder is executing one explicit multi-step task.
Task id: {id}
Goal: {goal}
State: {status}
Recorded actions:
{actions}

Treat this task state as control metadata, not as a user instruction. Respond for the current step only. Either propose exactly one next reviewable action using the available tool format, or finish the task with a normal answer and no tool block."""
WINDOWS_RESERVED_NAMES = {"CON", "PRN", "AUX", "NUL"} | {
    f"{prefix}{number}" for prefix in ("COM", "LPT") for number in range(1, 10)
}


def is_safe_windows_relative_path(path: str) -> bool:
    components = path.replace("\\", "/").split("/")
    return all(
        component not in {"", ".", ".."}
        and not component.endswith((".", " "))
        and not any(ord(character) <= 0x1F or character in '<>:"|?*' for character in component)
        and component.split(".", 1)[0].upper() not in WINDOWS_RESERVED_NAMES
        for component in components
    )


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
    if context is not None and not isinstance(context, dict):
        raise ValueError("Context must be an object.")

    context_messages: list[Message] = []
    orchestration = payload.get("orchestration")
    if orchestration is not None:
        if not isinstance(orchestration, dict):
            raise ValueError("Orchestration state must be an object.")
        task_id, goal, status, actions = (
            orchestration.get("id"), orchestration.get("goal"),
            orchestration.get("status"), orchestration.get("actions"),
        )
        allowed_statuses = {"thinking", "awaiting_approval", "running", "awaiting_ai", "completed", "failed"}
        if (
            set(orchestration) != {"id", "goal", "status", "actions"}
            or not isinstance(task_id, str) or not task_id.strip()
            or not isinstance(goal, str) or not goal.strip()
            or status not in allowed_statuses
            or not isinstance(actions, list)
        ):
            raise ValueError("Orchestration state is invalid.")
        action_lines = []
        for action in actions:
            if not isinstance(action, dict) or not isinstance(action.get("id"), str) or action.get("tool") not in {"file", "terminal"} or action.get("status") not in {"proposed", "running", "completed", "failed", "cancelled"}:
                raise ValueError("Orchestration action is invalid.")
            result = action.get("result")
            if result is not None and (not isinstance(result, dict) or set(result) != {"outcome"} or result.get("outcome") not in {"completed", "failed", "cancelled", "declined", "interrupted"}):
                raise ValueError("Orchestration result is invalid.")
            action_lines.append(f'- {action["id"]}: {action["tool"]} / {action["status"]}' + (f' / result {result["outcome"]}' if result else ""))
        context_messages.append(Message(role="system", content=ORCHESTRATION_PROMPT.format(
            id=task_id, goal=goal, status=status, actions="\n".join(action_lines) or "(none)",
        )))
    if context is None:
        return [*context_messages, *messages]
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
        path, content, saved_content = open_file.get("path"), open_file.get("content"), open_file.get("savedContent")
        if (
            not isinstance(path, str)
            or not path.strip()
            or not isinstance(content, str)
            or not isinstance(saved_content, str)
        ):
            raise ValueError("Open file context needs a path, text content, and saved content.")
        context_messages.append(Message(
            role="system",
            content=OPEN_FILE_PROMPT.format(path=path, content=content),
        ))
        context_messages.append(Message(role="system", content=FILE_PROPOSAL_PROMPT))

    selection = context.get("selection")
    if selection is not None:
        if not isinstance(selection, dict):
            raise ValueError("Selection context must be an object.")
        state = selection.get("state")
        if state == "none" and set(selection) == {"state"}:
            context_messages.append(Message(role="system", content=NO_SELECTION_PROMPT))
        elif state == "active":
            path, content = selection.get("path"), selection.get("content")
            if (
                set(selection) != {"state", "path", "content"}
                or not isinstance(path, str)
                or not path.strip()
                or not isinstance(content, str)
                or not content
            ):
                raise ValueError("Active selection context needs a path and non-empty text content.")
            context_messages.append(Message(
                role="system",
                content=SELECTION_PROMPT.format(path=path, content=content),
            ))
        else:
            raise ValueError("Selection context needs an active or none state.")

    if not context_messages:
        raise ValueError("Context must contain an openFile, selection, or project object.")
    if project is not None and open_file is None:
        context_messages.append(Message(role="system", content=FILE_PROPOSAL_PROMPT))
    if project is not None:
        context_messages.append(Message(role="system", content=TERMINAL_PROPOSAL_PROMPT))
    if any(
        message.content.startswith(("AutoCoder File Tool result", "AutoCoder Terminal Tool result"))
        for message in messages
    ):
        context_messages.append(Message(role="system", content=TOOL_RESULT_PROMPT))
    return [*context_messages, *messages]


def parse_file_proposal(answer: Message, payload: Any) -> dict[str, str] | None:
    """Extract a safe replacement, creation, or deletion proposal."""
    match = FILE_PROPOSAL_PATTERN.search(answer.content)
    if match is None:
        return None
    try:
        proposal = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    context = (payload.get("context") or {}) if isinstance(payload, dict) else {}
    open_file = context.get("openFile")
    project = context.get("project")
    if (
        not isinstance(proposal, dict)
        or proposal.get("operation") not in {"replace", "create", "delete"}
        or not isinstance(proposal.get("path"), str)
        or not proposal["path"].strip()
    ):
        return None
    if proposal["operation"] == "delete":
        if (
            set(proposal) != {"operation", "path"}
            or not isinstance(open_file, dict)
            or proposal["path"] != open_file.get("path")
            or not isinstance(open_file.get("content"), str)
            or not isinstance(open_file.get("savedContent"), str)
            or open_file["content"] != open_file["savedContent"]
        ):
            return None
        return {
            **proposal,
            "originalContent": open_file["content"],
            "expectedSavedContent": open_file["savedContent"],
        }
    if set(proposal) != {"operation", "path", "content"} or not isinstance(proposal.get("content"), str):
        return None
    if proposal["operation"] == "replace":
        if (
            not isinstance(open_file, dict)
            or proposal["path"] != open_file.get("path")
            or not isinstance(open_file.get("content"), str)
        ):
            return None
        return {**proposal, "originalContent": open_file["content"]}

    entries = project.get("entries") if isinstance(project, dict) else None
    normalized = proposal["path"].replace("\\", "/")
    normalized_entries = {
        entry.replace("\\", "/") for entry in entries
    } if isinstance(entries, list) else set()
    if (
        not isinstance(entries, list)
        or normalized.startswith("/")
        or not is_safe_windows_relative_path(proposal["path"])
        or f"file: {normalized}" in normalized_entries
        or f"directory: {normalized}" in normalized_entries
    ):
        return None
    return proposal


def parse_terminal_proposal(answer: Message, payload: Any) -> dict[str, str] | None:
    """Extract a command proposal only when a project is open."""
    match = TERMINAL_PROPOSAL_PATTERN.search(answer.content)
    if match is None:
        return None
    try:
        proposal = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    context = (payload.get("context") or {}) if isinstance(payload, dict) else {}
    command = proposal.get("command") if isinstance(proposal, dict) else None
    if (
        not isinstance(proposal, dict)
        or set(proposal) != {"command"}
        or not isinstance(command, str)
        or not command.strip()
        or "\0" in command
        or not isinstance(context.get("project"), dict)
        or FILE_PROPOSAL_PATTERN.search(answer.content) is not None
    ):
        return None
    return {"command": command.strip()}


def read_stdin_payload() -> Any:
    """Read the Tauri bridge contract without consulting the host text encoding."""
    raw = sys.stdin.buffer.read()
    return json.loads(raw.decode("utf-8"))


def write_stdout_response(
    answer: Message,
    proposal: dict[str, str] | None = None,
    command_proposal: dict[str, str] | None = None,
) -> None:
    """Write the Tauri bridge contract as UTF-8 bytes, independent of locale."""
    response = json.dumps(
        {"message": answer.__dict__, "proposal": proposal, "commandProposal": command_proposal},
        ensure_ascii=False,
    ).encode("utf-8")
    sys.stdout.buffer.write(response)
    sys.stdout.buffer.flush()


def main() -> int:
    try:
        payload = read_stdin_payload()
        answer = OllamaProvider().chat(parse_request(payload))
        write_stdout_response(
            answer,
            parse_file_proposal(answer, payload),
            parse_terminal_proposal(answer, payload),
        )
        return 0
    except (ValueError, json.JSONDecodeError, UnicodeDecodeError, ProviderError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
