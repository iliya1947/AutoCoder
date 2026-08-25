# AutoCoder Python backend

The Tauri application starts `main.py` for each chat request and exchanges one JSON document over standard input/output. The first provider is local Ollama.

Defaults:
- endpoint: `http://127.0.0.1:11434/api/chat`
- model: `qwen2.5-coder:7b`

Override them with `AUTOCODER_OLLAMA_URL` and `AUTOCODER_OLLAMA_MODEL`. If Python is not available as the platform default command, set `AUTOCODER_PYTHON` to its executable path.
