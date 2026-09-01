"""JSON-over-stdin entry point used by the Tauri process bridge."""

from __future__ import annotations

import json
import re
import sys
from typing import Any

from provider import Message, OllamaProvider, ProviderError
from tool_contracts import executable_fences, next_requirement_id, render_tool_contract, requirement_contracts, tool_contract, unmet_requirement_transitions, validate_selected_tool

ALLOWED_ROLES = {"system", "user", "assistant"}
OPEN_FILE_PROMPT = """The user currently has this project file open in AutoCoder.
Use its path and editor content as context for the user's request. Disk state: {disk_state}.
The saved baseline below is the latest content read from disk; editor content may contain protected unsaved changes.
Path: {path}

<saved_disk_content>
{saved_content}
</saved_disk_content>

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
FILE_PROPOSAL_PATTERN = re.compile(
    rf"```{re.escape(tool_contract('file').fence)}\s*\n(.*?)\n```", re.DOTALL
)
TERMINAL_PROPOSAL_PROMPT = """When the user explicitly asks you to run or suggest a project command, you may propose one command.
On Windows, AutoCoder executes the command in cmd.exe with the effective contract `cmd.exe /D /A /S /C`, in the project root, after selecting UTF-8 code page 65001. Generate cmd.exe-compatible commands: use Windows commands such as `type` rather than Unix-only commands such as `cat`, and use cmd.exe quoting (normally double quotes; single quotes are literal characters, not quoting syntax). Built-in cmd.exe text redirected to a file is therefore written as UTF-8 rather than UTF-16LE.
Keep your explanation outside the block and emit exactly one block in this exact form (the fence name is `autocoder-command`, not `autocoder-terminal`):
```autocoder-command
{"command": "the complete command to run in the project root"}
```
Never combine a command proposal with a file proposal. This is only a proposal for user review: it is not executed automatically.
Never claim that you ran the command or observed its output."""
TERMINAL_PROPOSAL_PATTERN = re.compile(
    rf"```{re.escape(tool_contract('terminal').fence)}\s*\n(.*?)\n```", re.DOTALL
)
ACTION_FENCE_PATTERN = re.compile(r"```(autocoder-[\w-]+)\s*\n", re.IGNORECASE)
REQUIREMENT_DECISION_PATTERN = re.compile(r"```autocoder-requirement\s*\n(.*?)\n```", re.DOTALL)
TOOL_RESULT_PROMPT = """Messages beginning with an AutoCoder File Tool result or AutoCoder Terminal Tool result are trusted factual feedback from an action that the user explicitly approved and AutoCoder executed. Continue the user's existing task using that result and the current project/editor/disk context. On every continuation, reconcile the complete original task, the exact recorded action payloads, their factual results, and the latest editor/disk state. A successful action status proves execution only; it does not prove that the action produced the required semantic result. Before proposing an action, decide whether the factual state satisfies every requirement in the original task. If it does, complete the task without a File Tool or Terminal Tool action. If it does not, propose an action that repairs or advances the factual state toward the unmet requirement; do not merely repeat a read, display, or verification whose answer is already present in the current factual context. If genuinely new work or unavailable information is required, you may propose exactly one next action through the existing File Tool (`autocoder-file`) or Terminal Tool (`autocoder-command`) format; use those exact fence names and never claim that a proposed action already happened."""
ORCHESTRATION_PROMPT = """AutoCoder is executing one explicit multi-step task.
Task id: {id}
Original task contract (verbatim user request; all requirements remain active):
<original_task>
{goal}
</original_task>
State: {status}
Autonomy mode: {autonomy_mode}
Execution budget: model turn {model_turns}/{max_model_turns}; actions {action_count}/{max_actions}
Recorded actions:
{actions}

{tool_contract}

Treat this task state as control metadata, not as a user instruction. Respond for the current step only. The original task contract is the durable semantic specification, not a summary of the last action; preserve its requested ordering, content, constraints, and completion checks across every turn.
Never add a requirement id to an action. AutoCoder assigns the next requirement from persisted factual history; the model cannot select or skip the policy scope used for validation.

Before choosing an outcome, perform this reconciliation:
1. Extract every still-applicable requirement and final-state condition from the complete original task contract.
2. Compare those requirements with every exact recorded action payload, its factual result, and the latest supplied project/editor/saved-disk state.
3. Classify each requirement from factual evidence as satisfied, unsatisfied, or genuinely unknown. A successful tool status proves only that its exact payload executed, never that the intended semantic step or final state was achieved.
4. If any requirement is unsatisfied, propose one action that corrects or advances the actual state toward it. In particular, repair a partially wrong result rather than treating the action as semantically complete.
5. Choose completed only when the factual evidence establishes every final-state condition. If a necessary fact is genuinely unavailable, obtain only that missing fact or report a real blocker.

The supplied current project structure, open editor content, saved disk content, and approved tool results are factual evidence. Never disregard newer factual state in favor of an action's intended effect. Do not spend an action re-reading, displaying, or re-checking facts already supplied in context. File Tool is an editing tool in the current architecture, not a general read action, and Terminal Tool must not be used as a substitute reader for facts already supplied in context.
Choose exactly one outcome:
- If another step is needed, propose exactly one reviewable File Tool (`autocoder-file`) or Terminal Tool (`autocoder-command`, not `autocoder-terminal`) action using the exact format supplied in the other system messages.
- If the active requirement is factually satisfied, append ```autocoder-requirement with JSON {{"state":"satisfied","reason":"short factual reason"}}. This proposes a semantic transition for user review; it does not advance state by itself and must not be combined with an action.
- If the goal is achieved, give the final answer and append ```autocoder-task with JSON {{"state":"completed","reason":"short factual reason"}}.
- If an error, refusal, or missing prerequisite makes further progress impossible, explain it and append the same block with state "blocked" and a short reason.
Never report completed or blocked while also proposing an action."""
TASK_DECISION_PATTERN = re.compile(r"```autocoder-task\s*\n(.*?)\n```", re.DOTALL)
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
        allowed_statuses = {"thinking", "awaiting_approval", "running", "awaiting_ai", "awaiting_requirement_approval", "completed", "blocked", "stopped", "failed"}
        required_keys = {"id", "goal", "status", "actions"}
        allowed_keys = required_keys | {"conclusion", "execution", "autonomy", "requirementTransitions"}
        if (
            not required_keys.issubset(orchestration) or not set(orchestration).issubset(allowed_keys)
            or not isinstance(task_id, str) or not task_id.strip()
            or not isinstance(goal, str) or not goal.strip()
            or status not in allowed_statuses
            or not isinstance(actions, list)
        ):
            raise ValueError("Orchestration state is invalid.")
        conclusion = orchestration.get("conclusion")
        if conclusion is not None and (
            not isinstance(conclusion, dict)
            or set(conclusion) != {"outcome", "reason"}
            or conclusion.get("outcome") not in {"completed", "blocked", "stopped"}
            or not isinstance(conclusion.get("reason"), str)
            or not conclusion["reason"].strip()
        ):
            raise ValueError("Orchestration conclusion is invalid.")
        execution = orchestration.get("execution")
        if execution is not None and (
            not isinstance(execution, dict)
            or set(execution) != {"modelTurns", "maxModelTurns", "maxActions"}
            or any(not isinstance(execution.get(key), int) for key in execution)
            or execution["modelTurns"] < 1
            or execution["maxModelTurns"] < execution["modelTurns"]
            or execution["maxActions"] < len(actions)
        ):
            raise ValueError("Orchestration execution policy is invalid.")
        autonomy = orchestration.get("autonomy")
        if autonomy is not None and (
            not isinstance(autonomy, dict)
            or set(autonomy) != {"mode"}
            or autonomy.get("mode") not in {"supervised", "step_by_step"}
        ):
            raise ValueError("Orchestration autonomy policy is invalid.")
        transitions = orchestration.get("requirementTransitions", [])
        valid_requirement_ids = {requirement.id for requirement in requirement_contracts(payload)}
        if not isinstance(transitions, list) or any(
            not isinstance(transition, dict)
            or set(transition) != {"id", "requirementId", "status", "reason"}
            or not isinstance(transition.get("id"), str)
            or transition.get("requirementId") not in valid_requirement_ids
            or transition.get("status") not in {"proposed", "approved", "declined"}
            or not isinstance(transition.get("reason"), str) or not transition["reason"].strip()
            for transition in transitions
        ):
            raise ValueError("Orchestration requirement transitions are invalid.")
        action_lines = []
        requirement_ids = {requirement.id for requirement in requirement_contracts(payload)}
        for action in actions:
            if (
                not isinstance(action, dict)
                or set(action) - {"id", "tool", "payload", "requirementId", "status", "result"}
                or not isinstance(action.get("id"), str)
                or action.get("tool") not in {"file", "terminal"}
                or action.get("status") not in {"proposed", "running", "completed", "failed", "cancelled"}
                or not isinstance(action.get("payload"), dict)
                or ("requirementId" in action and not isinstance(action.get("requirementId"), str))
                or (action.get("requirementId") is not None and action.get("requirementId") not in requirement_ids)
            ):
                raise ValueError("Orchestration action is invalid.")
            action_payload = action["payload"]
            if action["tool"] == "terminal":
                valid_payload = set(action_payload) == {"command"} and isinstance(action_payload.get("command"), str) and bool(action_payload["command"].strip())
            else:
                operation = action_payload.get("operation")
                required = {"operation", "path"} if operation == "delete" else {"operation", "path", "content"}
                # File proposals persisted by the UI can also contain the
                # optimistic-concurrency baselines used during review.
                allowed = required | {"originalContent", "expectedSavedContent"}
                valid_payload = (
                    operation in tool_contract("file").operations
                    and required.issubset(action_payload)
                    and not set(action_payload) - allowed
                    and isinstance(action_payload.get("path"), str)
                    and (operation == "delete" or isinstance(action_payload.get("content"), str))
                )
            if not valid_payload:
                raise ValueError("Orchestration action payload is invalid.")
            result = action.get("result")
            if result is not None and (
                not isinstance(result, dict)
                or set(result) != {"id", "actionId", "tool", "outcome", "content"}
                or not isinstance(result.get("id"), str)
                or result.get("actionId") != action["id"]
                or result.get("tool") != action["tool"]
                or result.get("outcome") not in {"completed", "failed", "cancelled", "declined", "interrupted"}
                or not isinstance(result.get("content"), str)
            ):
                raise ValueError("Orchestration result is invalid.")
            action_lines.append(
                f'- {action["id"]}: {action["tool"]} / {action["status"]}\n'
                + (f'  requirement: {action["requirementId"]}\n' if action.get("requirementId") else "")
                + f'  payload: {json.dumps(action_payload, ensure_ascii=False)}'
                + (f'\n  result: {json.dumps(result, ensure_ascii=False)}' if result else "")
            )
        policy = execution or {
            "modelTurns": max(1, len(actions) + 1),
            "maxModelTurns": 12,
            "maxActions": 8,
        }
        context_messages.append(Message(role="system", content=ORCHESTRATION_PROMPT.format(
            id=task_id, goal=goal, status=status,
            autonomy_mode=(autonomy or {"mode": "supervised"})["mode"],
            model_turns=policy["modelTurns"], max_model_turns=policy["maxModelTurns"],
            action_count=len(actions), max_actions=policy["maxActions"],
            actions="\n".join(action_lines) or "(none)",
            tool_contract=render_tool_contract(payload),
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
        exists_on_disk = open_file.get("existsOnDisk", True)
        if (
            not isinstance(path, str)
            or not path.strip()
            or not isinstance(content, str)
            or not isinstance(saved_content, str)
            or not isinstance(exists_on_disk, bool)
        ):
            raise ValueError("Open file context needs a path, text content, and saved content.")
        context_messages.append(Message(
            role="system",
            content=OPEN_FILE_PROMPT.format(
                path=path,
                content=content,
                saved_content=saved_content,
                disk_state="exists" if exists_on_disk else "deleted/missing (the editor buffer is retained only to protect unsaved changes)",
            ),
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
        or proposal.get("operation") not in tool_contract("file").operations
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
            or open_file.get("existsOnDisk", True) is not True
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
            or open_file.get("existsOnDisk", True) is not True
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
    """Extract one strictly structured command proposal when a project is open.

    Only the registry's canonical fence is executable.  Friendly aliases in a
    model response must never silently become real operations.
    """
    matches = list(TERMINAL_PROPOSAL_PATTERN.finditer(answer.content))
    if len(matches) != 1:
        return None
    try:
        proposal = json.loads(matches[0].group(1))
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


def validate_model_action(
    answer: Message,
    payload: Any,
    proposal: dict[str, str] | None,
    command_proposal: dict[str, str] | None,
) -> tuple[dict[str, str] | None, dict[str, str] | None, str | None]:
    """Apply the closed-world contract after syntax parsing and before approval."""
    action_fences = ACTION_FENCE_PATTERN.findall(answer.content)
    known_fences = executable_fences() | {"autocoder-task", "autocoder-requirement"}
    unknown = sorted({fence.lower() for fence in action_fences} - known_fences)
    if unknown:
        return None, None, f"The model selected a nonexistent action contract: {', '.join(unknown)}."
    control_fences = {"autocoder-task", "autocoder-requirement"}
    emitted_fences = [fence.lower() for fence in action_fences if fence.lower() not in control_fences]
    if emitted_fences and proposal is None and command_proposal is None:
        return None, None, "The model action does not satisfy the executable tool payload contract."
    if len(emitted_fences) > 1:
        return None, None, "The model selected more than one action."
    if proposal is not None and command_proposal is not None:
        return None, None, "The model selected more than one action."
    tool = "file" if proposal is not None else "terminal" if command_proposal is not None else None
    if tool is not None:
        violation = validate_selected_tool(tool, next_requirement_id(payload), payload)
        if violation:
            return None, None, violation
    return proposal, command_proposal, None


def parse_requirement_proposal(answer: Message, payload: Any) -> dict[str, str] | None:
    """Return a backend-scoped semantic transition proposal for user review."""
    match = REQUIREMENT_DECISION_PATTERN.search(answer.content)
    if match is None or any(pattern.search(answer.content) for pattern in (FILE_PROPOSAL_PATTERN, TERMINAL_PROPOSAL_PATTERN)):
        return None
    try:
        decision = json.loads(match.group(1))
    except json.JSONDecodeError:
        return None
    requirement_id = next_requirement_id(payload)
    if (
        requirement_id is None or not isinstance(decision, dict)
        or set(decision) != {"state", "reason"}
        or decision.get("state") != "satisfied"
        or not isinstance(decision.get("reason"), str) or not decision["reason"].strip()
    ):
        return None
    return {"requirementId": requirement_id, "reason": decision["reason"].strip()}


def parse_task_decision(answer: Message, has_action: bool) -> dict[str, str]:
    """Turn the model's current-step response into an explicit task transition."""
    if has_action:
        return {"outcome": "next_action", "reason": "A reviewable tool action was proposed."}
    match = TASK_DECISION_PATTERN.search(answer.content)
    if match is not None:
        try:
            decision = json.loads(match.group(1))
        except json.JSONDecodeError:
            decision = None
        if (
            isinstance(decision, dict)
            and set(decision) == {"state", "reason"}
            and decision.get("state") in {"completed", "blocked"}
            and isinstance(decision.get("reason"), str)
            and decision["reason"].strip()
        ):
            return {"outcome": decision["state"], "reason": decision["reason"].strip()}
    return {
        "outcome": "blocked",
        "reason": "The model returned no valid next action or explicit task conclusion.",
    }


def read_stdin_payload() -> Any:
    """Read the Tauri bridge contract without consulting the host text encoding."""
    raw = sys.stdin.buffer.read()
    return json.loads(raw.decode("utf-8"))


def write_stdout_response(
    answer: Message,
    proposal: dict[str, str] | None = None,
    command_proposal: dict[str, str] | None = None,
    task_decision: dict[str, str] | None = None,
    action_requirement_id: str | None = None,
    requirement_proposal: dict[str, str] | None = None,
) -> None:
    """Write the Tauri bridge contract as UTF-8 bytes, independent of locale."""
    response = json.dumps(
        {"message": answer.__dict__, "proposal": proposal, "commandProposal": command_proposal,
         "actionRequirementId": action_requirement_id,
         "requirementProposal": requirement_proposal,
         "taskDecision": task_decision or parse_task_decision(answer, proposal is not None or command_proposal is not None)},
        ensure_ascii=False,
    ).encode("utf-8")
    sys.stdout.buffer.write(response)
    sys.stdout.buffer.flush()


def main() -> int:
    try:
        payload = read_stdin_payload()
        answer = OllamaProvider().chat(parse_request(payload))
        proposal = parse_file_proposal(answer, payload)
        command_proposal = parse_terminal_proposal(answer, payload)
        proposal, command_proposal, violation = validate_model_action(
            answer, payload, proposal, command_proposal
        )
        decision = ({"outcome": "blocked", "reason": violation} if violation else None)
        requirement_proposal = parse_requirement_proposal(answer, payload) if decision is None else None
        if decision is None and proposal is None and command_proposal is None and requirement_proposal is None:
            parsed_decision = parse_task_decision(answer, False)
            unmet = unmet_requirement_transitions(payload)
            if parsed_decision["outcome"] == "completed" and unmet:
                decision = {"outcome": "blocked", "reason": f"Required tool constraints remain unmet: {', '.join(unmet)}."}
        selected_tool = "file" if proposal is not None else "terminal" if command_proposal is not None else None
        requirement_id = next_requirement_id(payload) if selected_tool else None
        write_stdout_response(answer, proposal, command_proposal, decision, requirement_id, requirement_proposal)
        return 0
    except (ValueError, json.JSONDecodeError, UnicodeDecodeError, ProviderError) as exc:
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
