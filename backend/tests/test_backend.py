import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

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

    @patch("provider.request.urlopen", return_value=FakeResponse())
    def test_ollama_provider_sends_cyrillic_as_utf8(self, urlopen):
        OllamaProvider(model="test-model").chat([Message("user", "Привет, мир!")])

        body = urlopen.call_args.args[0].data
        self.assertIn("Привет, мир!".encode("utf-8"), body)
        self.assertNotIn(b"\\u041f", body)
        self.assertEqual(json.loads(body)["messages"][0]["content"], "Привет, мир!")


if __name__ == "__main__":
    unittest.main()
