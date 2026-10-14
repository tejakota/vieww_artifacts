"""Add classes.dex to an APK and set hasCode=true in its compiled manifest.

    python3 apk_add_code.py IN.apk classes.dex OUT.apk

Used by apk-add-code.sh, which explains why. OUT is unsigned and unaligned:
the old signature is dropped (it no longer matches) and the caller zipaligns
and signs.

The manifest inside an APK is Android's binary XML, not text. Flipping a
boolean attribute does not change its size, so this edits the one value in
place instead of recompiling the manifest.
"""

import struct
import sys
import zipfile

RES_STRING_POOL = 0x0001
RES_XML = 0x0003
RES_XML_RESOURCE_MAP = 0x0180
RES_XML_START_ELEMENT = 0x0102
TYPE_INT_BOOLEAN = 0x12
ATTR_HAS_CODE = 0x0101000C  # android.R.attr.hasCode
UTF8_FLAG = 0x100


def _strings(buf, at):
    count, _styles, flags, start, _ = struct.unpack_from("<5I", buf, at + 8)
    offsets = struct.unpack_from("<%dI" % count, buf, at + 28)
    base = at + start
    utf8 = flags & UTF8_FLAG
    out = []
    for off in offsets:
        p = base + off
        if utf8:
            # UTF-16 length, then UTF-8 length; each 1 or 2 bytes.
            for _ in range(2):
                n = buf[p]
                p += 1
                if n & 0x80:
                    n = ((n & 0x7F) << 8) | buf[p]
                    p += 1
            out.append(buf[p:p + n].decode("utf-8", "replace"))
        else:
            n = struct.unpack_from("<H", buf, p)[0]
            p += 2
            if n & 0x8000:
                n = ((n & 0x7FFF) << 16) | struct.unpack_from("<H", buf, p)[0]
                p += 2
            out.append(buf[p:p + 2 * n].decode("utf-16-le", "replace"))
    return out


def set_has_code(manifest: bytes) -> tuple[bytes, str]:
    """Return the manifest with application@hasCode true, and what changed."""
    buf = bytearray(manifest)
    kind, header, _size = struct.unpack_from("<HHI", buf, 0)
    if kind != RES_XML:
        raise ValueError("AndroidManifest.xml is not binary XML (type %#x)" % kind)
    strings, resmap = [], []
    at = header
    while at < len(buf):
        kind, header, size = struct.unpack_from("<HHI", buf, at)
        if size < 8:
            raise ValueError("corrupt chunk at %d" % at)
        if kind == RES_STRING_POOL:
            strings = _strings(buf, at)
        elif kind == RES_XML_RESOURCE_MAP:
            resmap = list(struct.unpack_from("<%dI" % ((size - header) // 4), buf, at + header))
        elif kind == RES_XML_START_ELEMENT:
            body = at + header
            _ns, name, attr_start, attr_size, attr_count = struct.unpack_from("<IIHHH", buf, body)
            if 0 <= name < len(strings) and strings[name] == "application":
                for i in range(attr_count):
                    a = body + attr_start + i * attr_size
                    _ans, aname, _raw, _vsize, _res0, dtype, data = struct.unpack_from("<IIIHBBI", buf, a)
                    rid = resmap[aname] if aname < len(resmap) else None
                    is_has_code = rid == ATTR_HAS_CODE or (
                        rid is None and aname < len(strings) and strings[aname] == "hasCode")
                    if not is_has_code:
                        continue
                    if dtype != TYPE_INT_BOOLEAN:
                        raise ValueError("hasCode is not a boolean (type %#x)" % dtype)
                    if data != 0:
                        return bytes(buf), "hasCode already true"
                    struct.pack_into("<I", buf, a + 16, 0xFFFFFFFF)
                    return bytes(buf), "hasCode false -> true"
                # cargo-apk always writes hasCode; growing the element to add
                # one is not worth supporting until something omits it.
                raise ValueError("<application> has no hasCode attribute to flip")
        at += size
    raise ValueError("no <application> element in the manifest")


def main(src, dex_path, dst):
    dex = open(dex_path, "rb").read()
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w") as zout:
        names = zin.namelist()
        for info in zin.infolist():
            if info.filename.startswith("META-INF/"):
                continue  # the old signature, which no longer matches
            if info.filename == "classes.dex":
                continue  # replaced below; makes a second run harmless
            data = zin.read(info)
            if info.filename == "AndroidManifest.xml":
                data, what = set_has_code(data)
                print("apk_add_code:", what)
            out = zipfile.ZipInfo(info.filename, info.date_time)
            out.compress_type = info.compress_type
            out.external_attr = info.external_attr
            zout.writestr(out, data)
        if "AndroidManifest.xml" not in names:
            raise SystemExit("apk_add_code: %s has no AndroidManifest.xml" % src)
        zout.writestr(zipfile.ZipInfo("classes.dex", (2026, 1, 1, 0, 0, 0)), dex,
                      compress_type=zipfile.ZIP_DEFLATED)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    main(*sys.argv[1:])
