#!/usr/bin/env python3
"""Extract five unchanged simple glyph outlines from Monaco 0.47's Codicon.

No font runtime dependency: GPUI's macOS font loader rejects icon fonts without
an ASCII m. The SVG coordinates preserve the original glyphs and 16px em box.
The original font and the upstream content/code notices are kept in assets/.
"""
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parents[1]
FONT = (ROOT / 'assets/fonts/monaco-codicon.ttf').read_bytes()

def unpack(fmt, offset):
    return struct.unpack_from('>' + fmt, FONT, offset)

TABLES = {unpack('4s', 12 + i * 16)[0].decode(): unpack('II', 20 + i * 16)
          for i in range(unpack('H', 4)[0])}
EM = unpack('H', TABLES['head'][0] + 18)[0]
ASCENT = unpack('h', TABLES['hhea'][0] + 4)[0]
assert unpack('h', TABLES['head'][0] + 50)[0] == 0

def glyph_id(code):
    base = TABLES['cmap'][0]
    subtable = next(base + offset for platform, encoding, offset in
                    (unpack('HHI', base + 4 + 8 * i) for i in range(unpack('H', base + 2)[0]))
                    if platform == 3 and encoding == 1)
    assert unpack('H', subtable)[0] == 4
    count = unpack('H', subtable + 6)[0] // 2
    for i in range(count):
        end = unpack('H', subtable + 14 + 2 * i)[0]
        start = unpack('H', subtable + 16 + 2 * count + 2 * i)[0]
        if start <= code <= end:
            delta = unpack('h', subtable + 16 + 4 * count + 2 * i)[0]
            address = subtable + 16 + 6 * count + 2 * i
            relative = unpack('H', address)[0]
            value = unpack('H', address + relative + 2 * (code - start))[0] if relative else code
            return (value + delta) % 65536 if value else 0
    raise ValueError(hex(code))

def outlines(code):
    index = glyph_id(code)
    start = TABLES['glyf'][0] + 2 * unpack('H', TABLES['loca'][0] + 2 * index)[0]
    count = unpack('h', start)[0]
    assert count > 0, 'These five original glyphs are simple outlines'
    ends = unpack('H' * count, start + 10)
    offset = start + 10 + 2 * count
    instructions = unpack('H', offset)[0]
    offset += 2 + instructions
    flags = []
    while len(flags) <= ends[-1]:
        flag = FONT[offset]; offset += 1
        repeat = FONT[offset] if flag & 8 else 0
        offset += bool(flag & 8)
        flags.extend([flag] * (repeat + 1))
    axes = []
    for short, same in [(2, 16), (4, 32)]:
        values, value = [], 0
        for flag in flags:
            if flag & short:
                delta = FONT[offset] * (1 if flag & same else -1); offset += 1
            elif flag & same:
                delta = 0
            else:
                delta = unpack('h', offset)[0]; offset += 2
            value += delta; values.append(value)
        axes.append(values)
    points = [(x, ASCENT - y, bool(flag & 1)) for x, y, flag in zip(*axes, flags)]
    contours, begin = [], 0
    for end in ends:
        contours.append(points[begin:end + 1]); begin = end + 1
    return contours

def number(value):
    return f'{value:g}'

def xy(point):
    return f'{number(point[0])} {number(point[1])}'

def midpoint(a, b):
    return ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, True)

def path(contour):
    if contour[0][2]:
        start, points = contour[0], contour[1:]
    elif contour[-1][2]:
        start, points = contour[-1], contour[:-1]
    else:
        start, points = midpoint(contour[-1], contour[0]), contour
    points = points + [start]
    result, i = [f'M {xy(start)}'], 0
    while i < len(points):
        point = points[i]
        if point[2]:
            result.append(f'L {xy(point)}'); i += 1
        else:
            following = points[i + 1]
            end = following if following[2] else midpoint(point, following)
            result.append(f'Q {xy(point)} {xy(end)}'); i += 2 if following[2] else 1
    return ' '.join(result) + ' Z'

for name, code in [('error', 0xea87), ('warning', 0xea6c), ('next', 0xea9a),
                   ('previous', 0xeaa1), ('close', 0xea76)]:
    data = ' '.join(path(contour) for contour in outlines(code))
    svg = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {EM} {EM}" width="16" height="16"><path d="{data}"/></svg>\n'
    destination = ROOT / f'assets/icons/diagnostic-{name}.svg'
    if '--check' in sys.argv:
        assert destination.read_text() == svg, destination
    else:
        destination.write_text(svg)
    print(name, f'U+{code:04X}', 'glyph', glyph_id(code))
