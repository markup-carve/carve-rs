#!/usr/bin/env python3
"""Regenerate src/table_unicode_18_data.rs from Unicode 18.0.0 UCD files."""

import hashlib
import sys
from pathlib import Path

SOURCES = {
    'GraphemeBreakProperty': ('auxiliary/GraphemeBreakProperty.txt', '0839dcb79e4ac639ecd538b1abf7c9d22e3f9dd265b7e182d33627aa4d75b45a'),
    'EastAsianWidth': ('EastAsianWidth.txt', 'a0cf29eacd00cfcaec4381c6b7c281685f18dbb4e7ff82b4076ccb342ca839aa'),
    'DerivedCoreProperties': ('DerivedCoreProperties.txt', '09c928886a178fcafd93c29e4bd59073a058e5a100b716d425cb563ab50f68c9'),
    'emoji-data': ('emoji/emoji-data.txt', '80d00f8e616a0ef27fd6b8de3b758c06383b5d917e2977709578e68baf733bf1'),
}
GCB_NAMES = ['CR', 'LF', 'Control', 'Extend', 'ZWJ', 'Regional_Indicator',
             'Prepend', 'SpacingMark', 'L', 'V', 'T', 'LV', 'LVT']


def read_rows(directory, name):
    path, digest = SOURCES[name]
    data = (directory / path).read_bytes()
    if hashlib.sha256(data).hexdigest() != digest:
        raise ValueError(f'{path}: expected Unicode 18.0.0 SHA-256 {digest}')
    for line in data.decode().splitlines():
        body = line.split('#')[0]
        if ';' not in body:
            continue
        fields = [field.strip() for field in body.split(';')]
        start, _, end = fields[0].partition('..')
        yield int(start, 16), int(end or start, 16), fields[1:]


def flat(rows):
    result = []
    for row in sorted(rows):
        result.extend(row)
    return result


def main(directory):
    categories = {name: index + 1 for index, name in enumerate(GCB_NAMES)}
    ranges = {
        'GCB': flat((a, b, categories[p[0]]) for a, b, p in read_rows(directory, 'GraphemeBreakProperty') if p[0] in categories),
        'WIDE': flat((a, b) for a, b, p in read_rows(directory, 'EastAsianWidth') if p[0] in ('W', 'F')),
        'PICT': flat((a, b) for a, b, p in read_rows(directory, 'emoji-data') if p[0] == 'Extended_Pictographic'),
        'INCB_CONSONANT': flat((a, b) for a, b, p in read_rows(directory, 'DerivedCoreProperties') if p[:2] == ['InCB', 'Consonant']),
        'INCB_LINKER': flat((a, b) for a, b, p in read_rows(directory, 'DerivedCoreProperties') if p[:2] == ['InCB', 'Linker']),
        'INCB_EXTEND': flat((a, b) for a, b, p in read_rows(directory, 'DerivedCoreProperties') if p[:2] == ['InCB', 'Extend']),
    }
    content = ('// Unicode 18.0.0 property ranges for table display width.\n'
               '// Sources: unicode.org/Public/18.0.0/ucd/{auxiliary/GraphemeBreakProperty,EastAsianWidth,DerivedCoreProperties,emoji/emoji-data}.txt\n'
               '// Unicode data: Copyright © 2026 Unicode, Inc. https://www.unicode.org/terms_of_use.html\n')
    for name, values in ranges.items():
        content += f'pub(crate) const {name}: &[u32] = &[\n'
        for index in range(0, len(values), 30):
            content += '  ' + ', '.join(map(str, values[index:index + 30])) + ',\n'
        content += '];\n'
    target = Path(__file__).resolve().parents[1] / 'src/table_unicode_18_data.rs'
    target.write_text(content)


if __name__ == '__main__':
    if len(sys.argv) != 2:
        raise SystemExit('usage: generate-table-unicode-18.py <Unicode-18.0.0-ucd-directory>')
    main(Path(sys.argv[1]))
