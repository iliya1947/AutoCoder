"""Executable tool contracts and deterministic orchestration action validation.

The model is a planner, not the authority on what tools exist.  This registry is
the single backend description used both to build model context and to accept an
action for the approval UI.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Any, Callable


PayloadValidator = Callable[[dict[str, Any]], bool]


@dataclass(frozen=True)
class ToolContract:
    id: str
    display_name: str
    fence: str
    operations: tuple[str, ...]
    aliases: tuple[str, ...]
    is_available: Callable[[dict[str, Any]], bool]


TOOL_CONTRACTS = (
    ToolContract(
        id="file",
        display_name="File Tool",
        fence="autocoder-file",
        operations=("create", "replace", "delete"),
        aliases=("file tool", "file-tool", "инструмент файлов", "файловый инструмент"),
        is_available=lambda context: isinstance(context.get("project"), dict)
        or isinstance(context.get("openFile"), dict),
    ),
    ToolContract(
        id="terminal",
        display_name="Terminal Tool",
        fence="autocoder-command",
        operations=("execute",),
        aliases=("terminal tool", "terminal-tool", "инструмент терминала", "терминальный инструмент"),
        is_available=lambda context: isinstance(context.get("project"), dict),
    ),
)


def available_tool_contracts(payload: Any) -> tuple[ToolContract, ...]:
    context = (payload.get("context") or {}) if isinstance(payload, dict) else {}
    return tuple(contract for contract in TOOL_CONTRACTS if contract.is_available(context))


def explicit_tool_constraint(goal: str) -> frozenset[str] | None:
    """Compile an unambiguous canonical tool reference into an allow-list.

    This intentionally recognises registry-owned names, rather than verbs such
    as "write" or "run".  Consequently it cannot guess a preference the user
    did not state, while an explicit tool choice becomes enforceable in code.
    """
    mentioned = {
        contract.id
        for contract in TOOL_CONTRACTS
        if any(re.search(rf"(?<!\w){re.escape(alias)}(?!\w)", goal, re.IGNORECASE) for alias in contract.aliases)
    }
    return frozenset(mentioned) if len(mentioned) == 1 else None


def orchestration_contract(payload: Any) -> tuple[tuple[ToolContract, ...], frozenset[str] | None]:
    available = available_tool_contracts(payload)
    orchestration = payload.get("orchestration") if isinstance(payload, dict) else None
    goal = orchestration.get("goal", "") if isinstance(orchestration, dict) else ""
    return available, explicit_tool_constraint(goal)


def render_tool_contract(payload: Any) -> str:
    available, constrained = orchestration_contract(payload)
    lines = ["Executable tool contract (authoritative; closed world):"]
    for tool in available:
        lines.append(
            f'- id={tool.id}; fence={tool.fence}; operations={",".join(tool.operations)}'
        )
    if not available:
        lines.append("- (no executable tools are available in the current context)")
    lines.append(
        "Allowed tools for this task: "
        + (", ".join(sorted(constrained)) if constrained is not None else ", ".join(t.id for t in available))
    )
    lines.append("Any tool, fence, operation, or payload outside this contract is invalid and cannot reach approval.")
    return "\n".join(lines)


def validate_selected_tool(tool: str, payload: Any) -> str | None:
    available, constrained = orchestration_contract(payload)
    available_ids = {contract.id for contract in available}
    if tool not in available_ids:
        return f"Tool '{tool}' is not available in the current factual context."
    if constrained is not None and tool not in constrained:
        required = ", ".join(sorted(constrained))
        return f"Tool '{tool}' contradicts the user's explicit tool constraint; allowed: {required}."
    return None
