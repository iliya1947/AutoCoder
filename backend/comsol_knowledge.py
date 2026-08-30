"""Bounded, dependency-free retrieval from project-local COMSOL 6.4 knowledge."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass
from pathlib import Path

KNOWLEDGE_RELATIVE_PATH = Path(".autocoder") / "comsol-knowledge"
SUPPORTED_SUFFIXES = {".java", ".md", ".txt"}
TARGET_PRODUCT = "COMSOL Multiphysics"
TARGET_VERSION = "6.4.429"
MAX_FILES = 500
MAX_FILE_BYTES = 2_000_000
MAX_RESULTS = 4
MAX_CHUNK_CHARS = 1_600
WORD_PATTERN = re.compile(r"[\w.-]+", re.UNICODE)


@dataclass(frozen=True)
class KnowledgeMatch:
    source: str
    content: str
    score: int


def _terms(text: str) -> set[str]:
    return {term.casefold() for term in WORD_PATTERN.findall(text) if len(term) >= 2}


def _chunks(text: str) -> list[str]:
    paragraphs = [part.strip() for part in re.split(r"\n\s*\n", text) if part.strip()]
    chunks: list[str] = []
    current = ""
    for paragraph in paragraphs:
        if len(paragraph) > MAX_CHUNK_CHARS:
            if current:
                chunks.append(current)
                current = ""
            chunks.extend(paragraph[offset:offset + MAX_CHUNK_CHARS] for offset in range(0, len(paragraph), MAX_CHUNK_CHARS))
        elif not current:
            current = paragraph
        elif len(current) + len(paragraph) + 2 <= MAX_CHUNK_CHARS:
            current += "\n\n" + paragraph
        else:
            chunks.append(current)
            current = paragraph
    if current:
        chunks.append(current)
    return chunks


def _validated_root(project_root: Path) -> Path | None:
    root = project_root / KNOWLEDGE_RELATIVE_PATH
    manifest = root / "manifest.json"
    try:
        metadata = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        return None
    if metadata != {"product": TARGET_PRODUCT, "version": TARGET_VERSION}:
        return None
    return root


def search_comsol_knowledge(project_root: Path, query: str) -> list[KnowledgeMatch]:
    """Return relevant snippets only from an explicitly versioned local corpus."""
    root = _validated_root(project_root)
    query_terms = _terms(query)
    if root is None or not query_terms:
        return []

    matches: list[KnowledgeMatch] = []
    files_seen = 0
    for path in sorted(root.rglob("*")):
        if files_seen >= MAX_FILES:
            break
        if path.is_symlink() or not path.is_file() or path.name == "manifest.json" or path.suffix.casefold() not in SUPPORTED_SUFFIXES:
            continue
        files_seen += 1
        try:
            if path.stat().st_size > MAX_FILE_BYTES:
                continue
            text = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            continue
        source = path.relative_to(root).as_posix()
        source_terms = _terms(source)
        for chunk in _chunks(text):
            chunk_terms = _terms(chunk)
            overlap = query_terms & chunk_terms
            if not overlap:
                continue
            score = sum(3 if term in source_terms else 1 for term in overlap)
            matches.append(KnowledgeMatch(source=source, content=chunk, score=score))

    matches.sort(key=lambda match: (-match.score, match.source, match.content))
    return matches[:MAX_RESULTS]
