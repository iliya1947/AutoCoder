import io
import json
import subprocess
import sys
import unittest
from urllib import error
from pathlib import Path
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from diagnose_chat import PAYLOAD, byte_report
import main as backend_main
from main import TOOL_RESULT_PROMPT, parse_file_proposal, parse_messages, parse_request, parse_terminal_proposal
from provider import Message, OllamaProvider, OllamaRuntime, ProviderError


class FakeResponse:
    def __init__(self, payload=None):
        self.payload = payload or {"message": {"role": "assistant", "content": "Ready"}}

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return None

    def read(self):
        return json.dumps(self.payload).encode()


def ready_ollama(request_or_url, **_kwargs):
    url = request_or_url.full_url if hasattr(request_or_url, "full_url") else request_or_url
    if url.endswith("/api/version"):
        return FakeResponse({"version": "0.11.8"})
    if url.endswith("/api/tags"):
        return FakeResponse({"models": [{"name": "qwen2.5-coder:7b"}, {"name": "test-model"}]})
    return FakeResponse()


class BackendTests(unittest.TestCase):
    @patch("provider.request.urlopen", side_effect=ready_ollama)
    def test_utf8_stdin_preserves_cyrillic_through_request_and_ollama_payload(self, urlopen):
        stdin_bytes = json.dumps(PAYLOAD, ensure_ascii=False).encode("utf-8")
        binary_stdin = type("BinaryStdin", (), {"buffer": io.BytesIO(stdin_bytes)})()

        binary_stdout = type("BinaryStdout", (), {"buffer": io.BytesIO()})()

        with patch.object(backend_main.sys, "stdin", binary_stdin), patch.object(
            backend_main.sys, "stdout", binary_stdout
        ):
            self.assertEqual(backend_main.main(), 0)

        sent_body = urlopen.call_args.args[0].data
        sent_messages = json.loads(sent_body.decode("utf-8"))["messages"]
        self.assertIn("АвтоКодер_тестовый файл.txt", sent_messages[0]["content"])
        self.assertEqual(sent_messages[-1]["content"], "Ответь дословно только содержимым открытого файла")
        self.assertIn("АвтоКодер_тестовый файл.txt".encode("utf-8"), sent_body)
        self.assertIn("Ответь дословно только содержимым открытого файла".encode("utf-8"), sent_body)
        self.assertNotIn("РћС".encode("utf-8"), sent_body)
        self.assertFalse(byte_report(stdin_bytes)["hasUtf8Bom"])

    @patch("main.OllamaProvider.chat", return_value=Message("assistant", "Готово: файл сохранён"))
    def test_production_stdout_is_bomless_utf8_json_with_cyrillic(self, _chat):
        stdin_bytes = b'{"messages":[{"role":"user","content":"Save it"}]}'
        binary_stdin = type("BinaryStdin", (), {"buffer": io.BytesIO(stdin_bytes)})()
        stdout_bytes = io.BytesIO()
        binary_stdout = type("BinaryStdout", (), {"buffer": stdout_bytes})()

        with patch.object(backend_main.sys, "stdin", binary_stdin), patch.object(
            backend_main.sys, "stdout", binary_stdout
        ):
            self.assertEqual(backend_main.main(), 0)

        output = stdout_bytes.getvalue()
        self.assertFalse(output.startswith(b"\xef\xbb\xbf"))
        self.assertIn("Готово: файл сохранён".encode("utf-8"), output)
        decoded = output.decode("utf-8")
        self.assertNotIn("Р“РѕС‚РѕРІРѕ", decoded)
        self.assertEqual(
            json.loads(decoded),
            {
                "message": {"role": "assistant", "content": "Готово: файл сохранён"},
                "proposal": None,
                "commandProposal": None,
            },
        )

    def test_extracts_terminal_proposal_only_for_an_open_project(self):
        answer = Message(
            "assistant",
            'Проверьте команду.\n```autocoder-command\n{"command":"npm test"}\n```',
        )
        payload = {"context": {"project": {"name": "demo", "entries": []}}}

        self.assertEqual(parse_terminal_proposal(answer, payload), {"command": "npm test"})
        self.assertIsNone(parse_terminal_proposal(answer, {"context": None}))

    def test_rejects_invalid_or_combined_terminal_proposal(self):
        payload = {"context": {"project": {"name": "demo", "entries": []}}}
        invalid = Message("assistant", '```autocoder-command\n{"command":"   "}\n```')
        combined = Message(
            "assistant",
            '```autocoder-command\n{"command":"npm test"}\n```\n'
            '```autocoder-file\n{"operation":"create","path":"x","content":"x"}\n```',
        )

        self.assertIsNone(parse_terminal_proposal(invalid, payload))
        self.assertIsNone(parse_terminal_proposal(combined, payload))

    def test_extracts_file_proposal_for_current_open_file(self):
        answer = Message(
            "assistant",
            'Предлагаю изменение.\n```autocoder-file\n{"operation":"replace","path":"src/main.py","content":"print(42)\\n"}\n```',
        )
        payload = {
            "context": {"openFile": {"path": "src/main.py", "content": "print(1)\n", "savedContent": "print(1)\n"}}
        }

        self.assertEqual(parse_file_proposal(answer, payload), {
            "operation": "replace",
            "path": "src/main.py",
            "content": "print(42)\n",
            "originalContent": "print(1)\n",
        })

    def test_rejects_file_proposal_for_another_path(self):
        answer = Message(
            "assistant",
            '```autocoder-file\n{"operation":"create","path":"../outside.py","content":"bad"}\n```',
        )
        payload = {"context": {"openFile": {"path": "src/main.py", "content": "safe", "savedContent": "safe"}}}

        self.assertIsNone(parse_file_proposal(answer, payload))

    def test_extracts_delete_proposal_only_for_current_open_file(self):
        payload = {"context": {"openFile": {"path": "src/main.py", "content": "print(1)\n", "savedContent": "print(1)\n"}}}
        answer = Message(
            "assistant",
            '```autocoder-file\n{"operation":"delete","path":"src/main.py"}\n```',
        )

        self.assertEqual(parse_file_proposal(answer, payload), {
            "operation": "delete",
            "path": "src/main.py",
            "originalContent": "print(1)\n",
            "expectedSavedContent": "print(1)\n",
        })

        other = Message(
            "assistant",
            '```autocoder-file\n{"operation":"delete","path":"src/other.py"}\n```',
        )
        self.assertIsNone(parse_file_proposal(other, payload))

        dirty_payload = {"context": {"openFile": {
            "path": "src/main.py", "content": "unsaved", "savedContent": "saved"
        }}}
        self.assertIsNone(parse_file_proposal(answer, dirty_payload))

    def test_extracts_new_file_proposal_for_an_absent_project_path(self):
        answer = Message(
            "assistant",
            '```autocoder-file\n{"operation":"create","path":"src/new.py","content":"print(42)\\n"}\n```',
        )
        payload = {"context": {"project": {
            "name": "demo", "entries": ["directory: src", "file: src/main.py"]
        }}}

        self.assertEqual(parse_file_proposal(answer, payload), {
            "operation": "create", "path": "src/new.py", "content": "print(42)\n"
        })

    def test_rejects_new_file_proposal_for_an_existing_path(self):
        answer = Message(
            "assistant",
            '```autocoder-file\n{"operation":"create","path":"src/main.py","content":"replace"}\n```',
        )
        payload = {"context": {"project": {
            "name": "demo", "entries": ["directory: src", "file: src/main.py"]
        }}}

        self.assertIsNone(parse_file_proposal(answer, payload))

    def test_rejects_windows_unsafe_new_file_paths(self):
        payload = {"context": {"project": {"name": "demo", "entries": []}}}
        for path in ("existing.txt:stream", "CON.txt", "NUL.txt", "COM1.log", "bad?.txt", "bad*.txt", "bad. "):
            answer = Message(
                "assistant",
                f'```autocoder-file\n{{"operation":"create","path":{json.dumps(path)},"content":"bad"}}\n```',
            )
            with self.subTest(path=path):
                self.assertIsNone(parse_file_proposal(answer, payload))

    def test_invalid_utf8_stdin_uses_existing_error_path(self):
        binary_stdin = type("BinaryStdin", (), {"buffer": io.BytesIO(b"\xff")})()
        stderr = io.StringIO()

        with patch.object(backend_main.sys, "stdin", binary_stdin), patch.object(
            backend_main.sys, "stderr", stderr
        ):
            self.assertEqual(backend_main.main(), 1)

        self.assertIn("utf-8", stderr.getvalue())

    def test_stdin_capture_reports_exact_input_bytes(self):
        input_bytes = b'{"messages":[{"role":"user","content":"??????"}]}'
        completed = subprocess.run(
            [sys.executable, str(Path(__file__).resolve().parents[1] / "diagnose_chat.py"), "--capture-stdin"],
            input=input_bytes,
            stdout=subprocess.PIPE,
            check=True,
        )

        report = json.loads(completed.stdout)
        self.assertEqual(report["stdin"]["hex"], input_bytes.hex(" "))
        self.assertEqual(report["stdin"]["utf8Text"], input_bytes.decode())

    def test_parses_valid_contract(self):
        self.assertEqual(parse_messages({"messages": [{"role": "user", "content": "Hi"}]}), [Message("user", "Hi")])

    def test_rejects_empty_content(self):
        with self.assertRaises(ValueError):
            parse_messages({"messages": [{"role": "user", "content": " "}]})

    def test_adds_open_file_as_system_context(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Explain this"}],
                "context": {"openFile": {"path": "src/main.py", "content": "print('hi')", "savedContent": "print('hi')"}},
            }
        )

        self.assertEqual(messages[0].role, "system")
        self.assertIn("Path: src/main.py", messages[0].content)
        self.assertIn("print('hi')", messages[0].content)
        self.assertIn("autocoder-file", messages[1].content)
        self.assertEqual(messages[2], Message("user", "Explain this"))

    def test_adds_read_only_project_structure_as_system_context(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Where are the tests?"}],
                "context": {
                    "project": {
                        "name": "AutoCoder",
                        "entries": ["directory: backend", "file: backend/main.py"],
                    }
                },
            }
        )

        self.assertEqual(messages[0].role, "system")
        self.assertIn("Project: AutoCoder", messages[0].content)
        self.assertIn("file: backend/main.py", messages[0].content)
        self.assertIn("Do not assume their contents", messages[0].content)
        self.assertIn("autocoder-file", messages[1].content)
        self.assertIn("autocoder-command", messages[2].content)
        self.assertEqual(messages[3], Message("user", "Where are the tests?"))

    def test_marks_executed_tool_feedback_as_factual_orchestration_context(self):
        tool_result = "AutoCoder Terminal Tool result (this is factual output):\nCommand: npm test\nStatus: exit code: 0"
        messages = parse_request(
            {
                "messages": [
                    {"role": "user", "content": "Run the tests"},
                    {"role": "assistant", "content": "Please review npm test"},
                    {"role": "user", "content": tool_result},
                ],
                "context": {"project": {"name": "demo", "entries": []}},
            }
        )

        self.assertEqual(messages[-4], Message("system", TOOL_RESULT_PROMPT))
        self.assertEqual(messages[-1], Message("user", tool_result))

    def test_accepts_project_with_no_entries_and_open_file_together(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Explain this"}],
                "context": {
                    "project": {"name": "Empty project", "entries": []},
                    "openFile": {"path": "README", "content": "draft", "savedContent": "draft"},
                },
            }
        )

        self.assertEqual([message.role for message in messages], ["system", "system", "system", "system", "user"])
        self.assertIn("Project: Empty project", messages[0].content)
        self.assertIn("Path: README", messages[1].content)
        self.assertIn("autocoder-command", messages[3].content)

    def test_adds_editor_selection_as_prioritized_system_context(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Explain the selection"}],
                "context": {
                    "selection": {
                        "state": "active",
                        "path": "src/main.py",
                        "content": "result = calculate()",
                    }
                },
            }
        )

        self.assertEqual(messages[0].role, "system")
        self.assertIn("Path: src/main.py", messages[0].content)
        self.assertIn("<selection>\nresult = calculate()\n</selection>", messages[0].content)
        self.assertIn("Give it priority", messages[0].content)
        self.assertEqual(messages[1], Message("user", "Explain the selection"))

    def test_rejects_empty_editor_selection(self):
        with self.assertRaisesRegex(ValueError, "Selection context"):
            parse_request(
                {
                    "messages": [{"role": "user", "content": "Explain"}],
                    "context": {"selection": {"path": "src/main.py", "content": ""}},
                }
            )

    def test_explicit_no_selection_is_distinct_from_open_file_content(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "What is selected?"}],
                "context": {
                    "openFile": {"path": "two.txt", "content": "Тестовый файл номер 2", "savedContent": "Тестовый файл номер 2"},
                    "selection": {"state": "none"},
                },
            }
        )

        self.assertEqual([message.role for message in messages], ["system", "system", "system", "user"])
        self.assertIn("<open_file>\nТестовый файл номер 2\n</open_file>", messages[0].content)
        self.assertIn("no active text selection", messages[2].content)
        self.assertIn("not to the open file content", messages[2].content)
        self.assertNotIn("Тестовый файл номер 2", messages[2].content)

    @patch("provider.request.urlopen", side_effect=ready_ollama)
    def test_open_file_request_matches_working_ollama_message_shape(self, urlopen):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Ответь содержимым открытого файла"}],
                "context": {
                    "openFile": {
                        "path": "АвтоКодер_тестовый файл.txt",
                        "content": "123 123 123",
                        "savedContent": "123 123 123",
                    }
                },
            }
        )

        OllamaProvider(model="qwen2.5-coder:7b").chat(messages)

        sent = json.loads(urlopen.call_args.args[0].data)
        self.assertEqual(sent["messages"][0]["role"], "system")
        self.assertIn("Path: АвтоКодер_тестовый файл.txt", sent["messages"][0]["content"])
        self.assertIn("<open_file>\n123 123 123\n</open_file>", sent["messages"][0]["content"])
        self.assertIn("autocoder-file", sent["messages"][1]["content"])
        self.assertEqual(sent["messages"][2], {"role": "user", "content": "Ответь содержимым открытого файла"})
        self.assertFalse(sent["stream"])

    def test_rejects_invalid_open_file_context(self):
        with self.assertRaises(ValueError):
            parse_request({"messages": [{"role": "user", "content": "Hi"}], "context": {"openFile": {}}})

    @patch("provider.request.urlopen", side_effect=ready_ollama)
    def test_ollama_provider_returns_assistant_message(self, urlopen):
        result = OllamaProvider(model="test-model").chat([Message("user", "Hi")])
        self.assertEqual(result, Message("assistant", "Ready"))
        sent = json.loads(urlopen.call_args.args[0].data)
        self.assertEqual(sent["model"], "test-model")
        self.assertFalse(sent["stream"])

    @patch("provider.request.urlopen")
    def test_ollama_http_error_preserves_response_details(self, urlopen):
        def response(request_or_url, **_kwargs):
            url = request_or_url.full_url if hasattr(request_or_url, "full_url") else request_or_url
            if url.endswith("/api/version"):
                return FakeResponse({"version": "0.11.8"})
            if url.endswith("/api/tags"):
                return FakeResponse({"models": [{"name": "test-model"}]})
            raise error.HTTPError(url, 400, "Bad Request", {}, io.BytesIO(b'{"error":"prompt is too long"}'))

        urlopen.side_effect = response

        with self.assertRaisesRegex(ProviderError, 'HTTP 400.*prompt is too long'):
            OllamaProvider(model="test-model").chat([Message("user", "Hi")])

    @patch("provider.request.urlopen", side_effect=ready_ollama)
    def test_ollama_provider_sends_cyrillic_as_utf8(self, urlopen):
        OllamaProvider(model="test-model").chat([Message("user", "Привет, мир!")])

        body = urlopen.call_args.args[0].data
        self.assertIn("Привет, мир!".encode("utf-8"), body)
        self.assertNotIn(b"\\u041f", body)
        self.assertEqual(json.loads(body)["messages"][0]["content"], "Привет, мир!")

    def test_running_ollama_is_used_without_launching_or_relaunching(self):
        launcher = Mock()
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434",
            opener=lambda *_args, **_kwargs: FakeResponse({"version": "1.0"}),
            process_launcher=launcher,
        )

        runtime.ensure_ready()
        runtime.ensure_ready()

        launcher.assert_not_called()

    def test_stopped_ollama_is_started_then_becomes_ready(self):
        process = Mock()
        process.poll.return_value = None
        launcher = Mock(return_value=process)
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434",
            opener=Mock(),
            executable_finder=lambda: Path("C:/Users/test/AppData/Local/Programs/Ollama/ollama.exe"),
            process_launcher=launcher,
            sleep=lambda _seconds: None,
        )
        runtime.is_ready = Mock(side_effect=[False, False, True])

        runtime.ensure_ready()

        launcher.assert_called_once()
        self.assertEqual(runtime.is_ready.call_count, 3)

    def test_missing_local_ollama_has_specific_error(self):
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434", opener=Mock(), executable_finder=lambda: None
        )
        runtime.is_ready = Mock(return_value=False)

        with self.assertRaisesRegex(ProviderError, "Local Ollama was not found"):
            runtime.ensure_ready()

    def test_http_503_readiness_is_not_reported_as_missing_executable(self):
        unavailable = error.HTTPError(
            "http://127.0.0.1:11434/api/version", 503, "Service Unavailable", {}, None
        )
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434",
            opener=Mock(side_effect=unavailable),
            executable_finder=Mock(),
        )

        with self.assertRaisesRegex(ProviderError, r"readiness endpoint.*HTTP 503"):
            runtime.ensure_ready()

        runtime.executable_finder.assert_not_called()

    def test_ollama_launch_error_reports_executable_and_system_reason(self):
        executable = Path("C:/Ollama/ollama.exe")
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434",
            opener=Mock(),
            executable_finder=lambda: executable,
            process_launcher=Mock(side_effect=OSError(5, "Access is denied")),
        )
        runtime.is_ready = Mock(return_value=False)

        with self.assertRaisesRegex(ProviderError, r"C:/Ollama/ollama.exe.*Access is denied"):
            runtime.ensure_ready()

    def test_ollama_start_timeout_reports_path_and_endpoint(self):
        process = Mock()
        process.poll.return_value = None
        clock = iter([0.0, 0.0, 1.0])
        executable = Path("C:/Ollama/ollama.exe")
        runtime = OllamaRuntime(
            "http://127.0.0.1:11434",
            timeout=0.5,
            opener=Mock(),
            executable_finder=lambda: executable,
            process_launcher=lambda _path: process,
            monotonic=lambda: next(clock),
            sleep=lambda _seconds: None,
        )
        runtime.is_ready = Mock(return_value=False)

        with self.assertRaisesRegex(ProviderError, r"Timed out.*C:/Ollama/ollama.exe.*127.0.0.1"):
            runtime.ensure_ready()

    def test_missing_model_has_specific_error_and_does_not_chat(self):
        def opener(request_or_url, **_kwargs):
            url = request_or_url.full_url if hasattr(request_or_url, "full_url") else request_or_url
            if url.endswith("/api/version"):
                return FakeResponse({"version": "1.0"})
            if url.endswith("/api/tags"):
                return FakeResponse({"models": [{"name": "another-model:latest"}]})
            self.fail("Chat must not run when the required model is absent")

        with self.assertRaisesRegex(ProviderError, "qwen2.5-coder:7b.*is not installed"):
            OllamaProvider(opener=opener).chat([Message("user", "Hi")])

    def test_remote_endpoint_is_not_managed(self):
        runtime = Mock()
        provider = OllamaProvider(
            url="https://explicit-provider.example/api/chat",
            model="remote-model",
            runtime=runtime,
            opener=lambda *_args, **_kwargs: FakeResponse(),
        )

        provider.chat([Message("user", "Hi")])

        runtime.ensure_ready.assert_not_called()
        runtime.ensure_model.assert_not_called()


if __name__ == "__main__":
    unittest.main()
