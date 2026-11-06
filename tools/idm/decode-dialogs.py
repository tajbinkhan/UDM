"""Decode IDM's dialog templates from REA's resource listing.

REA's `inspect_pe_resources` on IDMan.exe returns too much to read whole, so
save it to a file and run:

    python -I tools/idm/decode-dialogs.py <rea-resources.json> <IDMan.exe> [filter]

`filter` is text separated by `|`; a dialog is printed when its caption or a
control's text contains one of them (any case). Without it, every dialog is
printed. Each dialog shows its ID, template kind, style and extended style
(hex), caption, size in dialog units, then `(control id, class, text)` rows.
Both DLGTEMPLATE and DLGTEMPLATEEX are read: 71 of IDM 6.43's 105 dialogs
are the classic kind.
"""

import json
import struct
import sys


def read_text(b, o):
    """A string-or-ordinal field: `#n` for an ordinal."""
    if struct.unpack_from("<H", b, o)[0] == 0xFFFF:
        return "#%d" % struct.unpack_from("<H", b, o + 2)[0], o + 4
    end = o
    while struct.unpack_from("<H", b, end)[0]:
        end += 2
    return b[o:end].decode("utf-16le"), end + 2


def align(o):
    return (o + 3) & ~3


def parse(b):
    ex = struct.unpack_from("<HH", b, 0) == (1, 0xFFFF)
    if ex:
        _, _, _help, exstyle, style, count, _x, _y, cx, cy = struct.unpack_from("<HHIIIHhhhh", b, 0)
        o = 26
    else:
        style, exstyle, count, _x, _y, cx, cy = struct.unpack_from("<IIHhhhh", b, 0)
        o = 18
    _menu, o = read_text(b, o)
    _cls, o = read_text(b, o)
    caption, o = read_text(b, o)
    if style & 0x40:  # DS_SETFONT: point size (and weight, italic, charset), face
        o += 6 if ex else 2
        _face, o = read_text(b, o)
    controls = []
    for _ in range(count):
        o = align(o)
        if ex:
            _, _cex, _cst, _x, _y, _w, _h, cid = struct.unpack_from("<IIIhhhhI", b, o)
            o += 24
        else:
            _cst, _cex, _x, _y, _w, _h, cid = struct.unpack_from("<IIhhhhH", b, o)
            o += 18
        cls, o = read_text(b, o)
        text, o = read_text(b, o)
        extra = struct.unpack_from("<H", b, o)[0]
        o += 2 + extra
        controls.append((cid, cls, text))
    return ex, style, exstyle, caption, (cx, cy), controls


def main():
    resources = json.load(open(sys.argv[1], encoding="utf-8"))["normalized_result"]["resources"]
    exe = open(sys.argv[2], "rb").read()
    keys = [k.lower() for k in sys.argv[3].split("|")] if len(sys.argv) > 3 else []
    for r in resources:
        if r["type"].get("id") != 5:  # RT_DIALOG
            continue
        where = r["payload"]["location"]
        name = r["name"].get("id", r["name"].get("name"))
        try:
            ex, style, exstyle, caption, size, controls = parse(exe[where["offset"] : where["offset"] + where["bytes"]])
        except (struct.error, UnicodeDecodeError) as e:
            print("== dialog", name, "could not be read:", e)
            continue
        text = (caption + " " + " ".join(str(t) for _, _, t in controls)).lower()
        if keys and not any(k in text for k in keys):
            continue
        kind = "EX" if ex else "classic"
        print("== dialog %s %s style=%08x exstyle=%08x %r %s" % (name, kind, style, exstyle, caption, size))
        for c in controls:
            print("   ", c)


if __name__ == "__main__":
    main()
