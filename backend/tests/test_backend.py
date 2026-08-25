import json
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from main import parse_messages
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

    @patch("provider.request.urlopen", return_value=FakeResponse())
    def test_ollama_provider_returns_assistant_message(self, urlopen):
        result = OllamaProvider(model="test-model").chat([Message("user", "Hi")])
        self.assertEqual(result, Message("assistant", "Ready"))
        sent = json.loads(urlopen.call_args.args[0].data)
        self.assertEqual(sent["model"], "test-model")
        self.assertFalse(sent["stream"])


if __name__ == "__main__":
    unittest.main()
