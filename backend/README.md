# AutoCoder Python backend

The Tauri application starts `main.py` for each chat request and exchanges one JSON document over standard input/output. The request contains the conversation and, when a text file is open, its relative path and current editor content. The backend adds that file as system context before calling the first provider, local Ollama.

Defaults:
- endpoint: `http://127.0.0.1:11434/api/chat`
- model: `qwen2.5-coder:7b`

Override them with `AUTOCODER_OLLAMA_URL` and `AUTOCODER_OLLAMA_MODEL`. If Python is not available as the platform default command, set `AUTOCODER_PYTHON` to its executable path.

## Local COMSOL 6.4.429 knowledge

For a COMSOL project, create `.autocoder/comsol-knowledge/manifest.json` in the opened project:

```json
{"product":"COMSOL Multiphysics","version":"6.4.429"}
```

Place user-provided official documentation excerpts, verified examples, and working project code
under that directory as UTF-8 `.txt`, `.md`, or `.java` files. On each chat request AutoCoder performs
a bounded local lexical search, sends at most four relevant excerpts to Ollama, and shows the source
paths used below the response. A missing or differently versioned manifest disables retrieval so that
material for another COMSOL release cannot silently ground 6.4.429 answers. The corpus is never
downloaded and no index or request leaves the machine.

## Windows UTF-8 diagnostic

With Ollama running, `python backend/diagnose_chat.py > chat-diagnostic.json`
constructs the Russian reproducer in Python, sends explicit UTF-8 bytes to a
real `main.py` subprocess, captures the actual HTTP request, and then forwards
it to Ollama. The report includes the URL, method, headers, exact Content-Type,
body bytes and hex, BOM status, decoded/parsed JSON, messages, control
characters, and Ollama response.

To inspect a PowerShell text pipeline separately, run in the shell where the
problem reproduced:

```powershell
'{"messages":[{"role":"user","content":"Ответь дословно только содержимым открытого файла"}]}' |
  python backend/diagnose_chat.py --capture-stdin > powershell-stdin.json
```

Compare `powershell-stdin.json`'s `stdin.hex` and `stdin.utf8Text` with
`chat-diagnostic.json`'s `backendStdin`; report any `?`, decode error, BOM, or
changed Cyrillic code point. The reports contain the diagnostic prompt and open
file content, so keep them local unless their contents have been reviewed.
