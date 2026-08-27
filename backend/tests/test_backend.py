import io
import json
import subprocess
import sys
import unittest
from urllib import error
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from diagnose_chat import PAYLOAD, byte_report
import main as backend_main
from main import parse_messages, parse_request
from provider import Message, OllamaProvider, ProviderError


class FakeResponse:
    def __enter__(self):
        return self

    def __exit__(self, *_):
        return None

    def read(self):
        return json.dumps({"message": {"role": "assistant", "content": "Ready"}}).encode()


class BackendTests(unittest.TestCase):
    @patch("provider.request.urlopen", return_value=FakeResponse())
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
        self.assertEqual(sent_messages[1]["content"], "Ответь дословно только содержимым открытого файла")
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
            {"message": {"role": "assistant", "content": "Готово: файл сохранён"}},
        )

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
                "context": {"openFile": {"path": "src/main.py", "content": "print('hi')"}},
            }
        )

        self.assertEqual(messages[0].role, "system")
        self.assertIn("Path: src/main.py", messages[0].content)
        self.assertIn("print('hi')", messages[0].content)
        self.assertEqual(messages[1], Message("user", "Explain this"))

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
        self.assertEqual(messages[1], Message("user", "Where are the tests?"))

    def test_accepts_project_with_no_entries_and_open_file_together(self):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Explain this"}],
                "context": {
                    "project": {"name": "Empty project", "entries": []},
                    "openFile": {"path": "README", "content": "draft"},
                },
            }
        )

        self.assertEqual([message.role for message in messages], ["system", "system", "user"])
        self.assertIn("Project: Empty project", messages[0].content)
        self.assertIn("Path: README", messages[1].content)

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
                    "openFile": {"path": "two.txt", "content": "Тестовый файл номер 2"},
                    "selection": {"state": "none"},
                },
            }
        )

        self.assertEqual([message.role for message in messages], ["system", "system", "user"])
        self.assertIn("<open_file>\nТестовый файл номер 2\n</open_file>", messages[0].content)
        self.assertIn("no active text selection", messages[1].content)
        self.assertIn("not to the open file content", messages[1].content)
        self.assertNotIn("Тестовый файл номер 2", messages[1].content)

    @patch("provider.request.urlopen", return_value=FakeResponse())
    def test_open_file_request_matches_working_ollama_message_shape(self, urlopen):
        messages = parse_request(
            {
                "messages": [{"role": "user", "content": "Ответь содержимым открытого файла"}],
                "context": {
                    "openFile": {
                        "path": "АвтоКодер_тестовый файл.txt",
                        "content": "123 123 123",
                    }
                },
            }
        )

        OllamaProvider(model="qwen2.5-coder:7b").chat(messages)

        sent = json.loads(urlopen.call_args.args[0].data)
        self.assertEqual(
            sent["messages"],
            [
                {
                    "role": "system",
                    "content": (
                        "The user currently has this project file open in AutoCoder.\n"
                        "Use its path and content as context for the user's request.\n"
                        "Path: АвтоКодер_тестовый файл.txt\n\n"
                        "<open_file>\n123 123 123\n</open_file>"
                    ),
                },
                {"role": "user", "content": "Ответь содержимым открытого файла"},
            ],
        )
        self.assertFalse(sent["stream"])

    def test_rejects_invalid_open_file_context(self):
        with self.assertRaises(ValueError):
            parse_request({"messages": [{"role": "user", "content": "Hi"}], "context": {"openFile": {}}})

    @patch("provider.request.urlopen", return_value=FakeResponse())
    def test_ollama_provider_returns_assistant_message(self, urlopen):
        result = OllamaProvider(model="test-model").chat([Message("user", "Hi")])
        self.assertEqual(result, Message("assistant", "Ready"))
        sent = json.loads(urlopen.call_args.args[0].data)
        self.assertEqual(sent["model"], "test-model")
        self.assertFalse(sent["stream"])

    @patch("provider.request.urlopen")
    def test_ollama_http_error_preserves_response_details(self, urlopen):
        urlopen.side_effect = error.HTTPError(
            "http://127.0.0.1:11434/api/chat",
            400,
            "Bad Request",
            {},
            io.BytesIO(b'{"error":"prompt is too long"}'),
        )

        with self.assertRaisesRegex(ProviderError, 'HTTP 400.*prompt is too long'):
            OllamaProvider(model="test-model").chat([Message("user", "Hi")])

    @patch("provider.request.urlopen", return_value=FakeResponse())
    def test_ollama_provider_sends_cyrillic_as_utf8(self, urlopen):
        OllamaProvider(model="test-model").chat([Message("user", "Привет, мир!")])

        body = urlopen.call_args.args[0].data
        self.assertIn("Привет, мир!".encode("utf-8"), body)
        self.assertNotIn(b"\\u041f", body)
        self.assertEqual(json.loads(body)["messages"][0]["content"], "Привет, мир!")


if __name__ == "__main__":
    unittest.main()
