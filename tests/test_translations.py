"""Keep every user-facing Ukrainian message paired with an English version."""
from collections import Counter
import json
from pathlib import Path
import re
import string
import unittest

ROOT = Path(__file__).resolve().parents[1]
LITERAL = re.compile(r'"(?:\\.|[^"\\])*"')
CYRILLIC = re.compile('[А-Яа-яІіЇїЄєҐґ]')


def translated_messages():
    for path in (ROOT / 'src').glob('*.rs'):
        # Language names are intentionally written in their own language here.
        if path.name == 'i18n.rs':
            continue
        source = path.read_text()
        for token in LITERAL.finditer(source):
            try:
                ukrainian = json.loads(token.group())
            except ValueError:
                continue
            if not CYRILLIC.search(ukrainian):
                continue
            prefix = source[max(0, token.start() - 50):token.start()]
            if not re.search(r'crate::tr!\s*\(\s*$', prefix):
                raise AssertionError(f'Untranslated message in {path.name}: {ukrainian}')
            remaining = source[token.end():]
            comma = re.match(r'\s*,\s*', remaining)
            if comma is None:
                raise AssertionError(f'Missing English text: {ukrainian}')
            english_token = LITERAL.match(remaining, comma.end())
            if english_token is None:
                raise AssertionError(f'Missing English text: {ukrainian}')
            yield ukrainian, json.loads(english_token.group())


def placeholders(text):
    return Counter((name, spec, conversion) for _, name, spec, conversion
                   in string.Formatter().parse(text) if name is not None)


class TranslationTests(unittest.TestCase):
    def test_all_messages_have_an_english_translation(self):
        pairs = list(translated_messages())
        self.assertGreater(len(pairs), 250)
        for ukrainian, english in pairs:
            with self.subTest(message=ukrainian):
                self.assertTrue(english.strip())
                self.assertFalse(CYRILLIC.search(english))

    def test_format_values_and_precision_match_in_both_languages(self):
        for ukrainian, english in translated_messages():
            with self.subTest(message=ukrainian):
                self.assertEqual(placeholders(ukrainian), placeholders(english))


if __name__ == '__main__':
    unittest.main()
