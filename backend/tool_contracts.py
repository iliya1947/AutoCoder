"""Authoritative executable tools and scoped orchestration constraints.

The model chooses which factual requirement to advance, but code owns the list of
real tools and verifies that the proposed tool is permitted for that declared
requirement before the action can reach approval.
"""

from __future__ import annotations

from dataclasses import dataclass
import re
from typing import Any, Callable


@dataclass(frozen=True)
class ToolContract:
    id: str
    display_name: str
    fence: str
    operations: tuple[str, ...]
    aliases: tuple[str, ...]
    is_available: Callable[[dict[str, Any]], bool]


@dataclass(frozen=True)
class RequirementContract:
    id: str
    text: str
    required_tools: frozenset[str]
    forbidden_tools: frozenset[str]


TOOL_CONTRACTS = (
    ToolContract(
        id="file",
        display_name="File Tool",
        fence="autocoder-file",
        operations=("create", "replace", "delete"),
        aliases=("file tool", "file-tool", "file editor", "инструмент файлов", "файловый инструмент"),
        is_available=lambda context: isinstance(context.get("project"), dict)
        or isinstance(context.get("openFile"), dict),
    ),
    ToolContract(
        id="terminal",
        display_name="Terminal Tool",
        fence="autocoder-command",
        operations=("execute",),
        aliases=("terminal tool", "terminal-tool", "terminal", "command line", "cmd", "инструмент терминала", "терминал"),
        is_available=lambda context: isinstance(context.get("project"), dict),
    ),
)


def tool_contract(tool_id: str) -> ToolContract:
    return next(contract for contract in TOOL_CONTRACTS if contract.id == tool_id)


def executable_fences() -> frozenset[str]:
    return frozenset(contract.fence for contract in TOOL_CONTRACTS)


def available_tool_contracts(payload: Any) -> tuple[ToolContract, ...]:
    context = (payload.get("context") or {}) if isinstance(payload, dict) else {}
    return tuple(contract for contract in TOOL_CONTRACTS if contract.is_available(context))


def _requirement_texts(goal: str) -> list[str]:
    """Create stable scopes without interpreting their semantics.

    Newlines/list items are primary boundaries; sentence punctuation is a
    fallback. Empty fragments are discarded and the original wording remains
    available to the model as evidence.
    """
    fragments: list[str] = []
    for line in goal.splitlines():
        line = re.sub(r"^\s*(?:[-*]|\d+[.)])\s*", "", line).strip()
        if not line:
            continue
        fragments.extend(part.strip() for part in re.split(r"(?<=[.!?])\s+", line) if part.strip())
    return fragments or [goal.strip()]


def _scoped_tool_mentions(text: str, contract: ToolContract) -> tuple[bool, bool]:
    aliases = "(?:" + "|".join(re.escape(alias) for alias in sorted(contract.aliases, key=len, reverse=True)) + ")"
    # A bare name is documentation or subject matter, not a constraint. Only
    # explicit invocation/avoidance constructions compile to policy.
    forbidden = bool(re.search(
        rf"(?:\bdo\s+not\s+use|\bdon't\s+use|\bwithout|\bavoid(?:\s+using)?|\bне\s+использ\w*|\bбез)\s+(?:the\s+)?{aliases}(?!\w)",
        text, re.IGNORECASE,
    ))
    required = bool(re.search(
        rf"(?:\buse|\busing|\bvia|\bthrough|\bwith|\bиспольз\w*|\bчерез|\bпосредством|\bс\s+помощью)\s+(?:the\s+)?{aliases}(?!\w)",
        text, re.IGNORECASE,
    ))
    return required and not forbidden, forbidden


def requirement_contracts(payload: Any) -> tuple[RequirementContract, ...]:
    orchestration = payload.get("orchestration") if isinstance(payload, dict) else None
    goal = orchestration.get("goal") if isinstance(orchestration, dict) else None
    if not isinstance(goal, str) or not goal.strip():
        return ()
    requirements = []
    for index, text in enumerate(_requirement_texts(goal), 1):
        required: set[str] = set()
        forbidden: set[str] = set()
        for contract in TOOL_CONTRACTS:
            is_required, is_forbidden = _scoped_tool_mentions(text, contract)
            if is_required:
                required.add(contract.id)
            if is_forbidden:
                forbidden.add(contract.id)
        requirements.append(RequirementContract(
            id=f"requirement-{index}", text=text,
            required_tools=frozenset(required), forbidden_tools=frozenset(forbidden),
        ))
    return tuple(requirements)


def render_tool_contract(payload: Any) -> str:
    available = available_tool_contracts(payload)
    requirements = requirement_contracts(payload)
    lines = ["Executable tool contract (authoritative; closed world):"]
    for tool in available:
        lines.append(f'- id={tool.id}; fence={tool.fence}; operations={",".join(tool.operations)}')
    if not available:
        lines.append("- (no executable tools are available in the current context)")
    if requirements:
        lines.append("Requirement scopes (declare exactly one requirementId in every action):")
        for requirement in requirements:
            policy = []
            if requirement.required_tools:
                policy.append("required=" + ",".join(sorted(requirement.required_tools)))
            if requirement.forbidden_tools:
                policy.append("forbidden=" + ",".join(sorted(requirement.forbidden_tools)))
            lines.append(f'- {requirement.id}; {"; ".join(policy) or "no explicit tool constraint"}; text={requirement.text!r}')
    lines.append("A tool constraint applies only to its requirement scope, never implicitly to the whole task.")
    lines.append("Any tool, fence, operation, payload, or requirementId outside this contract is invalid and cannot reach approval.")
    return "\n".join(lines)


def validate_selected_tool(tool: str, requirement_id: str | None, payload: Any) -> str | None:
    available_ids = {contract.id for contract in available_tool_contracts(payload)}
    if tool not in available_ids:
        return f"Tool '{tool}' is not available in the current factual context."
    requirements = requirement_contracts(payload)
    if not requirements:
        return None
    requirement = next((item for item in requirements if item.id == requirement_id), None)
    if requirement is None:
        return "The model action must declare a requirementId from the current task contract."
    if requirement.required_tools and tool not in requirement.required_tools:
        return f"Tool '{tool}' contradicts {requirement.id}; required: {', '.join(sorted(requirement.required_tools))}."
    if tool in requirement.forbidden_tools:
        return f"Tool '{tool}' is explicitly forbidden for {requirement.id}."
    return None
